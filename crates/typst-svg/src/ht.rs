//! Holotyp seam: render frames incrementally, with the frame body and the
//! resources it introduces handed out separately.
//!
//! How it works: `SVGRenderer` deduplicates glyph outlines, clip paths,
//! gradients and tilings while rendering, recording them in first-seen order.
//! Each definition can be written on its own (`glyph_defs`, `clip_path_defs`,
//! `gradient_defs`, ...), so the document path keeps writing them into a `<defs>`
//! block while a preview only ships the ones the client does not have yet.
//! Glyph ids are content hashes (`g` + 128-bit hash), so an id is stable across
//! frames and a client can accumulate the defs once and cache them.
//!
//! Usage from a preview server:
//! ```ignore
//! let mut session = SvgSession::new();
//! for page in document.pages() {
//!     let fragment = session.fragment(&page.frame, page.frame.size());
//!     // send `fragment.body` and `fragment.defs`
//! }
//! ```

use super::*;

/// Renders frames one at a time, sharing resources across them.
pub struct SvgSession {
    renderer: SVGRenderer<'static>,
}

impl Default for SvgSession {
    fn default() -> Self {
        Self::new()
    }
}

/// One rendered frame: its `<g>` body and the resources it introduces.
pub struct Fragment {
    /// The `<g>` element carrying the frame's content.
    pub body: String,
    /// Resource definitions the client does not have yet, as `(id, element)`.
    pub defs: Vec<(String, String)>,
}

impl SvgSession {
    /// Creates a new session with an empty resource table.
    pub fn new() -> Self {
        Self { renderer: SVGRenderer::new() }
    }

    /// Sets the table that turns unresolved links into `data-ht-link` tags, i.e. into a
    /// target the viewer jumps to without asking the server.
    pub fn set_targets(&mut self, targets: Option<Arc<dyn LinkTargets>>) {
        self.renderer.targets = targets;
    }

    /// Renders one frame, splitting the frame body from its resources.
    ///
    /// The body is a `<g>` element and can be grafted into a page `<svg>` as
    /// is. Each entry of `defs` is one `<defs>` child the client has not
    /// received before, so it only has to append it to its own resource table.
    pub fn fragment(&mut self, frame: &Frame, size: Size) -> Fragment {
        // Everything already in the table was sent by an earlier fragment;
        // whatever rendering appends from this point is new to the client.
        let from = self.renderer.glyphs.iter().count();

        let mut xml = XmlWriter::new(xml_options(false));
        {
            let mut svg = SvgElem::new(&mut xml, "g");
            let state = State::new(size);
            self.renderer.render_frame(&mut svg, &state, frame);
        }

        let mut defs = Vec::new();
        defs.extend(self.renderer.glyph_defs(from));
        defs.extend(self.renderer.clip_path_defs());
        defs.extend(self.renderer.gradient_defs());
        defs.extend(self.renderer.gradient_ref_defs());
        defs.extend(self.renderer.subgradient_defs());
        defs.extend(self.renderer.tiling_defs());
        defs.extend(self.renderer.tiling_ref_defs());

        Fragment { body: xml.end_document(), defs }
    }
}
