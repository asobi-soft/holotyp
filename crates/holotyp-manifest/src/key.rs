//! Type keys.
//!
//! A cast-table row is named by a *type key*, and there are exactly two kinds:
//! typst's long name for a type typst names (`stroke`), and the normalized Rust
//! type name for a structural type (`…::DashPattern`). The types below make
//! that the only choice available: [`RustType`] can only be built through
//! normalization, and [`Key`] is one kind or the other.
//!
//! Rows are built with these keys and only spell them as plain strings at the
//! manifest boundary; [`CanonicalKey::new`] is the one place a Rust key turns
//! into typst's long name.

use std::collections::BTreeMap;
use std::fmt;

use typst::foundations::Type;

use crate::cast::normalize;

/// A Rust type name in normalized form.
///
/// The field is private and every constructor normalizes, so a value can never
/// hold a raw [`std::any::type_name`] string.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RustType(String);

impl RustType {
    /// Normalize a raw Rust type name.
    pub fn new(raw: &str) -> Self {
        Self(normalize(raw))
    }

    /// Normalize the name of `T`.
    pub fn of<T: ?Sized>() -> Self {
        Self::new(std::any::type_name::<T>())
    }

    /// The normalized name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The normalized name, owned.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<&str> for RustType {
    fn from(raw: &str) -> Self {
        Self::new(raw)
    }
}

impl fmt::Display for RustType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The key of a cast-table row.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// typst's long name of a type typst names.
    Typst(String),
    /// The normalized Rust type name of a structural type.
    Rust(RustType),
}

impl Key {
    /// The key of a named type.
    pub fn typst(long_name: &str) -> Self {
        Self::Typst(long_name.to_string())
    }

    /// The key of a structural type.
    pub fn rust(raw: &str) -> Self {
        Self::Rust(RustType::new(raw))
    }

    /// The Rust type behind this key.
    ///
    /// A typst name without one (`none`, `auto`) has no shape to discover.
    pub fn rust_type(&self, to_rust: &BTreeMap<String, RustType>) -> Option<RustType> {
        match self {
            Self::Typst(long_name) => to_rust.get(long_name).cloned(),
            Self::Rust(rust_type) => Some(rust_type.clone()),
        }
    }

    /// The key as written in a row.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Typst(long_name) => long_name,
            Self::Rust(rust_type) => rust_type.as_str(),
        }
    }

    /// The key as written in a row, owned.
    pub fn into_string(self) -> String {
        match self {
            Self::Typst(long_name) => long_name,
            Self::Rust(rust_type) => rust_type.into_string(),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A [`Key`] that has already been resolved: typst's long name of a named type,
/// or the Rust type of a type typst does not name.
///
/// [`CanonicalKey::new`] is the only constructor, so a value of this type is
/// always safe to store as a row key or to point at with a reference.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalKey(Key);

/// No names, for the one kind of key that needs no lookup: typst's own.
static NO_NAMES: BTreeMap<RustType, String> = BTreeMap::new();

impl CanonicalKey {
    /// Resolve a key: a Rust type becomes typst's long name when the library
    /// has one for it.
    ///
    /// The only constructor. Metadata is written in Rust types while the
    /// library is the only source of names, so this is where a key becomes
    /// usable as a row key or a reference.
    pub fn new(key: Key, to_typ: &BTreeMap<RustType, String>) -> Self {
        Self(match key {
            Key::Rust(rust_type) => match to_typ.get(&rust_type) {
                Some(long_name) => Key::Typst(long_name.clone()),
                None => Key::Rust(rust_type),
            },
            Key::Typst(long_name) => Key::Typst(long_name),
        })
    }

    /// The key of a Rust type: typst's long name when it has one, otherwise the
    /// Rust type itself.
    pub fn from_rust(rust_type: &RustType, to_typ: &BTreeMap<RustType, String>) -> Self {
        Self::new(Key::Rust(rust_type.clone()), to_typ)
    }

    /// The key of a typst type.
    ///
    /// Its name is typst's own, so it is canonical without a lookup.
    pub fn of_type(ty: &Type) -> Self {
        Self::new(Key::typst(ty.long_name()), &NO_NAMES)
    }

    /// The key as it is spelled in a row.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The key as it is spelled in a row, owned.
    pub fn into_string(self) -> String {
        self.0.into_string()
    }

    /// The key without the guarantee, for a row that goes back through
    /// refinement.
    pub fn into_key(self) -> Key {
        self.0
    }

    /// The Rust type behind this key (see [`Key::rust_type`]).
    pub fn rust_type(&self, to_rust: &BTreeMap<String, RustType>) -> Option<RustType> {
        self.0.rust_type(to_rust)
    }
}

impl fmt::Display for CanonicalKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
