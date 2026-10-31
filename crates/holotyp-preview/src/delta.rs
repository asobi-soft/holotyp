//! Content-addressed page and slot deltas over compiled frames.

use std::collections::{HashMap, VecDeque};

use holotyp_preview_protocol::{
    CopyRun, Def, Delta, Item, Layout, NewPage, PageEntry, PageSize, SlotOp,
};
use typst_layout::{Page, PagedDocument};
use typst_library::layout::{FrameItem, Point};
use typst_library::model::LinkTargets;
use typst_svg::Fragment;

use crate::render::{FrameRenderer, background_item};

/// One top-level frame item, as the diff sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotItem {
    /// Content hash of the item; its position is not part of it.
    pub hash: u64,
    pub x: f32,
    pub y: f32,
    /// Tags carry metadata rather than content and never render.
    pub is_tag: bool,
}

/// The slots of one page, in frame order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageModel {
    pub width: f32,
    pub height: f32,
    /// Content hash of the page's background body, `None` for a page that
    /// paints none.
    pub background: Option<u64>,
    pub items: Vec<SlotItem>,
}

impl PageModel {
    /// The page's size as the protocol carries it.
    fn size(&self) -> PageSize {
        PageSize { width: self.width, height: self.height }
    }
}

impl SlotItem {
    /// One slot, hashed against the same link targets the renderer writes.
    fn new(pos: &Point, item: &FrameItem, targets: &dyn LinkTargets) -> Self {
        Self {
            hash: typst_library::layout::item_hash::item_hash(item, targets),
            x: pos.x.to_pt() as f32,
            y: pos.y.to_pt() as f32,
            is_tag: matches!(item, FrameItem::Tag(_)),
        }
    }
}

impl PageModel {
    fn new(page: &Page, targets: &dyn LinkTargets) -> Self {
        let size = page.frame.size();
        Self {
            width: size.x.to_pt() as f32,
            height: size.y.to_pt() as f32,
            background: background_item(page)
                .map(|item| typst_library::layout::item_hash::item_hash(&item, targets)),
            items: page
                .frame
                .items()
                .map(|(pos, item)| SlotItem::new(pos, item, targets))
                .collect(),
        }
    }
}

/// The pages of one compilation, as the diff sees them.
#[derive(Debug, Clone, Default)]
pub struct DocumentModel {
    pub pages: Vec<PageModel>,
}

impl From<&PagedDocument> for DocumentModel {
    fn from(document: &PagedDocument) -> Self {
        let targets: &dyn LinkTargets = document;
        Self {
            pages: document
                .pages()
                .iter()
                .map(|page| PageModel::new(page, targets))
                .collect(),
        }
    }
}

impl DocumentModel {
    /// A previous page that is the same page: slots and size both match.
    fn identical_page(&self, page: &PageModel, index: usize) -> Option<usize> {
        (self.pages.get(index) == Some(page))
            .then_some(index)
            .or_else(|| self.pages.iter().position(|candidate| candidate == page))
    }

    /// The page sharing the most slots with `page`, if any shares one.
    fn closest_page(&self, page: &PageModel, index: usize) -> Option<usize> {
        self.pages
            .iter()
            .enumerate()
            .map(|(other, candidate)| (other, shared_slots(candidate, page)))
            .max_by_key(|&(other, shared)| (shared, usize::MAX - other.abs_diff(index)))
            .filter(|&(_, shared)| shared > 0)
            .map(|(other, _)| other)
    }
}

/// How many slots two pages have in common, counting duplicates.
fn shared_slots(a: &PageModel, b: &PageModel) -> usize {
    let mut counts: HashMap<u64, usize> = HashMap::new();
    for item in &a.items {
        *counts.entry(item.hash).or_default() += 1;
    }
    b.items
        .iter()
        .filter(|item| match counts.get_mut(&item.hash) {
            Some(count) if *count > 0 => {
                *count -= 1;
                true
            }
            _ => false,
        })
        .count()
}

impl FrameRenderer {
    /// Sends the whole document, forgetting what the client already has.
    pub fn full_sync(&mut self, model: &DocumentModel) -> Delta {
        self.reset_sent();
        self.build_delta(None, model)
    }

    /// Sends what changed between two revisions.
    pub fn diff(&mut self, old: &DocumentModel, new: &DocumentModel) -> Delta {
        self.build_delta(Some(old), new)
    }

    fn build_delta(&mut self, old: Option<&DocumentModel>, new: &DocumentModel) -> Delta {
        let mut delta = Delta {
            revision: self.revision(),
            reset: old.is_none(),
            ..Delta::default()
        };
        let mut copy: Option<(u32, u32)> = None;

        // for each page
        for (index, page) in new.pages.iter().enumerate() {
            // if there's identical page, copy it
            if let Some(from) = old.and_then(|old| old.identical_page(page, index)) {
                copy_page(&mut delta, &mut copy, from as u32);
                continue;
            }
            flush_copy(&mut delta, &mut copy);

            // there isn't identical, then diff with the closest one
            let from_index = old.and_then(|old| old.closest_page(page, index));
            let from = from_index.and_then(|from| old.map(|old| &old.pages[from]));
            let layout = match from {
                // only size changed
                Some(from) if from.items == page.items => Layout::Same,
                _ => Layout::Ops(self.slot_ops(from, page, index, &mut delta)),
            };
            let size = from
                .is_none_or(|from| from.size() != page.size())
                .then(|| page.size());
            self.emit_background(&mut delta, index, page.background);
            delta.pages.push(PageEntry::New(NewPage {
                from: from_index.map(|from| from as u32),
                size,
                background: page.background,
                layout,
            }));
        }
        flush_copy(&mut delta, &mut copy);
        delta
    }

    /// The slot operations that turn `from` into `page`.
    fn slot_ops(
        &mut self,
        from: Option<&PageModel>,
        page: &PageModel,
        page_index: usize,
        delta: &mut Delta,
    ) -> Vec<SlotOp> {
        // hash -> [index]
        let mut from_slots: HashMap<u64, VecDeque<usize>> = HashMap::new();
        if let Some(from) = from {
            for (index, item) in from.items.iter().enumerate() {
                from_slots.entry(item.hash).or_default().push_back(index);
            }
        }

        let mut ops = Vec::new();
        // for each item in current page
        for (index, item) in page.items.iter().enumerate() {
            let index32 = index as u32;
            let matched =
                from_slots.get_mut(&item.hash).and_then(VecDeque::pop_front).zip(from);
            let Some((from_index, previous_page)) = matched else {
                // this is a new item
                // a tag has no body, so there is nothing for the client to place.
                if !item.is_tag {
                    self.render_slot(delta, page_index, index, item);
                    ops.push(SlotOp::Place {
                        at: index32,
                        id: item.hash,
                        x: item.x,
                        y: item.y,
                    });
                }
                continue;
            };

            // reuse `from`
            let from = &previous_page.items[from_index];
            let dx = item.x - from.x;
            let dy = item.y - from.y;
            // Runs of equal shift collapse into one operation.
            if let Some(SlotOp::Reuse {
                at: run_at,
                from: run_from,
                count,
                dx: run_dx,
                dy: run_dy,
            }) = ops.last_mut()
                && *run_at + *count == index32
                && *run_from + *count == from_index as u32
                && *run_dx == dx
                && *run_dy == dy
            {
                *count += 1;
                continue;
            }
            ops.push(SlotOp::Reuse {
                at: index32,
                from: from_index as u32,
                count: 1,
                dx,
                dy,
            });
        }
        ops
    }

    /// Renders one slot and records its body and resources in the delta.
    fn render_slot(
        &mut self,
        delta: &mut Delta,
        page_index: usize,
        slot_index: usize,
        item: &SlotItem,
    ) {
        if self.already_sent(item.hash) {
            return;
        }
        let fragment = self.render_item(page_index, slot_index);
        self.push_fragment(delta, item.hash, fragment);
    }

    /// Renders one page's background and records its body and resources in the
    /// delta.
    fn emit_background(
        &mut self,
        delta: &mut Delta,
        page_index: usize,
        background: Option<u64>,
    ) {
        let Some(id) = background else { return };
        if self.already_sent(id) {
            return;
        }
        let fragment = self.render_background(page_index);
        self.push_fragment(delta, id, fragment);
    }

    /// Adds a rendered body plus the resources it introduced, once per id.
    fn push_fragment(&mut self, delta: &mut Delta, id: u64, fragment: Option<Fragment>) {
        let Some(fragment) = fragment else { return };
        self.mark_sent(id);
        delta.items.push(Item { id, svg: fragment.body });
        for (def, svg) in fragment.defs {
            if self.mark_def_sent(&def) {
                delta.defs.push(Def { id: def, svg });
            }
        }
    }
}

/// Extends the open run of copied pages, or starts a new one.
fn copy_page(delta: &mut Delta, copy: &mut Option<(u32, u32)>, from: u32) {
    if let Some((start, count)) = copy
        && *start + *count == from
    {
        *count += 1;
        return;
    }
    flush_copy(delta, copy);
    *copy = Some((from, 1));
}

/// Closes the open run of copies.
fn flush_copy(delta: &mut Delta, copy: &mut Option<(u32, u32)>) {
    if let Some((from, count)) = copy.take() {
        delta.pages.push(PageEntry::Copy(CopyRun { from, count }));
    }
}
