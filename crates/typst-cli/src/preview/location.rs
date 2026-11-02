//! Frozen source locations used by preview/editor jumps.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use holotyp_preview_protocol::{Position16, Range16};
use typst::syntax::{FileId, Source, Span};

use crate::preview::source::files_key;
use crate::preview::world::SnapshotWorld;

#[derive(Debug, Clone)]
pub(crate) struct FrozenSources {
    by_id: HashMap<FileId, Source>,
    /// What the editor is told, i.e. [`SnapshotWorld::display_path`].
    display_paths: HashMap<FileId, PathBuf>,
    /// The same files as identities ([`crate::preview::source::files_key`]), for matching
    /// paths that arrive from the editor.
    keys: HashMap<FileId, PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct SpanLocation {
    pub file: String,
    pub range: Range16,
    pub byte_range: (u32, u32),
}

impl FrozenSources {
    pub fn from_world(world: &SnapshotWorld) -> Self {
        let mut by_id = HashMap::new();
        let mut display_paths = HashMap::new();
        let mut keys = HashMap::new();
        for (id, source) in world.sources() {
            if let Some(path) = world.file_path(id) {
                keys.insert(id, path);
            }
            if let Some(path) = world.display_path(id) {
                display_paths.insert(id, path);
            }
            by_id.insert(id, source.clone());
        }
        Self { by_id, display_paths, keys }
    }

    pub fn source(&self, id: FileId) -> Option<&Source> {
        self.by_id.get(&id)
    }

    pub fn path(&self, id: FileId) -> Option<PathBuf> {
        self.display_paths.get(&id).cloned()
    }

    pub fn id_of(&self, path: &Path) -> Option<FileId> {
        if !path.is_absolute() {
            return None;
        }
        let key = files_key(path);
        self.keys
            .iter()
            .find(|(_, known)| known.as_path() == key)
            .map(|(id, _)| *id)
    }

    pub fn resolve(
        &self,
        span: Span,
        offset: u16,
        ranges: &HashMap<Span, (u32, u32)>,
    ) -> Option<SpanLocation> {
        let id = span.id()?;
        let source = self.source(id)?;
        let (stored_start, stored_end) = match ranges.get(&span).copied() {
            Some(range) => range,
            // not a node
            None => {
                let range = source.find(span)?.range();
                (range.start as u32, range.end as u32)
            }
        };
        let start = stored_start as usize;
        let end = (stored_end as usize).max(start);
        let byte_start = start.saturating_add(offset as usize).min(end);
        let byte_range = (byte_start as u32, end as u32);
        let range = byte_range_to_utf16(source, byte_start..end)?;
        let file = self.path(id)?.to_string_lossy().into_owned();
        Some(SpanLocation { file, range, byte_range })
    }
}

/// Convert a UTF-16 editor position to a UTF-8 byte offset in a frozen source.
pub(crate) fn position_to_byte(
    source: &Source,
    line: u32,
    character: u32,
) -> Option<usize> {
    let line_range = source.lines().line_to_range(line as usize)?;
    let content = source.text().get(line_range.clone())?.trim_end_matches(['\r', '\n']);
    if character as usize > content.encode_utf16().count() {
        return None;
    }
    let line_utf16_start = source.lines().byte_to_utf16(line_range.start)?;
    let target = line_utf16_start.checked_add(character as usize)?;
    let byte = source.lines().utf16_to_byte(target)?;
    (source.lines().byte_to_utf16(byte)? == target).then_some(byte)
}

/// Convert a byte range to LSP-style UTF-16 line/column positions.
pub(crate) fn byte_range_to_utf16(
    source: &Source,
    range: Range<usize>,
) -> Option<Range16> {
    let lines = source.lines();
    let start_line = lines.byte_to_line(range.start)?;
    let end_line = lines.byte_to_line(range.end)?;
    let start_column = lines.byte_to_utf16(range.start)?
        - lines.byte_to_utf16(lines.line_to_byte(start_line)?)?;
    let end_column = lines.byte_to_utf16(range.end)?
        - lines.byte_to_utf16(lines.line_to_byte(end_line)?)?;
    Some(Range16 {
        start: Position16 {
            line: start_line as u32,
            character: start_column as u32,
        },
        end: Position16 {
            line: end_line as u32,
            character: end_column as u32,
        },
    })
}

/// Distance from a position to a span in UTF-16 units. Zero means covered.
pub(crate) fn position_distance(
    source: &Source,
    target: usize,
    range: Range<usize>,
) -> Option<usize> {
    let target = source.lines().byte_to_utf16(target)?;
    let start = source.lines().byte_to_utf16(range.start)?;
    let end = source.lines().byte_to_utf16(range.end)?;
    Some(if target < start { start - target } else { target.saturating_sub(end) })
}
