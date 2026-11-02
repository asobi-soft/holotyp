//! A multi-file [`World`] over a [`SystemWorld`] and a [`SourceSet`] snapshot.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};

use crate::preview::source::SourceSet;
use crate::world::SystemWorld;

pub struct SnapshotWorld<'a> {
    base: &'a SystemWorld,
    sources: &'a SourceSet,
    main: FileId,
    display_root: &'a Path,
    requested_sources: Mutex<HashMap<FileId, FileResult<Source>>>,
    requested: Mutex<BTreeSet<PathBuf>>,
}

impl<'a> SnapshotWorld<'a> {
    /// Every file this compilation read.
    pub fn sources(&self) -> impl Iterator<Item = (FileId, Source)> {
        self.requested_sources
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, result)| {
                result.as_ref().ok().map(|source| (*id, source.clone()))
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn freeze(
        sources: &'a SourceSet,
        base: &'a SystemWorld,
        display_root: &'a Path,
    ) -> FileResult<Self> {
        // main
        let main_abs = sources.main_path();
        let main_vpath = VirtualPath::virtualize(base.root(), main_abs)
            .map_err(|_| FileError::AccessDenied)?;
        let main = RootedPath::new(VirtualRoot::Project, main_vpath).intern();

        // The main file must have text right now; if it does not, the watcher wakes us
        // once it appears.
        let has_text = sources
            .effective_text(main_abs)
            .map_err(|err| FileError::from_io(err, main_abs))?
            .is_some();
        if !has_text {
            return Err(FileError::NotFound(main_abs.to_path_buf()));
        }

        Ok(Self {
            base,
            sources,
            main,
            display_root,
            requested_sources: Mutex::new(HashMap::new()),
            requested: Mutex::new(BTreeSet::new()),
        })
    }

    /// Every requested dependency, including missing imports (so a later file creation
    /// can wake a failed session through the file watcher).
    pub fn requested_paths(&self) -> Vec<PathBuf> {
        self.requested.lock().unwrap().iter().cloned().collect()
    }

    pub fn file_path(&self, id: FileId) -> Option<PathBuf> {
        self.disk_path(id).ok()
    }

    /// The path to report to the editor for `id`.
    pub fn display_path(&self, id: FileId) -> Option<PathBuf> {
        match id.root() {
            VirtualRoot::Project => id.vpath().realize(self.display_root).ok(),
            VirtualRoot::Package(_) => self.file_path(id),
        }
    }

    // Resolve a FileId to disk path and insert it into `requested`
    fn disk_path(&self, id: FileId) -> FileResult<PathBuf> {
        let root = self.base.root();
        let path = match id.root() {
            // package file: the base world's package storage knows the root
            VirtualRoot::Package(_) => self.base.resolve(id)?,
            // project file
            VirtualRoot::Project => {
                let path =
                    id.vpath().realize(root).map_err(|_| FileError::AccessDenied)?;

                // reject escaping root
                if let Ok(real) = path.canonicalize() {
                    let root = root
                        .canonicalize()
                        .map_err(|err| FileError::from_io(err, root))?;
                    if !real.starts_with(root) {
                        return Err(FileError::AccessDenied);
                    }
                }
                path
            }
        };

        self.requested.lock().unwrap().insert(path.clone());
        Ok(path)
    }
}

impl World for SnapshotWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.base.library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.base.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        let mut requested_sources = self.requested_sources.lock().unwrap();
        requested_sources
            .entry(id)
            .or_insert_with(|| {
                let path = self.disk_path(id)?;
                self.sources
                    .load_source(id, &path)
                    .map_err(|err| FileError::from_io(err, &path))
            })
            .clone()
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = self.disk_path(id)?;
        self.sources
            .load_bytes(&path)
            .map_err(|err| FileError::from_io(err, &path))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.base.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.base.today(offset)
    }
}
