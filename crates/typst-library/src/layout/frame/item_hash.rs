//! Holotyp Item Hash
//!
//! `FrameItem`'s own `Hash` folds in source positions: the `Span` of every
//! `Glyph`, the `Span` carried by shapes and images. Inserting a single line
//! therefore changes the hash of every item after it, and a preview that
//! addresses items by that hash resends the whole document for a one-line edit.

use std::hash::{Hash, Hasher};

use crate::model::Destination;
use crate::text::{Glyph, TextItem};

use super::{Frame, FrameItem, GroupItem};

/// The content hash of one frame item.
pub fn item_hash(item: &FrameItem) -> u64 {
    struct Key<'a>(&'a FrameItem);

    impl Hash for Key<'_> {
        fn hash<H: Hasher>(&self, state: &mut H) {
            hash_item(self.0, state);
        }
    }

    // 128 bits folded into the protocol's 64-bit `ItemId`.
    let wide = typst_utils::hash128(&Key(item));
    (wide as u64) ^ ((wide >> 64) as u64)
}

fn hash_item<H: Hasher>(item: &FrameItem, state: &mut H) {
    match item {
        FrameItem::Group(group) => {
            0_u8.hash(state);
            hash_group(group, state);
        }
        FrameItem::Text(text) => {
            1_u8.hash(state);
            hash_text(text, state);
        }
        FrameItem::Shape(shape, _span) => {
            2_u8.hash(state);
            shape.hash(state);
        }
        FrameItem::Image(image, size, _span) => {
            3_u8.hash(state);
            image.hash(state);
            size.hash(state);
        }
        FrameItem::Link(dest, size) => {
            4_u8.hash(state);
            hash_destination(dest, state);
            size.hash(state);
        }
        FrameItem::Tag(_tag) => {
            // A tag draws nothing
            5_u8.hash(state);
        }
    }
}

fn hash_group<H: Hasher>(group: &GroupItem, state: &mut H) {
    let GroupItem { frame, transform, clip, label, parent: _parent } = group;
    // `parent` is logical, nothing is drawn for it
    hash_frame(frame, state);
    transform.hash(state);
    clip.hash(state);
    label.hash(state);
}

fn hash_frame<H: Hasher>(frame: &Frame, state: &mut H) {
    let Frame { size, baseline, items, kind } = frame;
    size.hash(state);
    baseline.hash(state);
    kind.hash(state);
    items.len().hash(state);
    for (pos, item) in items.iter() {
        pos.hash(state);
        hash_item(item, state);
    }
}

fn hash_text<H: Hasher>(text: &TextItem, state: &mut H) {
    let TextItem {
        font,
        size,
        fill,
        stroke,
        lang,
        region,
        text,
        glyphs,
    } = text;
    font.hash(state);
    size.hash(state);
    fill.hash(state);
    stroke.hash(state);
    lang.hash(state);
    region.hash(state);
    text.hash(state);
    glyphs.len().hash(state);
    for glyph in glyphs {
        let Glyph {
            id,
            x_advance,
            x_offset,
            y_advance,
            y_offset,
            range,
            span: _span,
        } = glyph;
        id.hash(state);
        x_advance.hash(state);
        x_offset.hash(state);
        y_advance.hash(state);
        y_offset.hash(state);
        range.hash(state);
    }
}

/// A link draws an `<a>` around its body; only a URL ends up in the markup.
fn hash_destination<H: Hasher>(dest: &Destination, state: &mut H) {
    match dest {
        Destination::Url(url) => {
            0_u8.hash(state);
            url.hash(state);
        }
        Destination::Position(pos) => {
            // nothing is drawn by typst yet, but probably in the future
            1_u8.hash(state);
            pos.hash(state);
        }
        Destination::Location(_location) => {
            // nothing is drawn by typst yet
            2_u8.hash(state);
        }
    }
}
