//! Cast-graph construction.
//!
//! The matrix is built from typst's *declared* cast information (`Type::input` /
//! `Reflect::input`), which is value-independent and complete by construction.
//! Value-level behaviour (dictionary keys, array element validation, string parsing) is
//! refined with the hand-maintained tables in [`crate::meta`].
//!
//! The graph is computed to a fixpoint: whenever a type is discovered, its `input()` is
//! read; each `dictionary`/`array`/`string` member is refined with metadata when
//! available (otherwise it stays general). Refinements can introduce new types (e.g.
//! `stroke`'s `dash` field is a `DashPattern`), which are then processed the same way.
//!
//! Rows are built as [`Cast`], whose type references are [`Key`]s: typst's long name for
//! named types (`stroke`), the normalized Rust type for structural types typst has no
//! name for (`…::DashPattern`). Only the manifest boundary turns them into the strings of
//! [`CastInfo`].

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use typst::foundations::{Array, Dict, Module, NativeType, Scope, Type, Value};

use typst::foundations::Smart;

use crate::cast_type::Cast;
use crate::key::{CanonicalKey, Key, RustType};
use crate::meta;
use crate::rust_type;
use crate::schema::CastInfo;

/// Normalize a type name for key comparison.
///
/// Only `Arc` is removed: its `Reflect` implementation delegates to the inner type, so it
/// does not change the accepted set, yet `type_name` spells it out for some values of a
/// type and not for others (`Option<Arc<Stroke>>` vs `Option<Stroke>`), which would
/// otherwise split one type into two keys.
///
/// `Option` and `Smart` add `none`/`auto` to the accepted set, so they stay part of the
/// key. The wrapper is matched by its full path, so a user-defined type that merely ends
/// in `Arc` is left alone.
pub fn normalize(ty: &str) -> String {
    let arc = rust_type::wrapper_prefix::<std::sync::Arc<()>>();
    rust_type::RustType::parse(ty).strip_all(arc)
}

/// Strip leading `Option`/`Smart`/`Arc` wrappers, for metadata lookup.
///
/// The structural shape of a dictionary/array belongs to the inner type, while the
/// wrappers only add `none`/`auto` as whole-value alternatives (which the row itself
/// already carries through `Type::input`).
pub fn strip_leading_wrappers(key: &str) -> String {
    let wrappers = [
        rust_type::wrapper_prefix::<Option<()>>(),
        rust_type::wrapper_prefix::<Smart<()>>(),
        rust_type::wrapper_prefix::<std::sync::Arc<()>>(),
    ];
    rust_type::RustType::parse(&normalize(key))
        .strip_leading(&wrappers)
        .to_string()
}

/// Build the cast table and the parameter-shape index.
///
/// Returns `(casts, cast_keys)`:
///
/// - `casts`: the manifest rows, with every key spelled as a string ([`CanonicalKey`]:
///   typst's long name when the type has one, otherwise the normalized Rust type).
/// - `cast_keys`: [`RustType`] -> the [`CanonicalKey`] a parameter of that type refers
///   to.
pub fn build(
    lib: &typst::Library,
) -> (BTreeMap<String, CastInfo>, BTreeMap<RustType, CanonicalKey>) {
    let mut casts: BTreeMap<CanonicalKey, Cast<CanonicalKey>> = BTreeMap::new();

    // collect [(rust_type, casts)] from func params, as init of `casts`
    let mut params = Vec::new();
    collect_casts_from_params(&lib.global, &mut params);

    // collect [(rust_type, typst type, casts)] of named types
    let named = collect_named_types(&lib.global);

    // build `rust_type <-> typst long name` map and initial casts
    let mut to_typ = BTreeMap::new();
    let mut to_rust = BTreeMap::new();
    for (rust_type, ty, cast) in named {
        let long_name = ty.long_name().to_string();
        // todo: also use types instead of strings?
        to_typ.insert(rust_type.clone(), long_name.clone());
        to_rust.insert(long_name, rust_type.clone());
        add_row(&mut casts, CanonicalKey::of_type(&ty), cast, &rust_type, &to_typ);
    }

    // build `cast_keys` and init `casts` todo: why casts init twice?
    let mut cast_keys = BTreeMap::new();
    for (rust_type, cast) in &params {
        if meta::has_meta(rust_type) {
            let key = CanonicalKey::from_rust(rust_type, &to_typ);
            cast_keys.insert(rust_type.clone(), key.clone());
            add_row(&mut casts, key, cast.clone(), rust_type, &to_typ);
        }
    }

    // iter over discovered keys to find fixpoint
    let mut queue: VecDeque<CanonicalKey> = casts.keys().cloned().collect();
    let mut visited = BTreeSet::new();
    while let Some(key) = queue.pop_front() {
        if !visited.insert(key.clone()) {
            continue;
        }
        // Seeded rows are refined already; a discovered one is built here.
        let row = match casts.get(&key) {
            Some(row) => row.clone(),
            None => {
                let rust_type = rust_type_of(&key, &to_rust);
                // `None` here means "no declared input": the tables alone can still
                // describe the row.
                let Some(refined) = refine(meta::initial_cast(&rust_type), &rust_type)
                else {
                    // Nothing describes it, neither typst nor the tables.
                    continue;
                };
                refined.resolve(&to_typ)
            }
        };
        casts.insert(key.clone(), row.clone());

        for ty in referenced_types(&row) {
            if !casts.contains_key(&ty) && !visited.contains(&ty) {
                queue.push_back(ty);
            }
        }
    }

    // Implicit coercions from `FromValue`, invisible in reflection.
    for (target, sources) in meta::coercions() {
        let key = CanonicalKey::from_rust(target, &to_typ);
        let row = casts.entry(key.clone()).or_insert_with(|| Cast::Type(key.clone()));
        for source in sources {
            row.add_member(Cast::Type(CanonicalKey::from_rust(source, &to_typ)));
        }
    }

    // convert into manifest format, they use strings
    let casts = casts
        .into_iter()
        .map(|(key, cast)| (key.into_string(), cast.to_schema()))
        .collect();
    (casts, cast_keys)
}

/// The Rust type behind a key.
fn rust_type_of(key: &CanonicalKey, to_rust: &BTreeMap<String, RustType>) -> RustType {
    key.rust_type(to_rust).unwrap_or_else(|| {
        panic!(
            "key `{key}` has no Rust type: typst names the type but binds it in \
             no scope, so its input cannot be read. Add it to `to_rust` in \
             `build`, next to `NoneValue` and `AutoValue`."
        )
    })
}

/// Refine a row once and store it resolved.
fn add_row(
    casts: &mut BTreeMap<CanonicalKey, Cast<CanonicalKey>>,
    key: CanonicalKey,
    cast: Cast<CanonicalKey>,
    rust_type: &RustType,
    to_typ: &BTreeMap<RustType, String>,
) {
    if casts.contains_key(&key) {
        return;
    }
    if let Some(refined) = refine(Some(cast.into_unresolved()), rust_type) {
        casts.insert(key, refined.resolve(to_typ));
    }
}

/// Collect (`rust_type`, reflected input) for every function/element parameter in the
/// library.
fn collect_casts_from_params(
    root: &Module,
    out: &mut Vec<(RustType, Cast<CanonicalKey>)>,
) {
    for (_, binding) in root.scope().iter() {
        match binding.read() {
            Value::Func(func) => {
                for param in func.params() {
                    if let Some(native) = param.to_native() {
                        out.push((
                            RustType::new(native.rust_type_raw),
                            Cast::from(&native.input),
                        ));
                    }
                }
                if let Some(scope) = func.scope() {
                    collect_params_scope(scope, out);
                }
            }
            Value::Module(module) => collect_casts_from_params(module, out),
            _ => {}
        }
    }
}

// todo: generalize with collect_params
fn collect_params_scope(scope: &Scope, out: &mut Vec<(RustType, Cast<CanonicalKey>)>) {
    for (_, binding) in scope.iter() {
        if let Value::Func(func) = binding.read() {
            for param in func.params() {
                if let Some(native) = param.to_native() {
                    out.push((
                        RustType::new(native.rust_type_raw),
                        Cast::from(&native.input),
                    ));
                }
            }
            if let Some(scope) = func.scope() {
                collect_params_scope(scope, out);
            }
        }
    }
}

/// Collect every named type of the library, as `(rust type, typst type, cast)`.
fn collect_named_types(root: &Module) -> Vec<(RustType, Type, Cast<CanonicalKey>)> {
    let mut out = Vec::new();
    walk_named_types(root, &mut out);
    // typst binds neither `none` nor `auto` in a scope, so the walk cannot see them; they
    // are still named types and referenced by rows. Added here, once, rather than in the
    // recursive step.
    for ty in meta::unbound_types() {
        out.push((RustType::new(ty.rust_type_raw()), *ty, Cast::from(&ty.input())));
    }
    out
}

fn walk_named_types(
    module: &Module,
    out: &mut Vec<(RustType, Type, Cast<CanonicalKey>)>,
) {
    for (_, binding) in module.scope().iter() {
        match binding.read() {
            Value::Type(ty) => {
                out.push((
                    RustType::new(ty.rust_type_raw()),
                    *ty,
                    Cast::from(&ty.input()),
                ));
            }
            Value::Module(module) => walk_named_types(module, out),
            _ => {}
        }
    }
}

/// Replace `dictionary`/`array` members with metadata when available, and add whatever
/// the metadata describes but the declaration does not mention.
fn refine(row: Option<Cast<Key>>, rust_type: &RustType) -> Option<Cast<Key>> {
    // dict, arr
    let mut seen = [false; 2];
    let row = row.map(|row| replace_placeholders(row, rust_type, &mut seen));

    // add if not seen
    let mut extra = Vec::new();
    if !seen[0]
        && let Some(fields) = meta::dict(rust_type)
    {
        extra.push(Cast::Dict {
            fields: fields.to_vec(),
            rest: meta::dict_rest(rust_type).map(Box::new),
        });
    }

    if !seen[1] {
        if let Some(elems) = meta::tuple(rust_type) {
            extra.push(Cast::Tuple { elems: elems.to_vec() });
        } else if let Some(elem) = meta::array(rust_type) {
            extra.push(Cast::Array { elem: Box::new(elem.clone()) });
        }
    }
    // nothing pushed, return replaced
    if extra.is_empty() {
        return row;
    }

    // build result
    let mut of = match row {
        Some(Cast::Union { of }) => of,
        Some(other) => vec![other],
        None => Vec::new(),
    };
    for member in extra {
        let repr = format!("{member:?}");
        if !of.iter().any(|m| format!("{m:?}") == repr) {
            of.push(member);
        }
    }
    Some(match of.len() {
        1 => of.pop().unwrap(),
        _ => Cast::Union { of },
    })
}

/// typst's long name for a type
fn long_name<T: NativeType>() -> &'static str {
    Type::of::<T>().long_name()
}

/// Replace the placeholders `dictionary` and `array`, recording which ones the row
/// mentioned in `seen`.
fn replace_placeholders(
    row: Cast<Key>,
    key: &RustType,
    seen: &mut [bool; 2],
) -> Cast<Key> {
    match row {
        Cast::Union { of } => Cast::Union {
            of: of.into_iter().map(|m| replace_placeholders(m, key, seen)).collect(),
        },
        Cast::Type(Key::Typst(ty)) if ty == long_name::<Dict>() => {
            seen[0] = true;
            match meta::dict(key) {
                Some(fields) => Cast::Dict {
                    fields: fields.to_vec(),
                    rest: meta::dict_rest(key).map(Box::new),
                },
                None => Cast::Type(Key::Typst(ty)),
            }
        }
        Cast::Type(Key::Typst(ty)) if ty == long_name::<Array>() => {
            seen[1] = true;
            if let Some(elems) = meta::tuple(key) {
                Cast::Tuple { elems: elems.to_vec() }
            } else if let Some(elem) = meta::array(key) {
                Cast::Array { elem: Box::new(elem.clone()) }
            } else {
                Cast::Type(Key::Typst(ty))
            }
        }
        other => other,
    }
}

/// All type keys referenced by a cast description.
fn referenced_types(info: &Cast<CanonicalKey>) -> Vec<CanonicalKey> {
    let mut out = Vec::new();
    // todo: i want more walkers actually, many functions here can be rewritten in the
    // walker's way
    fn walk(info: &Cast<CanonicalKey>, out: &mut Vec<CanonicalKey>) {
        match info {
            Cast::Type(key) => out.push(key.clone()),
            Cast::Union { of } => of.iter().for_each(|cast| walk(cast, out)),
            Cast::Dict { fields, rest } => {
                fields.iter().for_each(|field| walk(&field.input, out));
                if let Some(rest) = rest {
                    walk(rest, out);
                }
            }
            Cast::Array { elem } => walk(elem, out),
            Cast::Tuple { elems } => elems.iter().for_each(|cast| walk(cast, out)),
            _ => {}
        }
    }
    walk(info, &mut out);
    out
}

/// Replace the structural members of a parameter input with its type key.
pub fn rewrite_param(
    input: Cast<CanonicalKey>,
    key: &CanonicalKey,
) -> Cast<CanonicalKey> {
    let reference = Cast::Type(key.clone());
    fn rewrite(
        info: Cast<CanonicalKey>,
        reference: &Cast<CanonicalKey>,
    ) -> Cast<CanonicalKey> {
        match info {
            Cast::Union { of } => Cast::Union {
                of: of.into_iter().map(|m| rewrite(m, reference)).collect(),
            },
            Cast::Type(key)
                if key.as_str() == long_name::<Dict>()
                    || key.as_str() == long_name::<Array>() =>
            {
                reference.clone()
            }
            other => other,
        }
    }
    let out = rewrite(input, &reference);
    // Collapse duplicates.
    if let Cast::Union { of } = &out {
        let mut seen = BTreeSet::new();
        let mut members = Vec::new();
        for member in of {
            let repr = format!("{member:?}");
            if seen.insert(repr) {
                members.push(member.clone());
            }
        }
        return Cast::Union { of: members };
    }
    out
}
