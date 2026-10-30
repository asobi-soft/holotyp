//! Data plane: one [`crate::Message::Delta`] per revision.
//!
//! Vocabulary follows the SVG renderer instead of inventing new nouns:
//! - a [`Def`] is one `<defs>` entry the `typst-svg` renderer writes (`write_glyph_defs`,
//!   `write_clip_path_defs`, `write_gradients`, ...); its `id` is that renderer's
//!   `DedupId` (`g...` glyph symbol, `c...` clip path, `f...` gradient, `t...` tiling);
//! - an [`Item`] is the rendering of one top-level `FrameItem` at the frame origin
//!   (`SvgSession::render_item`), identified by the content hash of that `FrameItem`
//!   (position excluded).
//!
//! So the sizes involved are: many items per page, a handful of new items per edit, and a
//! document-wide pool of defs that is sent once.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Content hash of a top-level `FrameItem` (position excluded).
pub type ItemId = u64;

/// One revision of the document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct Delta {
    pub revision: u64,
    /// Start from an empty document. Sent as the first frame of a connection.
    pub reset: bool,
    /// Segments of this revision's page list, in order. Unchanged runs are one `Copy`
    /// segment, for example the 2000-page document is `[Copy{0,1999}, New{...}]`.
    pub pages: Vec<PageEntry>,
    /// Renderings the client does not have yet
    pub items: Vec<Item>,
    /// `<defs>` entries the client does not have yet
    pub defs: Vec<Def>,
}

/// The `PageEntry::Copy` payload: pages `[from, from + count)` of the previous revision,
/// unchanged in content and size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct CopyRun {
    pub from: u32,
    pub count: u32,
}

/// The `PageEntry::New` payload: one page of the new list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct NewPage {
    /// Index into the previous revision's page list, or `None` for a page with no
    /// previous state
    pub from: Option<u32>,
    /// Size; `None` = same as the `from` page.
    #[serde(default)]
    pub size: Option<PageSize>,
    /// The page background, as the id of the [`Item`] that paints it.
    pub background: Option<ItemId>,
    pub layout: Layout,
}

/// One segment of the new page list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub enum PageEntry {
    /// Pages `[from, from + count)` of the previous revision, unchanged in content and
    /// size.
    Copy(CopyRun),
    /// One page of the new list.
    New(NewPage),
}

/// How the page's slots look in this revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    /// Same slots as the `from` page; only the page size or the background can differ.
    Same,
    /// Exhaustive description of the page's slots, applied in order.
    Ops(Vec<SlotOp>),
}

/// One slot edit. Applying the whole list yields the page's new slot sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub enum SlotOp {
    /// Slots `[from, from + count)` of the `from` page, placed at `[at, at + count)` with
    /// a uniform shift.
    Reuse { at: u32, from: u32, count: u32, dx: f32, dy: f32 },
    /// Place item `id` at `at`, at `(x, y)`
    Place { at: u32, id: ItemId, x: f32, y: f32 },
}

/// The rendering of one top-level `FrameItem`, at the frame origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: ItemId,
    /// SVG fragment (`<g>...</g>`, `<use>...`, `<path>...`, ...).
    pub svg: String,
}

/// One `<defs>` entry as written by the renderer, keyed by its `DedupId`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct Def {
    pub id: String,
    /// `<symbol>...</symbol>`, `<clipPath>...</clipPath>`, `<linearGradient>...`, ...
    pub svg: String,
}

/// Page geometry in pt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "index.ts")]
#[serde(rename_all = "camelCase")]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}
