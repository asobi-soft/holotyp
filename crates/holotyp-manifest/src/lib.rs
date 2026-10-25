//! `holotyp-manifest` — reflects a Typst (fork) build's built-in library into
//! a versioned manifest (spec HT-0001).
//!
//! This crate is a library: [`generate`] builds the manifest, and the
//! `typst manifest` subcommand serializes it to stdout.

pub mod fork;
pub use holotyp_manifest_schema as schema;

use typst::Features;

/// Reflect the built-in library of this build.
///
/// Sketch: only the schema version is filled in; walking the library is not
/// implemented yet.
pub fn generate(features: Features) -> schema::Manifest {
    let _ = features;
    schema::Manifest {
        schema_version: schema::Manifest::SCHEMA_VERSION,
    }
}
