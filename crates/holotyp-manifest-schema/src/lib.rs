//! Manifest schema
//!
//! The wire format of the manifest (spec HT-0001).
//!
//! Sketch: only the envelope exists so far. The node tree, the cast table and
//! their serialization rules are described in a later commit.

use serde::{Deserialize, Serialize};

/// Top-level manifest structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// The schema version of this format. Currently 1.
    pub schema_version: u32,
}

impl Manifest {
    /// The schema version this crate emits and understands.
    pub const SCHEMA_VERSION: u32 = 1;
}
