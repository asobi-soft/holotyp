//! The preview session: owns the compile pipeline and turns source versions into protocol
//! frame batches.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use holotyp_preview::{FrameRenderer, delta::DocumentModel, jump::SpanIndex};
use holotyp_preview_protocol::{
    Delta, Message, OutlineNode, Position16, PreviewDiagnostic, Range16,
    RelatedInformation, Role, Severity, SourceSpan,
};
use syntax::{DiagSpan, Span, SyntaxNode};
use typst::diag::{
    Severity as TypstSeverity, SourceDiagnostic as TypstDiagnostic, SourceResult, Warned,
};
use typst::{World, syntax};
use typst_layout::PagedDocument;
use typst_library::{introspection::Tag, layout::FrameItem, model::HeadingElem};

use crate::preview::location;
use crate::preview::location::{FrozenSources, position_distance, position_to_byte};
use crate::preview::source::SourceSet;
use crate::preview::world::SnapshotWorld;
use crate::world::SystemWorld;

/// Everything one compile cycle publishes
#[derive(Debug, Clone)]
pub struct Update {
    pub epoch: u64,
    /// Fresh frames, or `None` when compilation failed
    pub delta: Option<Delta>,
    /// Heading outline to publish. `None` means there is nothing to send, either
    /// unchanged or the cycle failed
    pub outline: Option<Vec<OutlineNode>>,
    pub diags: Vec<PreviewDiagnostic>,
}

impl Update {
    /// The frames this update publishes.
    pub fn messages(self) -> Vec<(Role, Message)> {
        let mut messages =
            vec![(Role::Editor, Message::Diagnostics { diags: self.diags })];
        if let Some(nodes) = self.outline {
            messages.push((Role::Editor, Message::Outline { nodes }));
        }
        if let Some(delta) = self.delta {
            messages.push((Role::Viewer, Message::Delta(delta)));
        }
        messages
    }
}

/// What wakes a [`Session`]: something changed, and where.
#[derive(Debug)]
pub enum Trigger {
    /// The file watcher saw a change; which path moved is not reported.
    Disk,
    /// An editor replaced (`Some`) or released (`None`) one file's buffer.
    Overlay { file: String, version: u64, text: Option<String> },
    /// A new editor connection took over, clear all overlays
    EditorHello,
}

/// Compile-and-diff state machine over a source set.
pub struct Session {
    sources: SourceSet,
    base: SystemWorld,
    display_root: PathBuf,
    renderer: FrameRenderer,
    fresh_full: Option<Delta>,
    epoch: u64,
    last_doc: Option<Arc<PagedDocument>>,
    last_jump: Option<SpanIndex>,
    last_ranges: HashMap<Span, (u32, u32)>,
    last_sources: Option<FrozenSources>,
    last_outline: Vec<OutlineNode>,
    last_model: Option<DocumentModel>,
    last_failure: Option<Vec<PreviewDiagnostic>>,
    last_warnings: Vec<PreviewDiagnostic>,
}

impl Session {
    pub fn new(sources: SourceSet, base: SystemWorld, display_root: PathBuf) -> Self {
        Self {
            sources,
            base,
            display_root,
            renderer: FrameRenderer::new(),
            fresh_full: None,
            last_doc: None,
            last_jump: None,
            last_model: None,
            last_ranges: HashMap::default(),
            last_sources: None,
            last_outline: Vec::new(),
            epoch: 0,
            last_failure: None,
            last_warnings: Vec::new(),
        }
    }

    /// Initial compile. Returns the first publishable update.
    pub fn initial_cycle(&mut self) -> Update {
        self.sources.refresh();
        self.cycle()
    }

    /// Apply one wake-up trigger; returns whether a recompile is due.
    pub fn apply(&mut self, trigger: Trigger) -> bool {
        match trigger {
            Trigger::Disk => self.sources.refresh(),
            Trigger::Overlay { file, version, text } => {
                self.sources.set_overlay(&file, version, text)
            }
            Trigger::EditorHello => self.sources.clear_overlays(),
        }
    }

    /// Paths the file watcher must watch: dependencies and editor buffers.
    pub fn dependencies(&self) -> Vec<std::path::PathBuf> {
        self.sources.paths()
    }

    pub fn doc_loc_to_src_loc(
        &self,
        page: usize,
        x: f32,
        y: f32,
    ) -> Option<(String, Range16, (u32, u32))> {
        let b = self.last_jump.as_ref()?.hit(page, x, y)?;
        let location =
            self.last_sources
                .as_ref()?
                .resolve(b.span?, b.offset, &self.last_ranges)?;
        Some((location.file, location.range, location.byte_range))
    }

    pub fn src_loc_to_doc_loc(
        &self,
        file: &str,
        line: u32,
        col: u32,
    ) -> Option<(usize, f32, f32)> {
        let idx = self.last_jump.as_ref()?;
        let sources = self.last_sources.as_ref()?;
        let target_id = sources.id_of(std::path::Path::new(file))?;
        let target_byte = position_to_byte(sources.source(target_id)?, line, col)?;
        let mut best = None;
        for b in &idx.boxes {
            // has span
            let Some(span) = b.span else { continue };
            if span.id() != Some(target_id) {
                continue;
            }

            // target
            let Some(source) = sources.source(target_id) else { continue };

            // cur
            let Some(location) = sources.resolve(span, b.offset, &self.last_ranges)
            else {
                continue;
            };
            let (start, end) = location.byte_range;

            // distance between target and cur box's loc
            let Some(distance) =
                position_distance(source, target_byte, start as usize..end as usize)
            else {
                continue;
            };

            let Some(cur_pos) = source.lines().byte_to_utf16(start as usize) else {
                continue;
            };
            let target_col = source.lines().byte_to_utf16(target_byte)?;
            // Nearest byte distance, then distance from the span's anchor, then the
            // shortest span, then the smallest box. The area cannot be NaN (`Abs` is a
            // `Scalar`, which folds NaN to zero on construction), so it sorts as part of
            // the key.
            let key =
                (distance, target_col.abs_diff(cur_pos), end - start, b.width * b.height);
            if best.as_ref().is_none_or(|(old_key, _)| key < *old_key) {
                best = Some((key, b));
            }
        }
        best.map(|(_, b)| (b.page, b.x + b.width / 2.0, b.y + b.height / 2.0))
    }

    /// Outline from heading tags in a compiled document.
    fn outline(&self, doc: &PagedDocument) -> Vec<OutlineNode> {
        let mut roots: Vec<OutlineNode> = Vec::new();
        let mut stack: Vec<OutlineNode> = Vec::new();
        for (_pos, item) in doc.pages().iter().flat_map(|x| x.frame.items()) {
            let FrameItem::Tag(Tag::Start(elem, _)) = item else {
                continue;
            };
            let Some(h) = elem.to_packed::<HeadingElem>() else {
                continue;
            };

            let title = h.body.plain_text().to_string();
            // Concrete level if set; synthesized tags usually carry the resolved value
            // already, fall back to 1.
            let level = h
                .level
                .as_option()
                .and_then(|l| match l {
                    typst_library::foundations::Smart::Custom(v) => Some(v.get() as u8),
                    typst_library::foundations::Smart::Auto => None,
                })
                .unwrap_or(1);

            let Some(location) = self
                .last_sources
                .as_ref()
                .and_then(|sources| sources.resolve(elem.span(), 0, &self.last_ranges))
            else {
                continue;
            };

            let node = OutlineNode {
                title,
                level,
                span: SourceSpan { file: location.file, range: location.range },
                children: Vec::new(),
            };
            while let Some(top) = stack.last() {
                if top.level >= level {
                    let done = stack.pop().unwrap();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(done);
                    } else {
                        roots.push(done);
                    }
                } else {
                    break;
                }
            }
            stack.push(node);
        }
        while let Some(node) = stack.pop() {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(node);
            } else {
                roots.push(node);
            }
        }
        roots
    }

    /// Bring a (re)connecting client up to date: the whole document for viewers, the
    /// current diagnostics and outline for editors.
    pub fn resync(&mut self, role: Role) -> Update {
        let delta = if role == Role::Viewer { self.viewer_delta() } else { None };
        Update {
            epoch: self.epoch,
            delta,
            outline: Some(self.last_outline.clone()),
            diags: self.last_failure.as_ref().unwrap_or(&self.last_warnings).clone(),
        }
    }

    /// The whole document for a viewer.
    fn viewer_delta(&mut self) -> Option<Delta> {
        if let Some(delta) = &self.fresh_full {
            return Some(delta.clone());
        }
        let (doc, model) = (self.last_doc.as_ref()?, self.last_model.as_ref()?);
        let revision = self.renderer.revision();
        self.renderer = FrameRenderer::new();
        self.renderer.set_document(Arc::clone(doc));
        self.renderer.set_revision(revision);
        let delta = self.renderer.full_sync(model);
        self.fresh_full = Some(delta.clone());
        Some(delta)
    }

    /// Run one compile cycle and return what it publishes.
    pub(crate) fn cycle(&mut self) -> Update {
        self.epoch += 1;
        let epoch = self.epoch;
        let (delta, diags) = self.compile();
        // Compilation memoizes the whole layout, so without eviction the global cache
        // keeps one full document per edit. Same policy as `typst watch`.
        comemo::evict(10);
        // Only a full sync can be handed to a viewer joining before the next cycle.
        self.fresh_full = delta.as_ref().filter(|delta| delta.reset).cloned();
        let outline = self.outline_update();
        Update { epoch, delta, outline, diags }
    }

    /// The outline to publish, if this cycle produced a new one.
    fn outline_update(&mut self) -> Option<Vec<OutlineNode>> {
        if self.last_failure.is_some() {
            return None;
        }
        let outline = self.outline(self.last_doc.as_ref()?);
        if outline == self.last_outline {
            return None;
        }
        self.last_outline = outline.clone();
        Some(outline)
    }

    /// Compile the current snapshot, save everything we need, drop the world
    fn compile(&mut self) -> (Option<Delta>, Vec<PreviewDiagnostic>) {
        let Ok(world) =
            SnapshotWorld::freeze(&self.sources, &self.base, &self.display_root)
        else {
            let diags = vec![PreviewDiagnostic {
                severity: holotyp_preview_protocol::Severity::Error,
                message: "failed to build world".into(),
                span: None,
                related: Vec::new(),
            }];
            self.last_failure = Some(diags.clone());
            return (None, diags);
        };
        let warned = typst::compile::<PagedDocument>(&world);
        let compile_warnings = project_warnings(&world, &warned);

        let outcome = match warned.output {
            Ok(doc) => {
                // The document is shared with the renderer instead of copied into it.
                let doc = Arc::new(doc);
                let new_model = DocumentModel::from(&*doc);
                self.renderer.set_document(Arc::clone(&doc));
                // First successful compile sends everything, later ones diff against the
                // previous model.
                let frames = match self.last_model.take() {
                    Some(old) => self.renderer.diff(&old, &new_model),
                    None => self.renderer.full_sync(&new_model),
                };

                self.last_jump = Some(SpanIndex::of(&doc));
                self.last_ranges = span_ranges(&world);
                self.last_sources = Some(FrozenSources::from_world(&world));
                self.last_doc = Some(doc);
                self.last_model = Some(new_model);
                self.last_failure = None;

                (Some(frames), compile_warnings.clone())
            }
            Err(errors) => {
                let diags = project_diagnostics(&world, &errors);
                self.last_failure = Some(diags.clone());
                (None, diags)
            }
        };
        self.last_warnings = compile_warnings.clone();

        // Replace the dependency set
        let requested = world.requested_paths();
        drop(world);
        self.sources.set_dependencies(requested);

        outcome
    }
}

/// Collect span -> byte range from every source in the frozen world.
fn span_ranges(world: &SnapshotWorld) -> HashMap<Span, (u32, u32)> {
    let mut out = HashMap::new();
    for (_id, source) in world.sources() {
        collect_ranges(source.root(), 0, &mut out);
    }
    out
}

fn collect_ranges(node: &SyntaxNode, offset: usize, out: &mut HashMap<Span, (u32, u32)>) {
    let span = node.span();
    if !span.is_detached() {
        out.entry(span)
            .or_insert((offset as u32, (offset + node.len()) as u32));
    }
    let mut offset = offset;
    for child in node.children() {
        collect_ranges(child, offset, out);
        offset += child.len();
    }
}

/// Warnings attached to an otherwise successful compile.
fn project_warnings(
    world: &SnapshotWorld,
    warned: &Warned<SourceResult<PagedDocument>>,
) -> Vec<PreviewDiagnostic> {
    project_diagnostics(world, &warned.warnings)
}

fn project_diagnostics(
    world: &SnapshotWorld,
    diags: &[TypstDiagnostic],
) -> Vec<PreviewDiagnostic> {
    diags
        .iter()
        .map(|d| PreviewDiagnostic {
            severity: match d.severity {
                TypstSeverity::Error => Severity::Error,
                TypstSeverity::Warning => Severity::Warning,
            },
            message: d.hints.iter().fold(d.message.to_string(), |mut message, hint| {
                message.push_str("\nHint: ");
                message.push_str(&hint.v);
                message
            }),
            span: diagnostic_span(world, d.span),
            related: d
                .trace
                .iter()
                .filter_map(|point| {
                    diagnostic_span(world, point.span.into()).map(|span| {
                        RelatedInformation { span, message: point.v.to_string() }
                    })
                })
                .collect(),
        })
        .collect()
}

fn diagnostic_span(world: &SnapshotWorld, span: DiagSpan) -> Option<SourceSpan> {
    use typst::WorldExt;
    let id = span.id()?;
    let file = world.display_path(id)?.to_string_lossy().into_owned();
    let range = world
        .range(span)
        .and_then(|r| {
            let source = world.source(id).ok()?;
            location::byte_range_to_utf16(&source, r)
        })
        .unwrap_or(Range16 {
            start: Position16 { line: 0, character: 0 },
            end: Position16 { line: 0, character: 0 },
        });
    Some(SourceSpan { file, range })
}
