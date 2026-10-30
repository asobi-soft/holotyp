//! Payload types of the control-plane messages.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Who is speaking, announced in `Hello`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// The browser-side preview surface.
    Viewer,
    /// The editor / LSP control connection.
    Editor,
    /// The preview server itself.
    Server,
}

/// A compiler diagnostic as an LSP client consumes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct PreviewDiagnostic {
    pub severity: Severity,
    /// Typst diagnostic message + hint
    pub message: String,
    /// The primary location; `None` = detached
    pub span: Option<SourceSpan>,
    /// The call trace, as LSP `relatedInformation` entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<RelatedInformation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct RelatedInformation {
    pub span: SourceSpan,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A fatal error.
    Error,
    /// A non-fatal warning.
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    /// Absolute path of the file the span lives in.
    pub file: String,
    /// LSP-compatible range in UTF-16 code units.
    pub range: Range16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct Range16 {
    pub start: Position16,
    pub end: Position16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct Position16 {
    pub line: u32,
    pub character: u32,
}

/// A point on one page, in pt from that page's viewBox origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct PagePoint {
    pub page: u32,
    pub x: f32,
    pub y: f32,
}

/// One heading of the document outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct OutlineNode {
    pub title: String,
    pub level: u8,
    pub span: SourceSpan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<OutlineNode>,
}
