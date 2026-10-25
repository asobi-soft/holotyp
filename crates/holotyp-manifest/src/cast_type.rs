//! The tree of values a type accepts.
//!
//! This is the working representation of a cast row before it becomes the manifest's
//! [`CastInfo`]. Two differences from the schema type: a type reference is a key instead
//! of a string, and dictionary fields are [`Field`]s.
//!
//! The key type parameter says which stage the row is in: [`Key`] while metadata is still
//! being applied, [`CanonicalKey`] once every reference is resolved (see
//! [`Cast::<Key>::resolve`]). Strings only appear when the manifest is assembled
//! ([`Cast::<CanonicalKey>::to_schema`]).

use std::collections::BTreeMap;

use typst::foundations::{CastInfo as TyCastInfo, Repr};

use crate::key::{CanonicalKey, Key, RustType};
use crate::schema::{CastInfo, DictField};

#[derive(Debug, Clone, PartialEq)]
pub enum Cast<K> {
    Any,
    Value { value: String, ty: K, details: Option<String> },
    Type(K),
    Union { of: Vec<Cast<K>> },
    Dict { fields: Vec<Field<K>>, rest: Option<Box<Cast<K>>> },
    Array { elem: Box<Cast<K>> },
    Tuple { elems: Vec<Cast<K>> },
}

/// One field of a [`Cast::Dict`].
#[derive(Debug, Clone, PartialEq)]
pub struct Field<K> {
    pub name: String,
    pub input: Cast<K>,
    pub required: bool,
}

impl From<&TyCastInfo> for Cast<CanonicalKey> {
    fn from(info: &TyCastInfo) -> Self {
        match info {
            TyCastInfo::Any => Cast::Any,
            TyCastInfo::Value(value, details) => Cast::Value {
                value: value.repr().to_string(),
                ty: CanonicalKey::of_type(&value.ty()),
                details: if details.is_empty() {
                    None
                } else {
                    Some(details.to_string())
                },
            },
            TyCastInfo::Type(ty) => Cast::Type(CanonicalKey::of_type(ty)),
            TyCastInfo::Union(of) => {
                Cast::Union { of: of.iter().map(Cast::from).collect() }
            }
        }
    }
}

impl<K: PartialEq> Cast<K> {
    /// Add `member` as one more accepted alternative.
    ///
    /// A union is extended (without duplicates); a single shape becomes a union with it.
    /// Adding what is already the whole set does nothing.
    pub fn add_member(&mut self, member: Cast<K>) {
        match self {
            Cast::Union { of } => {
                if !of.contains(&member) {
                    of.push(member);
                }
            }
            other if *other == member => {}
            other => {
                let old = std::mem::replace(other, Cast::Any);
                *other = Cast::Union { of: vec![old, member] };
            }
        }
    }
}

impl Cast<Key> {
    /// Resolve every reference: a Rust key becomes typst's long name when the library has
    /// one for it.
    ///
    /// Metadata is written in Rust types, so this is the one rewrite a row needs before
    /// it can be stored.
    pub fn resolve(self, to_typ: &BTreeMap<RustType, String>) -> Cast<CanonicalKey> {
        match self {
            Cast::Any => Cast::Any,
            Cast::Value { value, ty, details } => {
                Cast::Value { value, ty: CanonicalKey::new(ty, to_typ), details }
            }
            Cast::Type(key) => Cast::Type(CanonicalKey::new(key, to_typ)),
            Cast::Union { of } => Cast::Union {
                of: of.into_iter().map(|cast| cast.resolve(to_typ)).collect(),
            },
            Cast::Dict { fields, rest } => Cast::Dict {
                fields: fields.into_iter().map(|field| field.resolve(to_typ)).collect(),
                rest: rest.map(|rest| Box::new(rest.resolve(to_typ))),
            },
            Cast::Array { elem } => Cast::Array { elem: Box::new(elem.resolve(to_typ)) },
            Cast::Tuple { elems } => Cast::Tuple {
                elems: elems.into_iter().map(|cast| cast.resolve(to_typ)).collect(),
            },
        }
    }
}

impl Cast<CanonicalKey> {
    /// The same tree with plain keys: canonical keys are a subset, so this only forgets
    /// the guarantee. Used when a stored row is refined again.
    pub fn into_unresolved(self) -> Cast<Key> {
        match self {
            Cast::Any => Cast::Any,
            Cast::Value { value, ty, details } => {
                Cast::Value { value, ty: ty.into_key(), details }
            }
            Cast::Type(key) => Cast::Type(key.into_key()),
            Cast::Union { of } => Cast::Union {
                of: of.into_iter().map(|cast| cast.into_unresolved()).collect(),
            },
            Cast::Dict { fields, rest } => Cast::Dict {
                fields: fields.into_iter().map(Field::into_unresolved).collect(),
                rest: rest.map(|rest| Box::new(rest.into_unresolved())),
            },
            Cast::Array { elem } => {
                Cast::Array { elem: Box::new(elem.into_unresolved()) }
            }
            Cast::Tuple { elems } => Cast::Tuple {
                elems: elems.into_iter().map(|cast| cast.into_unresolved()).collect(),
            },
        }
    }

    /// The manifest representation: every key spelled as a string.
    pub fn to_schema(&self) -> CastInfo {
        match self {
            Cast::Any => CastInfo::Any,
            Cast::Value { value, ty, details } => CastInfo::Value {
                value: value.clone(),
                ty: ty.as_str().to_string(),
                details: details.clone(),
            },
            Cast::Type(key) => CastInfo::Type { ty: key.as_str().to_string() },
            Cast::Union { of } => {
                CastInfo::Union { of: of.iter().map(Cast::to_schema).collect() }
            }
            Cast::Dict { fields, rest } => CastInfo::Dict {
                fields: fields.iter().map(Field::to_schema).collect(),
                rest: rest.as_ref().map(|rest| Box::new(rest.to_schema())),
            },
            Cast::Array { elem } => CastInfo::Array { elem: Box::new(elem.to_schema()) },
            Cast::Tuple { elems } => {
                CastInfo::Tuple { elems: elems.iter().map(Cast::to_schema).collect() }
            }
        }
    }
}

impl Field<Key> {
    fn resolve(self, to_typ: &BTreeMap<RustType, String>) -> Field<CanonicalKey> {
        Field {
            name: self.name,
            input: self.input.resolve(to_typ),
            required: self.required,
        }
    }
}

impl Field<CanonicalKey> {
    fn into_unresolved(self) -> Field<Key> {
        Field {
            name: self.name,
            input: self.input.into_unresolved(),
            required: self.required,
        }
    }

    fn to_schema(&self) -> DictField {
        DictField {
            name: self.name.clone(),
            input: self.input.to_schema(),
            required: self.required,
        }
    }
}
