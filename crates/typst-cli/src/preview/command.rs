//! `typst preview`: serve a live Holotyp preview of a document.

use ecow::{EcoString, eco_format};

use super::server::{browser_url, serve};

use crate::args::{Input, PreviewCommand};
use crate::world::SystemWorld;

pub fn preview(command: &'static PreviewCommand) -> typst::diag::HintedStrResult<()> {
    let world = SystemWorld::new(
        Some(&Input::Path(command.input.clone())),
        &command.world,
        &command.process,
    )?;

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
