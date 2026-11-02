//! The preview engine: compiles a document, projects it into a diffable
//! [`DocumentModel`], and emits protocol frames for the item-level delta transport.

pub mod delta;
pub mod jump;
pub mod render;

pub use delta::DocumentModel;
pub use render::FrameRenderer;
