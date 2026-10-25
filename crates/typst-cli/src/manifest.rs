//! `typst manifest`: dump the built-in library manifest (HT-0001). // HT: manifest subcommand
//!
//! Thin serializer over `holotyp-manifest`: the crate reflects the linked
//! build's library, this module owns the CLI surface (features, pretty print).

use ecow::{EcoString, eco_format};

use crate::args::ManifestCommand;

pub fn manifest(command: &ManifestCommand) -> typst::diag::HintedStrResult<()> {
    let features: typst::Features =
        command.features.iter().copied().map(Into::into).collect();
    let manifest = holotyp_manifest::generate(features);
    let json = if command.pretty {
        serde_json::to_string_pretty(&manifest)
    } else {
        serde_json::to_string(&manifest)
    }
    .map_err(|err| -> EcoString { eco_format!("serialize manifest: {err}") })?;
    println!("{json}");
    Ok(())
}
