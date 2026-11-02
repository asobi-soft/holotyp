//! Local HTTP/WS host for the preview engine, behind `typst preview`.
//!
//! [`server::serve`] builds the whole host around the preview engine, which
//! the `typst preview` command and the tests drive.

pub mod command;
pub mod server;
