//! The one message type of the protocol.
//!
//! Data plane ([`Message::Delta`]) and control plane share the enum; the
//! encoding is the same for both, so there is no tag byte and no way for the
//! two sides to disagree about "which kind of frame is this".

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Delta, OutlineNode, PagePoint, PreviewDiagnostic, Role, SourceSpan};

/// Everything that can travel in a frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub enum Message {
    /// Preview update
    Delta(Delta),
    /// Bidirectional hello; the first frame of a connection.
    Hello {
        /// The [`PROTOCOL_VERSION`](crate::PROTOCOL_VERSION) the sender speaks.
        protocol: u32,
        role: Role,
    },
    /// Server -> LSP
    Diagnostics { diags: Vec<PreviewDiagnostic> },
    /// Viewer -> Server: click at a page point, resolve it to source.
    DocToSource(PagePoint),
    /// Server -> Editor: where the requested document point came from.
    SourceLocation { span: SourceSpan },
    /// Editor -> Server: cursor moved; the server answers with `ScrollTo`.
    SourceToDoc { file: String, line: u32, col: u32 },
    /// Editor -> Server: replace the unsaved buffer of one file.
    ///
    /// `None` releases the overlay and resumes reading the file from disk.
    SourceUpdate { file: String, version: u64, text: Option<String> },
    /// Server -> Viewer: reveal this page point.
    ScrollTo(PagePoint),
    /// Server -> Editor: document outline
    Outline { nodes: Vec<OutlineNode> },
    /// Viewer -> Server: visible page range
    Viewport { first: u32, last: u32 },
    /// Either side: structured log/event channel for debugging
    Event { level: u8, msg: String },
}

impl Message {
    /// Version gate for the handshake.
    pub fn hello_version_ok(&self) -> Option<bool> {
        match self {
            Self::Hello { protocol, .. } => Some(*protocol == crate::PROTOCOL_VERSION),
            _ => None,
        }
    }

    /// The connection role announced by a `Hello`; `None` for other messages.
    pub fn role(&self) -> Option<Role> {
        match self {
            Self::Hello { role, .. } => Some(*role),
            _ => None,
        }
    }

    /// Whether this is a data message.
    pub fn is_data(&self) -> bool {
        matches!(self, Self::Delta(_))
    }
}
