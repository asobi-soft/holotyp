//! `holotyp-manifest` — reflects a Typst (fork) build's built-in library into a versioned
//! manifest (spec HT-0001).
//!
//! This crate is a library: [`generate`] builds the manifest, and the `typst manifest`
//! subcommand serializes it to stdout.

mod cast;
mod cast_type;
pub mod fork;
pub mod key;
pub mod meta;
mod meta_macros;
pub mod reflect;
mod rust_type;
pub use holotyp_manifest_schema as schema;

use typst::Features;
use typst::LibraryExt;

pub fn generate(features: Features) -> schema::Manifest {
    let library = typst::Library::builder().with_features(features).build();

    let build_version = typst::utils::version();
    let commit = build_version.commit().map(str::to_string);

    let build = schema::Build {
        name: fork::FORK_NAME.into(),
        version: build_version.raw().into(),
        revision: commit,
    };

    reflect::reflect_library(&library, build)
}
