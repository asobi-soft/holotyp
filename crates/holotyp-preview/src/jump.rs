//! Span <-> geometry index for the preview server.

use typst::syntax::Span;
use typst_library::layout::{Abs, Frame, FrameItem, Point, Transform};
use typst_library::text::TextItem;
use typst_library::visualize::Geometry;

/// One located span occurrence on a page.
#[derive(Debug, Clone)]
pub struct SpanBox {
    pub page: usize,
    /// Bounding box in page coordinates (pt, origin top-left).
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub span: Option<Span>,
    /// Extra offset in the span
    pub offset: u16,
}

/// The whole-document index.
#[derive(Debug, Clone, Default)]
pub struct SpanIndex {
    pub boxes: Vec<SpanBox>,
}

impl SpanIndex {
    /// Build from a paged document.
    pub fn of(document: &typst_layout::PagedDocument) -> Self {
        let mut boxes = Vec::new();
        for (pi, page) in document.pages().iter().enumerate() {
            index_frame(&page.frame, pi, Transform::identity(), &mut boxes);
        }
        Self { boxes }
    }

    /// Document location to source location
    pub fn hit(&self, page: usize, x: f32, y: f32) -> Option<&SpanBox> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let mut best: Option<&SpanBox> = None;
        for b in &self.boxes {
            if b.page != page || b.span.is_none() {
                continue;
            }
            let inside =
                x >= b.x && x <= b.x + b.width && y >= b.y && y <= b.y + b.height;

            // inside & smallest
            if inside
                && best.is_none_or(|cur| (b.width * b.height) < (cur.width * cur.height))
            {
                best = Some(b);
            }
        }
        best
    }

    /// Source location to document location
    pub fn locate(&self, span: Span) -> impl Iterator<Item = &SpanBox> {
        self.boxes.iter().filter(move |b| b.span == Some(span))
    }
}

fn index_frame(frame: &Frame, page: usize, transform: Transform, out: &mut Vec<SpanBox>) {
    for (pos, item) in frame.items() {
        let transform = transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                index_frame(
                    &group.frame,
                    page,
                    transform.pre_concat(group.transform),
                    out,
                );
            }
            FrameItem::Text(text) => index_text(text, page, transform, out),
            FrameItem::Shape(shape, span) => {
                let (width, height) = shape_geometry_size(shape);
                push_box(
                    out,
                    page,
                    transform,
                    Point::zero(),
                    Point::new(Abs::pt(width as f64), Abs::pt(height as f64)),
                    *span,
                    0,
                );
            }
            FrameItem::Image(_, size, span) => {
                push_box(out, page, transform, Point::zero(), size.to_point(), *span, 0);
            }
            FrameItem::Link(_, size) => {
                push_box(
                    out,
                    page,
                    transform,
                    Point::zero(),
                    size.to_point(),
                    Span::detached(),
                    0,
                );
            }
            FrameItem::Tag(_) => {}
        }
    }
}

fn index_text(
    text: &TextItem,
    page: usize,
    transform: Transform,
    out: &mut Vec<SpanBox>,
) {
    let size = text.size;
    let mut cursor = Point::zero();
    for glyph in &text.glyphs {
        if !glyph.span.0.is_detached() {
            // `advance` wide, `size` tall
            let offset = Point::new(glyph.x_offset.at(size), -glyph.y_offset.at(size));
            let start = Point::new(cursor.x, -cursor.y) + offset;
            let end = Point::new(start.x + glyph.x_advance.at(size), start.y - size);
            push_box(out, page, transform, start, end, glyph.span.0, glyph.span.1);
        }
        cursor += Point::new(glyph.x_advance.at(size), glyph.y_advance.at(size));
    }
}

/// Project all four corners, not just the origin/size: rotations, skew and negative
/// scales must produce a normalized page-space axis-aligned box.
#[allow(clippy::too_many_arguments)]
fn push_box(
    out: &mut Vec<SpanBox>,
    page: usize,
    transform: Transform,
    a: Point,
    b: Point,
    span: Span,
    offset: u16,
) {
    let corners = [a, Point::new(a.x, b.y), b, Point::new(b.x, a.y)]
        .map(|p| p.transform(transform));
    let mut min = corners[0];
    let mut max = corners[0];
    for p in &corners[1..] {
        min = min.min(*p);
        max = max.max(*p);
    }
    out.push(SpanBox {
        page,
        x: min.x.to_pt() as f32,
        y: min.y.to_pt() as f32,
        width: (max.x - min.x).to_pt() as f32,
        height: (max.y - min.y).to_pt() as f32,
        span: if span.is_detached() { None } else { Some(span) },
        offset,
    });
}

fn shape_geometry_size(shape: &typst_library::visualize::Shape) -> (f32, f32) {
    match &shape.geometry {
        Geometry::Line(to) => (to.x.abs().to_pt() as f32, to.y.abs().to_pt() as f32),
        Geometry::Rect(s) => (s.x.to_pt() as f32, s.y.to_pt() as f32),
        Geometry::Curve(curve) => {
            // Bounding box over all curve points.
            let mut maxx = 0.0_f32;
            let mut maxy = 0.0_f32;
            for item in &curve.0 {
                use typst_library::visualize::CurveItem;
                match item {
                    CurveItem::Move(p) | CurveItem::Line(p) => {
                        maxx = maxx.max(p.x.to_pt() as f32);
                        maxy = maxy.max(p.y.to_pt() as f32);
                    }
                    CurveItem::Cubic(p1, p2, p3) => {
                        for p in [p1, p2, p3] {
                            maxx = maxx.max(p.x.to_pt() as f32);
                            maxy = maxy.max(p.y.to_pt() as f32);
                        }
                    }
                    CurveItem::Close => {}
                }
            }
            (maxx, maxy)
        }
    }
}
