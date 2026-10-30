pub mod control;
pub mod data;
pub mod frame;
pub mod message;
pub mod settings;

pub use control::*;
pub use data::*;
pub use frame::*;
pub use message::*;
pub use settings::*;

use std::path::Path;
use ts_rs::TS;

/// The wire version, carried by [`Message::Hello`].
pub const PROTOCOL_VERSION: u32 = 1;

/// Write the TypeScript bindings for the whole protocol into `dir`.
pub fn export_bindings(dir: impl AsRef<Path>) -> Result<(), ts_rs::ExportError> {
    let dir = dir.as_ref();
    let cfg = ts_rs::Config::new().with_out_dir(dir.to_path_buf());

    // `Frame` wraps `Message`; exporting both covers every type in the crate.
    Frame::export_all(&cfg)?;
    Message::export_all(&cfg)?;
    PreviewSettings::export_all(&cfg)?;

    // https://github.com/Aleph-Alpha/ts-rs/issues/441
    std::fs::write(
        dir.join("version.ts"),
        format!("export const PROTOCOL_VERSION = {PROTOCOL_VERSION}"),
    )?;
    Ok(())
}
