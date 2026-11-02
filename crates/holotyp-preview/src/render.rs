//! Rendering bridge: renders individual frame items through the `SvgSession` seam and
//! tracks client-side slot positions across revisions.
//!
//! The seam hands back the `<g>` body and the resource definitions separately, so nothing
//! here has to look inside the SVG.

use std::collections::HashSet;
use std::sync::Arc;

use typst_layout::{Page, PagedDocument};
use typst_library::layout::{Frame, FrameItem, Point, Size};
use typst_library::model::LinkTargets;
use typst_library::visualize::Geometry;
use typst_svg::{Fragment, SvgSession};

/// The page background as an ordinary item: a page-sized rect painted with the page's
/// resolved fill, at the page origin. Just like what `SVGRenderer::render_page` does.
///
/// Bleeding is ignored since we are not printing.
pub fn background_item(page: &Page) -> Option<FrameItem> {
    let fill = page.fill_or_white()?;
    let shape = Geometry::Rect(page.frame.size()).filled(fill);
    Some(FrameItem::Shape(shape, typst_syntax::Span::detached()))
}

/// Renders items and tracks what the client currently shows.
pub struct FrameRenderer {
    session: SvgSession,
    doc: Option<Arc<PagedDocument>>,
    sent_items: HashSet<u64>,
    sent_defs: HashSet<String>,
    revision: u64,
}

impl FrameRenderer {
    pub fn new() -> Self {
        Self {
            session: SvgSession::new(),
            doc: None,
            sent_items: HashSet::new(),
            sent_defs: HashSet::new(),
            revision: 0,
        }
    }

    /// Swap in a newly compiled document (call once per compilation).
    pub fn set_document(&mut self, doc: Arc<PagedDocument>) {
        // The same document resolves link targets for rendering and for hashing.
        self.session.set_targets(Some(doc.clone() as Arc<dyn LinkTargets>));
        self.doc = Some(doc);
        self.revision += 1;
    }

    pub fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Render the item at (page, slot), origin-normalized.
    ///
    /// Returns `None` for non-renderable items (tags).
    pub fn render_item(&mut self, page: usize, slot: usize) -> Option<Fragment> {
        let p = self.doc.as_ref()?.pages().get(page)?;
        let size = p.frame.size();
        // Walk to the slot (frames expose an iterator, not indexing).
        let (_pos, item) = p
            .frame
            .items()
            .by_ref()
            .nth(slot)
            .map(|(pos, item)| (pos, item.clone()))?;

        if matches!(item, FrameItem::Tag(_)) {
            return None;
        }

        Some(self.render_origin(item, size))
    }

    /// Render the page's background item, origin-normalized.
    ///
    /// Returns `None` for a page that paints no background (`fill: none`).
    pub fn render_background(&mut self, page: usize) -> Option<Fragment> {
        let (item, size) = {
            let p = self.doc.as_ref()?.pages().get(page)?;
            (background_item(p)?, p.frame.size())
        };
        Some(self.render_origin(item, size))
    }

    /// Render one item at the frame origin.
    fn render_origin(&mut self, item: FrameItem, size: Size) -> Fragment {
        let mut carrier = Frame::soft(size);
        carrier.push(Point::zero(), item);
        self.session.fragment(&carrier, size)
    }
}

impl FrameRenderer {
    /// Whether this item's body was already sent in the current stream.
    pub(crate) fn already_sent(&self, item: u64) -> bool {
        self.sent_items.contains(&item)
    }

    /// Records an item body as sent.
    pub(crate) fn mark_sent(&mut self, item: u64) {
        self.sent_items.insert(item);
    }

    /// Records a resource as sent, reporting whether it was new.
    pub(crate) fn mark_def_sent(&mut self, id: &str) -> bool {
        self.sent_defs.insert(id.to_owned())
    }

    /// Forgets what the client has, so the next delta resends everything.
    pub(crate) fn reset_sent(&mut self) {
        self.sent_items.clear();
        self.sent_defs.clear();
    }
}

impl Default for FrameRenderer {
    fn default() -> Self {
        Self::new()
    }
}
