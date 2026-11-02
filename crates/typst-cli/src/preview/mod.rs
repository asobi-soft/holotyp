//! Local HTTP/WS host for the preview engine, behind `typst preview`.
//!
//! [`server::serve`] builds the whole host around a [`session::Session`], which the
//! `typst preview` command and the tests drive.

pub mod command;
mod location;
pub mod server;
pub mod session;
pub mod source;
pub mod world;
