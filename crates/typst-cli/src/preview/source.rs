//! The sources the preview host compiles from: files on disk plus the editor's unsaved
//! buffers, kept as one snapshot.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use typst::foundations::Bytes;
use typst::syntax::{FileId, Source};

/// A file's bytes plus the metadata that tells us whether it moved.
#[derive(Debug, Clone)]
struct DiskFile {
    bytes: Vec<u8>,
    stamp: Stamp,
}

/// What the last read from disk returned for one path.
#[derive(Debug, Clone)]
enum Disk {
    /// The file's bytes.
    File(DiskFile),
    /// The path exists but cannot be read as a file: it is a directory, it is not
    /// readable, it is a broken link, ... The error is kept so compilation can report
    /// what actually went wrong instead of guessing "not found".
    Unreadable(io::ErrorKind),
}

impl Disk {
    /// The bytes of a readable file.
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::File(file) => Some(&file.bytes),
            Self::Unreadable(_) => None,
        }
    }

    /// The error this path produced, if it could not be read.
    fn error(&self) -> Option<io::Error> {
        match self {
            Self::File(_) => None,
            Self::Unreadable(kind) => Some(io::Error::from(*kind)),
        }
    }
}

/// Cheap stat fingerprint of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    mtime_ms: u64,
    len: u64,
}

/// The editor's unsaved buffer for one file.
#[derive(Debug, Clone)]
struct Overlay {
    version: u64,
    text: String,
}

/// One tracked file: what disk has, and whether the editor overrides it.
#[derive(Debug, Default)]
struct Slot {
    /// The last compilation read this file (or tried to).
    dependency: bool,
    /// What the last read from disk returned. `None` means this file has not been read
    /// (yet)
    disk: Option<Disk>,
    overlay: Option<Overlay>,
}

impl Slot {
    /// The bytes compilation sees.
    fn effective(&self) -> Option<&[u8]> {
        match &self.overlay {
            Some(overlay) => Some(overlay.text.as_bytes()),
            None => self.disk.as_ref().and_then(Disk::bytes),
        }
    }
}

/// The project's sources: disk state plus editor overlays.
#[derive(Debug)]
pub struct SourceSet {
    main: PathBuf,
    files: BTreeMap<PathBuf, Slot>,
    /// Parsed sources survive snapshots so unchanged syntax keeps its spans and caches.
    parsed: Mutex<HashMap<FileId, (PathBuf, Source)>>,
    loaded: Mutex<HashMap<PathBuf, Bytes>>,
    parsed_valid: Mutex<HashSet<FileId>>,
}

impl SourceSet {
    pub fn new(main: PathBuf) -> Self {
        let main = files_key(&main);
        let mut files = BTreeMap::new();
        files.insert(main.clone(), Slot::default());
        Self {
            main,
            files,
            parsed: Mutex::default(),
            loaded: Mutex::default(),
            parsed_valid: Mutex::default(),
        }
    }

    /// The absolute path of the main source file.
    pub fn main_path(&self) -> &Path {
        &self.main
    }

    /// Every path the file watcher must watch: dependencies of the last compilation
    /// (including imports that do not exist yet), editor overlay files, and the main
    /// file.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.keys().cloned().collect()
    }

    /// Replace the dependency set with what the last compilation read.
    pub fn set_dependencies(&mut self, deps: impl IntoIterator<Item = PathBuf>) {
        for slot in self.files.values_mut() {
            slot.dependency = false;
        }
        for path in deps {
            self.files.entry(files_key(&path)).or_default().dependency = true;
        }
        self.sweep();
    }

    /// Drop entries nothing keeps alive: not a dependency, no overlay, not main.
    fn sweep(&mut self) {
        let main = &self.main;
        self.files.retain(|path, slot| {
            path == main || slot.dependency || slot.overlay.is_some()
        });
        self.loaded
            .get_mut()
            .unwrap()
            .retain(|path, _| self.files.contains_key(path));
        let parsed = self.parsed.get_mut().unwrap();
        parsed.retain(|_, (path, _)| self.files.contains_key(path));
        self.parsed_valid
            .get_mut()
            .unwrap()
            .retain(|id| parsed.contains_key(id));
    }

    /// Drop what the caches hold for one path
    fn invalidate(&mut self, path: &Path) {
        self.loaded.get_mut().unwrap().remove(path);
        let parsed = self.parsed.get_mut().unwrap();
        let parsed_valid = self.parsed_valid.get_mut().unwrap();
        parsed_valid.retain(|id| {
            parsed.get(id).is_some_and(|(cached, _)| cached.as_path() != path)
        });
    }

    /// Re-read every tracked file from disk. Returns whether any of them changed.
    pub fn refresh(&mut self) -> bool {
        let paths: Vec<PathBuf> = self.files.keys().cloned().collect();
        let mut changed = false;
        for path in paths {
            if self.refresh_one(&path) {
                self.invalidate(&path);
                changed = true;
            }
        }
        changed
    }

    /// Re-read one file if its metadata moved.
    fn refresh_one(&mut self, path: &Path) -> bool {
        // triggered by `refresh` on `files` keys, so there's no need to use `files_key`
        let Some(stamp) = stamp_of(path) else {
            // file does not exist on disk
            let had_disk = self.files.get(path).is_some_and(|slot| slot.disk.is_some());
            return had_disk && self.set_disk(path.to_path_buf(), None);
        };

        // A dependency can turn into something that cannot be read.
        let disk = match std::fs::read(path) {
            Ok(bytes) => Disk::File(DiskFile { bytes, stamp }),
            Err(err) => Disk::Unreadable(err.kind()),
        };

        let slot = self.files.get_mut(path).expect("refresh only walks tracked paths");
        let before = slot.effective().map(<[u8]>::to_vec);
        let changed = match (&slot.disk, &disk) {
            // unchanged file
            (Some(Disk::File(old)), Disk::File(new)) if old.stamp == new.stamp => false,
            // the same read error, nothing to do
            (Some(Disk::Unreadable(old)), Disk::Unreadable(new)) if old == new => false,
            // first read of a file: only a readable file has content to publish
            (None, Disk::File(_)) => slot.overlay.is_none(),
            // first read of an unreadable path: compilation reports the error
            (None, Disk::Unreadable(_)) => false,
            // content or error changed
            _ => true,
        };
        if changed {
            slot.disk = Some(disk);
        }
        changed && before.as_deref() != slot.effective()
    }

    /// Apply one file's disk state; `None` means the file is gone.
    ///
    /// Only the disk layer changes here. The editor's buffer stays.
    fn set_disk(&mut self, path: PathBuf, disk: Option<Disk>) -> bool {
        // triggered by `refresh_one` on `files` keys, so there's no need to use
        // `files_key`
        let Some(slot) = self.files.get_mut(&path) else { return false };
        let before = slot.effective().map(<[u8]>::to_vec);
        slot.disk = disk;
        before.as_deref() != slot.effective()
    }

    /// Apply an editor buffer update.
    pub fn set_overlay(
        &mut self,
        file: &str,
        version: u64,
        text: Option<String>,
    ) -> bool {
        let path = files_key(Path::new(file));
        self.invalidate(&path);
        let released = text.is_none();
        let changed = {
            let slot = self.files.entry(path).or_default();
            let before = slot.effective().map(<[u8]>::to_vec);
            match text {
                Some(text) => {
                    if slot.overlay.as_ref().is_some_and(|o| version <= o.version) {
                        return false;
                    }
                    slot.overlay = Some(Overlay { version, text });
                }
                None => slot.overlay = None,
            }
            before.as_deref() != slot.effective()
        };
        // A released overlay leaves nothing behind for a file that is not (or no longer)
        // an import.
        if released {
            self.sweep();
        }
        changed
    }

    /// Forget every buffer
    pub fn clear_overlays(&mut self) -> bool {
        let paths: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|(_, slot)| slot.overlay.is_some())
            .map(|(path, _)| path.clone())
            .collect();
        let mut changed = false;
        for path in paths {
            let released = {
                let Some(slot) = self.files.get_mut(&path) else { continue };
                let before = slot.effective().map(<[u8]>::to_vec);
                slot.overlay = None;
                before.as_deref() != slot.effective()
            };
            if released {
                self.invalidate(&path);
                changed = true;
            }
        }
        self.sweep();
        changed
    }

    /// `Ok(None)` means this source set has no text to offer because the file is not
    /// tracked.
    pub fn effective_text(&self, path: &Path) -> std::io::Result<Option<String>> {
        match self.effective_bytes(path) {
            Some(bytes) => Self::parse_text(bytes).map(Some),
            None => Ok(None),
        }
    }

    /// The text of a file the world resolved, reading the disk only for files this source
    /// set has never seen.
    pub fn load_text(&self, path: &Path) -> std::io::Result<String> {
        match self.held(path) {
            Some(Some(bytes)) => Self::parse_text(bytes),
            Some(None) => Err(self.read_error(path)),
            None => std::fs::read_to_string(path),
        }
    }

    /// Load the current text, incrementally updating the previous parsed source. Old
    /// world/jump snapshots keep their own immutable clones.
    pub fn load_source(&self, id: FileId, path: &Path) -> std::io::Result<Source> {
        let mut parsed = self.parsed.lock().unwrap();
        let mut parsed_valid = self.parsed_valid.lock().unwrap();
        if parsed_valid.contains(&id)
            && let Some((_, source)) = parsed.get(&id)
        {
            return Ok(source.clone());
        }
        let text = self.load_text(path)?;
        let (_, source) = parsed
            .entry(id)
            .and_modify(|(_, source)| {
                source.replace(&text);
            })
            .or_insert_with(|| (files_key(path), Source::new(id, text)));
        parsed_valid.insert(id);
        Ok(source.clone())
    }

    /// Interpret held bytes as text, the way reading the file would.
    fn parse_text(bytes: Vec<u8>) -> std::io::Result<String> {
        String::from_utf8(bytes)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
    }

    /// Bytes for a file the world resolved, reading the disk only for files this source
    /// set has never seen.
    pub fn load_bytes(&self, path: &Path) -> std::io::Result<Bytes> {
        let key = files_key(path);
        let mut loaded = self.loaded.lock().unwrap();
        if let Some(bytes) = loaded.get(&key) {
            return Ok(bytes.clone());
        }
        let bytes = Bytes::new(match self.held(&key) {
            Some(Some(bytes)) => bytes,
            Some(None) => return Err(self.read_error(&key)),
            None => std::fs::read(path)?,
        });
        loaded.insert(key, bytes.clone());
        Ok(bytes)
    }

    /// The bytes this source set holds for a path, if it holds any.
    fn effective_bytes(&self, path: &Path) -> Option<Vec<u8>> {
        self.held(path).flatten()
    }

    /// What this source set holds for a path: `None` when the file is not tracked, the
    /// bytes when it has some, and `Some(None)` when the last read of a tracked file
    /// failed.
    fn held(&self, path: &Path) -> Option<Option<Vec<u8>>> {
        let slot = self.files.get(&files_key(path))?;
        if let Some(overlay) = &slot.overlay {
            return Some(Some(overlay.text.as_bytes().to_vec()));
        }
        match slot.disk.as_ref()? {
            Disk::File(file) => Some(Some(file.bytes.clone())),
            Disk::Unreadable(_) => Some(None),
        }
    }

    /// The error the last read of a tracked path produced.
    fn read_error(&self, path: &Path) -> std::io::Error {
        self.files
            .get(&files_key(path))
            .and_then(|slot| slot.disk.as_ref())
            .and_then(Disk::error)
            .unwrap_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
    }
}

/// The one identity convention for the file map: the file's real path.
///
/// Editors may spell a path through a symlink (a workspace opened via one), packages and
/// imports may reach the same file by another spelling, and Windows gives `\\?\C:\...`
/// for canonical form. All of that is identity only: what we report back to the editor
/// uses the root as it was given (see
/// [`crate::preview::world::SnapshotWorld::display_path`]).
pub(crate) fn files_key(path: &Path) -> PathBuf {
    assert!(path.is_absolute(), "file keys must be absolute, got {}", path.display());
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Stat fingerprint of a path, or `None` if it does not exist.
fn stamp_of(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp {
        mtime_ms: meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_millis() as u64),
        len: meta.len(),
    })
}
