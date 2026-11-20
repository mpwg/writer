use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use serde::Serialize;
use serde_json::{Map, Value};
use tempfile::NamedTempFile;

use crate::{
    Document, ProjectError, Result,
    format::{Frontmatter, Manifest, split_document},
    project::{DIRECTORIES, MANIFEST, io_error},
    version,
};

pub use version::Checkpoint;

#[derive(Clone)]
struct Change {
    expected: Option<Vec<u8>>,
    bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct Conflict {
    pub path: PathBuf,
    pub on_disk: Option<Vec<u8>>,
    pub in_app: Vec<u8>,
}

impl Conflict {
    pub fn validate_merge(&self, bytes: &[u8]) -> Result<()> {
        let invalid = |reason: &str| storage_error(&self.path, reason);
        let text = str::from_utf8(bytes).map_err(|error| storage_error(&self.path, error))?;
        if self.path == Path::new(MANIFEST) {
            let merged: Manifest =
                serde_saphyr::from_str(text).map_err(|error| storage_error(&self.path, error))?;
            let original: Manifest = serde_saphyr::from_slice(&self.in_app)
                .map_err(|error| storage_error(&self.path, error))?;
            if merged.id != original.id || merged.format_version != original.format_version {
                return Err(invalid(
                    "Buchkennung und Formatversion müssen erhalten bleiben",
                ));
            }
        } else {
            let metadata = |text: &str| -> Result<Frontmatter> {
                let (yaml, _) = split_document(text)
                    .ok_or_else(|| invalid("Die YAML-Metadaten müssen von --- umschlossen sein"))?;
                serde_saphyr::from_str(yaml).map_err(|error| storage_error(&self.path, error))
            };
            let merged = metadata(text)?;
            let original =
                str::from_utf8(&self.in_app).map_err(|error| storage_error(&self.path, error))?;
            let original = metadata(original)?;
            if merged.id != original.id || merged.kind != original.kind {
                return Err(invalid(
                    "Dokumentkennung und Dokumenttyp müssen erhalten bleiben",
                ));
            }
        }
        Ok(())
    }
}

pub enum ConflictResolution {
    InApp,
    OnDisk,
    Merged(Vec<u8>),
}

pub struct Store {
    root: PathBuf,
    pending: BTreeMap<PathBuf, Change>,
    conflict: Option<Conflict>,
}

impl Store {
    pub fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(|error| io_error(root, error))?;
        Ok(Self {
            root,
            pending: BTreeMap::new(),
            conflict: None,
        })
    }

    pub fn stage(
        &mut self,
        relative: &Path,
        expected: Option<Vec<u8>>,
        bytes: Vec<u8>,
    ) -> Result<()> {
        if !is_book_path(relative) {
            return Err(storage_error(relative, "Ungültiger Buchpfad"));
        }
        self.pending
            .entry(relative.to_owned())
            .and_modify(|change| change.bytes.clone_from(&bytes))
            .or_insert(Change { expected, bytes });
        Ok(())
    }

    pub fn stage_batch(&mut self, stage: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let pending = self.pending.clone();
        if let Err(error) = stage(self) {
            self.pending = pending;
            return Err(error);
        }
        Ok(())
    }

    pub fn stage_document(
        &mut self,
        relative: &Path,
        previous: Option<&Document>,
        fields: impl Serialize,
        body: &str,
    ) -> Result<()> {
        let mut metadata: Map<String, Value> = match previous {
            Some(document) => serde_saphyr::from_str(document.frontmatter())
                .map_err(|error| storage_error(relative, error))?,
            None => Map::new(),
        };
        let fields: Map<String, Value> = serde_json::to_value(fields)
            .and_then(serde_json::from_value)
            .map_err(|error| storage_error(relative, error))?;
        metadata.extend(fields);
        let frontmatter =
            serde_saphyr::to_string(&metadata).map_err(|error| storage_error(relative, error))?;
        let bytes = format!("---\n{frontmatter}---\n{body}").into_bytes();
        self.stage(
            relative,
            previous.map(|document| document.original().as_bytes().to_vec()),
            bytes,
        )
    }

    pub fn stage_body(&mut self, relative: &Path, document: &Document, body: &str) -> Result<()> {
        let original = document.original();
        let prefix = &original[..original.len() - document.body().len()];
        self.stage(
            relative,
            Some(original.as_bytes().to_vec()),
            format!("{prefix}{body}").into_bytes(),
        )
    }

    pub fn relative_path(&self, document: &Document) -> Result<PathBuf> {
        let path = document.path();
        let absolute = std::path::absolute(path).map_err(|error| io_error(path, error))?;
        absolute
            .strip_prefix(&self.root)
            .map(Path::to_owned)
            .or_else(|_| {
                // Fall back to the canonical parent when the book was opened through an alias.
                let parent = absolute
                    .parent()
                    .ok_or_else(|| storage_error(path, "Projektpfad fehlt"))?;
                let name = absolute
                    .file_name()
                    .ok_or_else(|| storage_error(path, "Dateiname fehlt"))?;
                parent
                    .canonicalize()
                    .ok()
                    .and_then(|parent| {
                        parent
                            .strip_prefix(&self.root)
                            .ok()
                            .map(|relative| relative.join(name))
                    })
                    .ok_or_else(|| storage_error(path, "Datei liegt außerhalb des Buchordners"))
            })
    }

    pub fn conflict(&self) -> Option<&Conflict> {
        self.conflict.as_ref()
    }
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn latest_version_time(&self) -> Option<String> {
        version::latest_version_time(&self.root)
    }

    pub fn has_unversioned_changes(&self) -> Result<bool> {
        version::has_unversioned_changes(&self.root)
    }

    pub fn discard_pending(&mut self) {
        self.pending.clear();
        self.conflict = None;
    }

    pub fn resolve(&mut self, resolution: ConflictResolution) -> Result<()> {
        if let (Some(conflict), ConflictResolution::Merged(bytes)) = (&self.conflict, &resolution) {
            conflict.validate_merge(bytes)?;
        }
        let Some(conflict) = self.conflict.take() else {
            return Ok(());
        };
        if let Some(change) = self.pending.get_mut(&conflict.path) {
            match resolution {
                ConflictResolution::OnDisk => {
                    self.pending.remove(&conflict.path);
                }
                ConflictResolution::InApp => change.expected = conflict.on_disk,
                ConflictResolution::Merged(bytes) => {
                    change.expected = conflict.on_disk;
                    change.bytes = bytes;
                }
            }
        }
        self.flush()
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let _lock = lock(&self.root)?;
        self.flush_locked()
    }

    pub fn checkpoint(&mut self, message: &str) -> Result<Checkpoint> {
        let _lock = lock(&self.root)?;
        self.flush_locked()?;
        version::initialize(&self.root)?;
        version::checkpoint(&self.root, message)
    }

    fn flush_locked(&mut self) -> Result<()> {
        self.conflict = None;
        // Check the entire batch before writing so a known conflict leaves the batch pending.
        for (relative, change) in &self.pending {
            let path = checked_path(&self.root, relative)?;
            let current = read_optional(&path)?;
            if current != change.expected && current.as_deref() != Some(change.bytes.as_slice()) {
                self.conflict = Some(Conflict {
                    path: relative.clone(),
                    on_disk: current,
                    in_app: change.bytes.clone(),
                });
                return Err(storage_error(
                    &path,
                    "Die Datei wurde außerhalb der App geändert",
                ));
            }
        }
        while let Some((relative, change)) = self.pending.first_key_value() {
            let path = checked_path(&self.root, relative)?;
            let current = read_optional(&path)?;
            guard_change(&path, current.as_deref(), change).inspect_err(|_| {
                self.conflict = Some(Conflict {
                    path: relative.clone(),
                    on_disk: current.clone(),
                    in_app: change.bytes.clone(),
                });
            })?;
            if current.as_deref() != Some(change.bytes.as_slice()) {
                if current.is_none()
                    && let Some(parent) = path.parent()
                {
                    // Git checkouts omit managed directories until they contain a file.
                    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
                }
                write_atomic(&path, &change.bytes, current.as_deref())?;
            }
            let relative = relative.clone();
            self.pending.remove(&relative);
        }
        Ok(())
    }
}

fn guard_change(path: &Path, current: Option<&[u8]>, change: &Change) -> Result<()> {
    if current != change.expected.as_deref() && current != Some(change.bytes.as_slice()) {
        return Err(storage_error(
            path,
            "Die Datei wurde außerhalb der App geändert",
        ));
    }
    Ok(())
}

fn lock(root: &Path) -> Result<File> {
    let path = root.join(".writer.lock");
    let file = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| io_error(&path, error))?;
    file.lock().map_err(|error| io_error(&path, error))?;
    Ok(file)
}

pub(crate) fn is_book_path(path: &Path) -> bool {
    if path == Path::new(MANIFEST) {
        return true;
    }
    let mut components = path.components();
    matches!(components.next(), Some(Component::Normal(name)) if DIRECTORIES.iter().any(|dir| name == *dir))
        && components.clone().next().is_some()
        && components.all(|part| matches!(part, Component::Normal(_)))
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut path = root.to_owned();
    for part in relative.components() {
        path.push(part);
        match path.symlink_metadata() {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(storage_error(&path, "Symbolische Verknüpfung im Buchpfad"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&path, error)),
        }
    }
    Ok(path)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(path, error)),
    }
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8], expected: Option<&[u8]>) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| storage_error(path, "Zielordner fehlt"))?;
    let mut temp = NamedTempFile::new_in(parent).map_err(|error| io_error(path, error))?;
    if expected.is_some() {
        let permissions = fs::metadata(path)
            .map_err(|error| io_error(path, error))?
            .permissions();
        temp.as_file()
            .set_permissions(permissions)
            .map_err(|error| io_error(path, error))?;
    }
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|error| io_error(path, error))?;
    if read_optional(path)?.as_deref() != expected {
        return Err(storage_error(
            path,
            "Die Datei wurde während des Sicherns geändert",
        ));
    }
    if expected.is_some() {
        temp.persist(path)
    } else {
        temp.persist_noclobber(path)
    }
    .map_err(|error| io_error(path, error.error))?;
    Ok(())
}

pub(crate) fn storage_error(path: &Path, reason: impl ToString) -> ProjectError {
    ProjectError::Storage {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}
