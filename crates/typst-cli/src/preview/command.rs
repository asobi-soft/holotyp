//! `typst preview`: serve a live Holotyp preview of a document.

use std::path::{Path, PathBuf};

use ecow::{EcoString, eco_format};
use typst::World;

use super::server::{browser_url, serve};
use super::session::Session;
use super::source::SourceSet;

use crate::args::{Input, PreviewCommand};
use crate::world::SystemWorld;

pub fn preview(command: &'static PreviewCommand) -> typst::diag::HintedStrResult<()> {
    let display_root = display_root(command);
    let world = SystemWorld::new(
        Some(&Input::Path(command.input.clone())),
        &command.world,
        &command.process,
    )?;

    let main = world
        .resolve(world.main())
        .map_err(|err| -> EcoString { eco_format!("{err}") })?;
    let session = Session::new(SourceSet::new(main), world, display_root);

    let settings = command.settings;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| -> EcoString { eco_format!("tokio runtime: {e}") })?;
    runtime.block_on(async move {
        match serve(session, &command.host, command.port, settings).await {
            Ok(bound) => {
                if !command.no_open
                    && let Err(err) = open::that_detached(browser_url(bound))
                {
                    eprintln!("could not open the preview page: {err}");
                }
                let _ = tokio::signal::ctrl_c().await;
                eprintln!("shutting down server at port {}", bound.port());
                Ok(())
            }
            Err(err) => Err(eco_format!("{err}").into()),
        }
    })
}

fn display_root(command: &PreviewCommand) -> PathBuf {
    let root = command
        .world
        .root
        .as_deref()
        .or_else(|| command.input.parent())
        .filter(|root| !root.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf())
}
