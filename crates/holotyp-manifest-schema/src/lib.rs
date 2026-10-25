//! Manifest schema
//!
//! Serialization rules:
//! - `None`/empty optional fields are **omitted**, never `null`.
//! - Boolean flags that default to `false` are omitted when `false`
//!   (`skip_serializing_if`): `contextual`, `element`, ...
//! - Unknown fields are ignored on load.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Top-level manifest structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// The schema version of this format. Currently 1.
    pub schema_version: u32,
    /// Identity of the Typst build whose library was reflected.
    pub build: Build,
    /// Features that were enabled while reflecting (e.g. `html`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Export formats registered in the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<String>,
    /// The global module: every built-in name, including the `math` and `calc` modules as
    /// their own entries.
    pub global: Node,
    /// How each type can be cast from, keyed by type: typst's long name for named types
    /// (`stroke`), or a structural type's Rust name for types that typst does not name
    /// (`…::DashPattern`).
    ///
    /// Rows reference each other by [`CastInfo::Type`], and structural details are
    /// recorded once per type.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub casts: BTreeMap<String, CastInfo>,
}

/// Identity of the reflected Typst build (fork name, version, git revision).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    /// Fork name from `crate::fork::FORK_NAME`; `"typst"` means upstream.
    pub name: String,
    /// Typst version (`typst::utils::version().raw()`).
    pub version: String,
    /// Git commit of the build, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// Stable map for scopes (ordered by name). Avoids pulling indexmap in: serialized form
/// is a JSON object anyway.
pub mod indexmap_like {
    #[allow(unused_imports)]
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;

    /// Names mapped to nodes. `BTreeMap` for deterministic output.
    #[allow(unused_imports)]
    pub type ScopeMap = BTreeMap<String, super::Node>;
}

/// A node in the module tree.
///
/// `Default` is an empty node: reflection builds the kind-specific fields and takes the
/// rest from a `..` update.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub kind: NodeKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Raw Typst-markup documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<Deprecation>,
    /// Nested scope: submodules (module), field/method table (type), element field scope
    /// (function).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Box<indexmap_like::ScopeMap>>,
    /// Parameters of a function, or of a type's constructor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    /// For `type` nodes: fields readable off a value of the type, e.g. `length.abs` or
    /// `version.major`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ValueField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<CastInfo>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub contextual: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub element: bool,
    /// For `symbol` nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<SymbolVariant>,
    /// For `value` nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_type: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    #[default]
    Module,
    Function,
    Type,
    Value,
    Symbol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deprecation {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
}

/// Function parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    pub input: CastInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub positional: bool,
    pub named: bool,
    pub variadic: bool,
    pub required: bool,
    pub settable: bool,
}

/// What values a type accepts: typst's `foundations::CastInfo` shapes (`any`, `value`,
/// `type`, `union`) plus the structural detail reflection erases (`dict`, `array`,
/// `tuple`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CastInfo {
    Any,
    Value {
        value: String,
        #[serde(rename = "type")]
        ty: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
    Type {
        #[serde(rename = "type")]
        ty: String,
    },
    Union {
        of: Vec<CastInfo>,
    },
    /// Casting from a dictionary with known fields.
    Dict {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fields: Vec<DictField>,
        /// Additional entries of any name that are accepted, e.g. HTML attributes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rest: Option<Box<CastInfo>>,
    },
    /// Casting from an array of variable length with a single element type.
    Array {
        elem: Box<CastInfo>,
    },
    /// Casting from an array of fixed length with positional element types.
    Tuple {
        elems: Vec<CastInfo>,
    },
}

/// One field readable off a value of a [`Node`] of kind `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueField {
    pub name: String,
    /// What the field's value can be cast into.
    pub output: CastInfo,
}

/// One field of a [`CastInfo::Dict`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictField {
    pub name: String,
    pub input: CastInfo,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
}

/// Symbol variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolVariant {
    pub symbol: String,
    /// The variant's modifiers as a multiset, normalized to sorted order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<String>,
    /// Whether the glyph is callable, i.e. maps to an accent or an L/R wrapper function
    /// (typst's `Symbol::func`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub callable: bool,
    /// Whether the bare name resolves to this variant.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<String>,
}

impl Manifest {
    /// The schema version this crate emits and understands.
    pub const SCHEMA_VERSION: u32 = 1;
}
