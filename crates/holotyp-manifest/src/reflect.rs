//! Reflection: walks a built `Library` and produces a [`Manifest`].
//!
//! All knowledge about typst's public API lives in this file (and `lib.rs`'s library
//! construction). When upstream drifts, this is the only file to fix (see the rebase
//! playbook in the workspace docs).

use std::collections::BTreeMap;

use ecow::EcoString;
use typst::foundations::{
    Binding, Func, Module, ParamInfo, Repr, Scope, Symbol, Type, Value,
};

use crate::cast_type::Cast;
use crate::key::{CanonicalKey, RustType};
use crate::schema::*;

/// Reflect an entire library.
pub fn reflect_library(
    lib: &typst::Library,
    guard: &typst::foundations::SilentBindingGuard,
    build: Build,
) -> Manifest {
    let features = feature_names(lib);
    let formats = format_names(lib);
    let (casts, param_keys) = crate::cast::build(lib, guard);
    let global_meta = Node {
        value_type: Some(Type::of::<Module>().long_name().to_string()),
        ..Default::default()
    };
    let global = reflect_module("global", &lib.global, guard, &param_keys, global_meta);

    Manifest {
        schema_version: Manifest::SCHEMA_VERSION,
        build,
        features,
        formats,
        global,
        casts,
    }
}

fn feature_names(lib: &typst::Library) -> Vec<String> {
    use typst::Feature;
    Feature::all()
        .filter(|f| lib.features.is_enabled(*f))
        .map(|f| f.to_string())
        .collect()
}

fn format_names(lib: &typst::Library) -> Vec<String> {
    lib.formats.iter().map(|f| f.elem.name().to_string()).collect()
}

/// Walk a scope into a name → node map.
fn reflect_scope_into(
    map: &mut BTreeMap<String, Node>,
    scope: &Scope,
    guard: &typst::foundations::SilentBindingGuard,
    keys: &BTreeMap<RustType, CanonicalKey>,
) {
    for (name, binding) in scope.iter() {
        if let Some(node) = reflect_binding(name, binding, guard, keys) {
            map.insert(name.to_string(), node);
        }
    }
}

/// The fields every node shares, read from its binding and value.
fn node_meta(binding: &Binding, value: &Value) -> Node {
    Node {
        title: binding.title().map(str::to_string),
        docs: binding.docs().map(str::to_string),
        deprecation: binding.deprecation().map(|d| Deprecation {
            message: d.message().to_string(),
            until: d.until().map(str::to_string),
        }),
        value_type: Some(value.ty().long_name().to_string()),
        ..Default::default()
    }
}

fn reflect_module(
    name: &str,
    module: &Module,
    guard: &typst::foundations::SilentBindingGuard,
    keys: &BTreeMap<RustType, CanonicalKey>,
    meta: Node,
) -> Node {
    let mut map = BTreeMap::new();
    reflect_scope_into(&mut map, module.scope(), guard, keys);
    Node {
        kind: NodeKind::Module,
        name: name.to_string(),
        scope: if map.is_empty() { None } else { Some(Box::new(map)) },
        ..meta
    }
}

fn reflect_binding(
    name: &EcoString,
    binding: &Binding,
    guard: &typst::foundations::SilentBindingGuard,
    keys: &BTreeMap<RustType, CanonicalKey>,
) -> Option<Node> {
    let value: Value = binding.read(guard).ok()?.clone();
    let meta = node_meta(binding, &value);

    Some(match &value {
        Value::Func(func) => reflect_func(name, func, guard, keys, meta),
        Value::Type(ty) => reflect_type(*ty, guard, keys, meta),
        Value::Module(module) => reflect_module(name, module, guard, keys, meta),
        Value::Symbol(symbol) => reflect_symbol(name, symbol, meta),
        _ => reflect_value(name, &value, meta),
    })
}

fn reflect_func(
    name: &str,
    func: &Func,
    guard: &typst::foundations::SilentBindingGuard,
    keys: &BTreeMap<RustType, CanonicalKey>,
    meta: Node,
) -> Node {
    let mut params = Vec::new();
    for p in func.params() {
        params.push(reflect_param(&p, keys));
    }
    let returns = func.returns().map(|info| Cast::from(info).to_schema());
    let element = func.to_element().is_some();
    let scope = func.scope().map(|s| {
        let mut map = BTreeMap::new();
        reflect_scope_into(&mut map, s, guard, keys);
        map
    });
    let func_name = func.name().unwrap_or(name);
    Node {
        kind: NodeKind::Function,
        name: func_name.to_string(),
        scope: scope.filter(|m| !m.is_empty()).map(Box::new),
        params,
        returns,
        contextual: func.contextual().unwrap_or(false),
        element,
        ..meta
    }
}

fn reflect_param(p: &ParamInfo, keys: &BTreeMap<RustType, CanonicalKey>) -> Param {
    let native = p.to_native().expect("Parameter is not native!");
    let default = native.default.map(|f| f().repr().to_string());
    let mut input = Cast::from(&native.input);

    // Replace dict & array with the query key of prebuilt cast table
    if let Some(key) = keys.get(&RustType::new(native.rust_type_raw)) {
        input = crate::cast::rewrite_param(input, key);
    }
    Param {
        name: p.name().unwrap_or("").to_string(),
        docs: (!native.docs.is_empty()).then(|| native.docs.to_string()),
        input: input.to_schema(),
        default,
        positional: p.positional(),
        named: p.named(),
        variadic: p.variadic(),
        required: p.required(),
        settable: p.settable(),
    }
}

fn reflect_type(
    ty: Type,
    guard: &typst::foundations::SilentBindingGuard,
    keys: &BTreeMap<RustType, CanonicalKey>,
    meta: Node,
) -> Node {
    let mut map = BTreeMap::new();
    reflect_scope_into(&mut map, ty.scope(), guard, keys);

    let mut params = Vec::new();
    if let Ok(ctor) = ty.constructor() {
        let ctor_scope = ctor.scope().map(|s| s.iter().count()).unwrap_or(0);
        assert_eq!(
            ctor_scope, 0,
            "Constructor of `{ty}` has an unexpected non-empty scope!"
        );
        for p in ctor.params() {
            params.push(reflect_param(&p, keys));
        }
    }

    // Fields read off a value of the type (`length.abs`, `version.major`). Only a handful
    // of types have them; they are not part of the scope.
    let fields = typst::foundations::fields_on(ty)
        .iter()
        .map(|field| {
            let output = (field.output)();
            ValueField {
                name: field.name.to_string(),
                output: Cast::from(&output).to_schema(),
            }
        })
        .collect();

    Node {
        kind: NodeKind::Type,
        name: ty.long_name().to_string(),
        scope: if map.is_empty() { None } else { Some(Box::new(map)) },
        params,
        fields,
        ..meta
    }
}

fn reflect_symbol(name: &str, symbol: &Symbol, meta: Node) -> Node {
    fn is_callable(glyph: &str) -> bool {
        typst::math::accent::get_accent_func(glyph).is_some()
            || typst::math::get_lr_wrapper_func(glyph).is_some()
    }

    let raw: Vec<_> = symbol.variants().collect();
    let default = raw
        .iter()
        .enumerate()
        .min_by_key(|(i, (modifiers, _, _))| (modifiers.iter().count(), *i))
        .map(|(i, _)| i);
    let variants = raw
        .iter()
        .enumerate()
        .map(|(i, (modifiers, value, deprecation))| {
            let mut set: Vec<String> = modifiers.iter().map(str::to_string).collect();
            set.sort();
            SymbolVariant {
                symbol: value.to_string(),
                modifiers: set,
                callable: is_callable(value),
                default: Some(i) == default,
                deprecation: deprecation.map(str::to_string),
            }
        })
        .collect();
    Node {
        kind: NodeKind::Symbol,
        name: name.to_string(),
        variants,
        ..meta
    }
}

fn reflect_value(name: &str, value: &Value, meta: Node) -> Node {
    Node {
        kind: NodeKind::Value,
        name: name.to_string(),
        repr: Some(value.repr().to_string()),
        ..meta
    }
}
