//! Hand-maintained cast metadata.
//!
//! Reflection exposes only *that* a value is a dictionary/array, not its shape.
//! This file stores just those details, keyed by type:
//!
//! - `dict`: a type that is cast from a dictionary with known fields;
//! - `array`: a type that is cast from an array with a single element type;
//! - `tuple`: a type that is cast from an array of fixed length.
//!
//! The tables double as the type registry: `k!` and `t!` register every type
//! they mention, and [`initial_cast`] reads a type's declared input back
//! through that registry (see [`register_key`]).

use std::sync::{LazyLock, Mutex};

use typst::foundations::{
    AutoValue, CastInfo as TyCastInfo, Content, Func, NoneValue, Reflect, Smart, Str,
    Symbol, Type,
};
use typst::layout::{Alignment, BaselinePos, Corners, Length, Margin, Ratio, Rel, Sides};
use typst::math::{Augment, AugmentOffsets, Delimiter, DelimiterPair};
use typst::model::{FirstLineIndent, JustificationLimits, Limits, LinkTarget};
use typst::text::{
    Costs, Covers, FontFamily, FontFeatures, FontList, FontVariations, SmartQuoteDict,
};
use typst::visualize::{
    DashLength, DashPattern, ImageFormat, LineCap, LineJoin, Paint, PixelEncoding, Stroke,
};

use crate::cast::strip_leading_wrappers;
use crate::cast_type::{Cast, Field};
use crate::key::{Key, RustType};
use crate::meta_macros::{f, k, req, t};

// ===========================================================================
// Hand-maintained metadata
// ===========================================================================

/// `(type, known fields, fields of any name)`.
type DictEntry = (RustType, Vec<Field<Key>>, Option<Cast<Key>>);

/// Types that are cast from a dictionary with known fields.
///
/// The third element is what an unknown field may be (`FontFeatures` accepts any
/// integer), or `None` when unknown fields are rejected.
static DICTS: LazyLock<Vec<DictEntry>> = LazyLock::new(|| {
    vec![
        (
            k!(Stroke),
            vec![
                f!("paint", t!(Paint)),
                f!("thickness", t!(Length)),
                f!("cap", t!(LineCap)),
                f!("join", t!(LineJoin)),
                f!("dash", t!(DashPattern)),
                f!("miter-limit", t!(f64)),
            ],
            None,
        ),
        (
            k!(Sides<Option<Option<Stroke>>>),
            vec![
                f!("rest", t!(Option<Stroke>)),
                f!("x", t!(Option<Stroke>)),
                f!("y", t!(Option<Stroke>)),
                f!("left", t!(Option<Stroke>)),
                f!("top", t!(Option<Stroke>)),
                f!("right", t!(Option<Stroke>)),
                f!("bottom", t!(Option<Stroke>)),
            ],
            None,
        ),
        (
            k!(Sides<Option<Rel<Length>>>),
            vec![
                f!("rest", t!(Rel<Length>)),
                f!("x", t!(Rel<Length>)),
                f!("y", t!(Rel<Length>)),
                f!("left", t!(Rel<Length>)),
                f!("top", t!(Rel<Length>)),
                f!("right", t!(Rel<Length>)),
                f!("bottom", t!(Rel<Length>)),
            ],
            None,
        ),
        (
            k!(Corners<Option<Rel<Length>>>),
            vec![
                f!("rest", t!(Rel<Length>)),
                f!("left", t!(Rel<Length>)),
                f!("top", t!(Rel<Length>)),
                f!("right", t!(Rel<Length>)),
                f!("bottom", t!(Rel<Length>)),
                f!("top-left", t!(Rel<Length>)),
                f!("top-right", t!(Rel<Length>)),
                f!("bottom-right", t!(Rel<Length>)),
                f!("bottom-left", t!(Rel<Length>)),
            ],
            None,
        ),
        (
            k!(Margin<Smart<Rel<Length>>>),
            vec![
                f!("rest", t!(Rel<Length>)),
                f!("x", t!(Rel<Length>)),
                f!("y", t!(Rel<Length>)),
                f!("top", t!(Rel<Length>)),
                f!("bottom", t!(Rel<Length>)),
                f!("left", t!(Rel<Length>)),
                f!("right", t!(Rel<Length>)),
                f!("inside", t!(Rel<Length>)),
                f!("outside", t!(Rel<Length>)),
            ],
            None,
        ),
        (
            k!(DashPattern),
            vec![req!("array", t!(Vec<DashLength>)), f!("phase", t!(Length))],
            None,
        ),
        (k!(FirstLineIndent), vec![f!("amount", t!(Length)), f!("all", t!(bool))], None),
        (
            k!(JustificationLimits),
            vec![
                f!("spacing", t!(Limits<Rel<Length>>)),
                f!("tracking", t!(Limits<Length>)),
            ],
            None,
        ),
        (
            k!(Limits<Rel<Length>>),
            vec![req!("min", t!(Rel<Length>)), req!("max", t!(Rel<Length>))],
            None,
        ),
        (
            k!(Limits<Length>),
            vec![req!("min", t!(Length)), req!("max", t!(Length))],
            None,
        ),
        (
            k!(BaselinePos),
            vec![f!("at", t!(Smart<Alignment>)), f!("shift", t!(Rel<Length>))],
            None,
        ),
        (
            k!(Costs),
            vec![
                f!("hyphenation", t!(Ratio)),
                f!("runt", t!(Ratio)),
                f!("widow", t!(Ratio)),
                f!("orphan", t!(Ratio)),
            ],
            None,
        ),
        (k!(FontList), vec![req!("name", t!(Str)), f!("covers", t!(Covers))], None),
        (k!(FontFamily), vec![req!("name", t!(Str)), f!("covers", t!(Covers))], None),
        (k!(FontFeatures), Vec::new(), Some(t!(i64))),
        (k!(FontVariations), Vec::new(), Some(t!(f64))),
        (k!(SmartQuoteDict), vec![f!("double", t!(Str)), f!("single", t!(Str))], None),
        (
            k!(ImageFormat),
            vec![
                req!("encoding", t!(PixelEncoding)),
                req!("width", t!(i64)),
                req!("height", t!(i64)),
            ],
            None,
        ),
        (
            k!(Augment<Length>),
            vec![
                f!("hline", t!(AugmentOffsets)),
                f!("vline", t!(AugmentOffsets)),
                f!("stroke", t!(Stroke)),
            ],
            None,
        ),
        (
            k!(typst::introspection::PagedPosition),
            vec![req!("page", t!(i64)), req!("x", t!(Length)), req!("y", t!(Length))],
            None,
        ),
        (
            k!(LinkTarget),
            vec![req!("page", t!(i64)), req!("x", t!(Length)), req!("y", t!(Length))],
            None,
        ),
    ]
});

/// Types that are cast from an array, and the type of its elements.
static ARRAYS: LazyLock<Vec<(RustType, Cast<Key>)>> = LazyLock::new(|| {
    vec![
        (k!(DashPattern), t!(DashLength)),
        (k!(FontFeatures), t!(Str)),
        (k!(AugmentOffsets), t!(i64)),
        (k!(Vec<DashLength>), t!(DashLength)),
        (k!(FontList), t!(FontFamily)),
    ]
});

/// Types that are cast from a fixed-length array, with one type per position.
static TUPLES: LazyLock<Vec<(RustType, Vec<Cast<Key>>)>> =
    LazyLock::new(|| vec![(k!(DelimiterPair), vec![t!(Delimiter), t!(Delimiter)])]);

/// Implicit coercions implemented in `FromValue`, not visible in `input()`.
///
/// Each entry is a type and the types a value may also come from: `float` can be
/// built from an `integer`, `content` from a `string`, a `symbol` or `none`, and
/// so on.
static COERCIONS: LazyLock<Vec<(RustType, Vec<RustType>)>> = LazyLock::new(|| {
    vec![
        (RustType::of::<f64>(), vec![RustType::of::<i64>()]),
        (RustType::of::<Str>(), vec![RustType::of::<Symbol>()]),
        (
            RustType::of::<Content>(),
            vec![
                RustType::of::<Str>(),
                RustType::of::<Symbol>(),
                RustType::of::<NoneValue>(),
            ],
        ),
        (RustType::of::<Func>(), vec![RustType::of::<Type>(), RustType::of::<Symbol>()]),
        (
            RustType::of::<Rel<Length>>(),
            vec![RustType::of::<Length>(), RustType::of::<Ratio>()],
        ),
    ]
});

/// Types typst has but binds in no scope.
///
/// Walking the library never meets them, yet `Option` and `Smart` refer to them
/// (`none`, `auto`), so they take part in the type maps like any other named
/// type.
static UNBOUND: LazyLock<Vec<Type>> =
    LazyLock::new(|| vec![Type::of::<NoneValue>(), Type::of::<AutoValue>()]);

// ===========================================================================
// The tables as a type registry
// ===========================================================================

/// A type mentioned by the metadata below.
struct Declared {
    /// The normalized Rust type key.
    rust_type: RustType,
    /// How to read the type's declared input.
    input: fn() -> TyCastInfo,
}

/// Every type reachable from the metadata, filled by `k!` and `t!`.
static DECLARED: Mutex<Vec<Declared>> = Mutex::new(Vec::new());

/// Register a type mentioned in the metadata and return its normalized key.
///
/// Called by `k!` (the type whose shape is described) and `t!` (a type used as
/// a field/element), so the tables double as the type registry.
pub fn register_key<T: Reflect + ?Sized>() -> RustType {
    let rust_type = RustType::new(std::any::type_name::<T>());
    let mut declared = DECLARED.lock().unwrap_or_else(|e| e.into_inner());
    if !declared.iter().any(|d| d.rust_type == rust_type) {
        declared.push(Declared {
            rust_type: rust_type.clone(),
            input: <T as Reflect>::input,
        });
    }
    rust_type
}

/// A type reference to a registered type (see [`register_key`]).
pub fn register_type<T: Reflect + ?Sized>() -> Cast<Key> {
    Cast::Type(Key::Rust(register_key::<T>()))
}

/// The declared input of a registered type, before refinement.
///
/// `None` means the type was never mentioned by the metadata, so there is no
/// declared input to start from; `cast::refine` then builds the row from the
/// tables alone (and yields nothing when they are silent too).
pub fn initial_cast(rust_type: &RustType) -> Option<Cast<Key>> {
    LazyLock::force(&DICTS);
    LazyLock::force(&ARRAYS);
    LazyLock::force(&TUPLES);
    let declared = {
        let declared = DECLARED.lock().unwrap_or_else(|e| e.into_inner());
        declared.iter().find(|d| d.rust_type == *rust_type).map(|d| d.input)
    };
    declared.map(|input| Cast::from(&(input)()).into_unresolved())
}

// ===========================================================================
// Lookups
// ===========================================================================

fn matches(key: &RustType, candidate: &RustType) -> bool {
    strip_leading_wrappers(key.as_str()) == strip_leading_wrappers(candidate.as_str())
}

pub fn dict(key: &RustType) -> Option<&'static [Field<Key>]> {
    DICTS
        .iter()
        .find(|(candidate, _, _)| matches(key, candidate))
        .map(|(_, fields, _)| fields.as_slice())
}

pub fn dict_rest(key: &RustType) -> Option<Cast<Key>> {
    DICTS
        .iter()
        .find(|(candidate, _, _)| matches(key, candidate))
        .and_then(|(_, _, rest)| rest.clone())
}

pub fn array(key: &RustType) -> Option<&'static Cast<Key>> {
    ARRAYS
        .iter()
        .find(|(candidate, _)| matches(key, candidate))
        .map(|(_, elem)| elem)
}

pub fn tuple(key: &RustType) -> Option<&'static [Cast<Key>]> {
    TUPLES
        .iter()
        .find(|(candidate, _)| matches(key, candidate))
        .map(|(_, elems)| elems.as_slice())
}

/// Whether any metadata exists for this type.
pub fn has_meta(key: &RustType) -> bool {
    dict(key).is_some()
        || dict_rest(key).is_some()
        || array(key).is_some()
        || tuple(key).is_some()
}

/// The hand-maintained list at the top of this file.
pub fn coercions() -> &'static [(RustType, Vec<RustType>)] {
    &COERCIONS
}

/// The hand-maintained list at the top of this file.
pub fn unbound_types() -> &'static [Type] {
    &UNBOUND
}
