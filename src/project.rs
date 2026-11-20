use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub use crate::format::DocumentKind;
use crate::{
    Id,
    format::{Frontmatter, Manifest, VERSION, split_document},
};

pub(crate) const MANIFEST: &str = "project.yaml";
pub(crate) const DIRECTORIES: [&str; 4] = ["Planung", "Manuskript", "Welt", "Brainstorming"];

pub type Result<T> = std::result::Result<T, ProjectError>;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {reason}")]
    InvalidManifest { path: PathBuf, reason: String },
    #[error("Der Zielordner ist belegt: {0}")]
    Occupied(PathBuf),
    #[error("Gib einen Buchtitel ein.")]
    EmptyTitle,
    #[error("{path}: {reason}")]
    Storage { path: PathBuf, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectIssue {
    #[error("{path}: {reason}")]
    InvalidDocument { path: PathBuf, reason: String },
    #[error("{path}: Kennung {id} kommt mehrfach vor")]
    DuplicateId { path: PathBuf, id: Id },
    #[error("{path}: Verweis auf {target} fehlt")]
    MissingReference { path: PathBuf, target: Id },
    #[error("{path}: Verweis auf {target} ist mehrdeutig")]
    AmbiguousReference { path: PathBuf, target: Id },
    #[error("{path}: {target} verweist auf ein Dokument ohne Szenentyp")]
    ExpectedScene { path: PathBuf, target: Id },
}

#[derive(Clone)]
pub struct Document {
    path: PathBuf,
    metadata: Frontmatter,
    frontmatter: String,
    body: String,
    original: String,
}

impl Document {
    pub fn id(&self) -> Id {
        self.metadata.id
    }
    pub fn kind(&self) -> DocumentKind {
        self.metadata.kind
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn title(&self) -> &str {
        self.metadata
            .title
            .as_deref()
            .or_else(|| self.path.file_stem().and_then(|stem| stem.to_str()))
            .unwrap_or("Ohne Titel")
    }
    pub fn frontmatter(&self) -> &str {
        &self.frontmatter
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub(crate) fn original(&self) -> &str {
        &self.original
    }
    pub fn scenes(&self) -> &[Id] {
        &self.metadata.scenes
    }
    pub fn references(&self) -> &[Id] {
        &self.metadata.references
    }
}

pub struct Project {
    id: Option<Id>,
    root: PathBuf,
    title: String,
    documents: BTreeMap<Id, Document>,
    issues: Vec<ProjectIssue>,
}

impl Project {
    pub fn create(root: &Path, title: &str) -> Result<Self> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ProjectError::EmptyTitle);
        }
        let manifest_path = root.join(MANIFEST);
        let yaml = serde_saphyr::to_string(&Manifest {
            title: title.into(),
            format_version: VERSION,
            id: Some(Id::new_v4()),
        })
        .map_err(|err| ProjectError::InvalidManifest {
            path: manifest_path.clone(),
            reason: err.to_string(),
        })?;

        let created_root = match fs::create_dir(root) {
            Ok(()) => true,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => false,
            Err(err) => return Err(io_error(root, err)),
        };
        let mut created_dirs = Vec::new();
        let mut created_manifest = false;
        let result = (|| {
            let mut entries = fs::read_dir(root).map_err(|err| io_error(root, err))?;
            if entries
                .next()
                .transpose()
                .map_err(|err| io_error(root, err))?
                .is_some()
            {
                return Err(ProjectError::Occupied(root.to_owned()));
            }

            let mut manifest = fs::File::create_new(&manifest_path)
                .map_err(|err| io_error(&manifest_path, err))?;
            created_manifest = true;
            for name in DIRECTORIES {
                let path = root.join(name);
                fs::create_dir(&path).map_err(|err| io_error(&path, err))?;
                created_dirs.push(path);
            }
            manifest
                .write_all(yaml.as_bytes())
                .map_err(|err| io_error(&manifest_path, err))
        })();
        if result.is_err() {
            for dir in created_dirs {
                let _ = fs::remove_dir(dir);
            }
            if created_manifest {
                let _ = fs::remove_file(&manifest_path);
            }
            if created_root {
                let _ = fs::remove_dir(root);
            }
        }
        result?;
        crate::version::initialize(root)?;
        Self::open(root)
    }

    pub fn open(root: &Path) -> Result<Self> {
        let path = root.join(MANIFEST);
        let text = fs::read_to_string(&path).map_err(|err| io_error(&path, err))?;
        let invalid = |reason| ProjectError::InvalidManifest {
            path: path.clone(),
            reason,
        };
        let manifest: Manifest =
            serde_saphyr::from_str(&text).map_err(|err| invalid(err.to_string()))?;
        if manifest.format_version != VERSION {
            return Err(invalid(format!(
                "Projektformat {} wird nicht unterstützt",
                manifest.format_version
            )));
        }
        if manifest.title.trim().is_empty() {
            return Err(invalid("Der Buchtitel fehlt".into()));
        }

        let mut paths = Vec::new();
        let mut issues = Vec::new();
        for name in DIRECTORIES {
            let dir = root.join(name);
            match dir.symlink_metadata() {
                Ok(metadata) if metadata.is_dir() => {
                    collect_documents(&dir, &mut paths, &mut issues)
                }
                Ok(_) => issues.push(invalid_document(
                    dir,
                    "Ein Projektverzeichnis wird erwartet",
                )),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => issues.push(invalid_document(dir, err)),
            }
        }
        paths.sort();
        let mut candidates = BTreeMap::<Id, Vec<Document>>::new();
        for path in paths {
            match read_document(&path) {
                Ok(document) => candidates.entry(document.id()).or_default().push(document),
                Err(reason) => issues.push(ProjectIssue::InvalidDocument { path, reason }),
            }
        }

        let mut ambiguous = BTreeSet::new();
        let mut documents = BTreeMap::new();
        for (id, mut copies) in candidates {
            if copies.len() == 1 {
                documents.insert(id, copies.remove(0));
            } else {
                ambiguous.insert(id);
                issues.extend(
                    copies
                        .into_iter()
                        .map(|document| ProjectIssue::DuplicateId {
                            path: document.path,
                            id,
                        }),
                );
            }
        }

        for document in documents.values() {
            let chapter_scenes = document.scenes().iter().map(|&id| (id, true));
            let references = document.references().iter().map(|&id| (id, false));
            for (target, scene_required) in chapter_scenes.chain(references) {
                let path = || document.path.clone();
                let issue = if ambiguous.contains(&target) {
                    ProjectIssue::AmbiguousReference {
                        path: path(),
                        target,
                    }
                } else if let Some(found) = documents.get(&target) {
                    if scene_required && found.kind() != DocumentKind::Scene {
                        ProjectIssue::ExpectedScene {
                            path: path(),
                            target,
                        }
                    } else {
                        continue;
                    }
                } else {
                    ProjectIssue::MissingReference {
                        path: path(),
                        target,
                    }
                };
                issues.push(issue);
            }
        }

        Ok(Self {
            root: root.into(),
            title: manifest.title,
            id: manifest.id,
            documents,
            issues,
        })
    }

    pub fn id(&self) -> Option<Id> {
        self.id
    }
    pub fn documents(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn document(&self, id: Id) -> Option<&Document> {
        self.documents.get(&id)
    }
    pub fn scene(&self, id: Id) -> Option<&Document> {
        self.document(id)
            .filter(|document| document.kind() == DocumentKind::Scene)
    }
    pub fn scenes(&self) -> impl Iterator<Item = &Document> {
        self.documents
            .values()
            .filter(|document| document.kind() == DocumentKind::Scene)
    }
    pub fn issues(&self) -> &[ProjectIssue] {
        &self.issues
    }
    pub fn has_missing_ref(&self, id: Id) -> bool {
        self.issues.iter().any(
            |issue| matches!(issue, ProjectIssue::MissingReference { target, .. } if *target == id),
        )
    }
}

pub(crate) fn io_error(path: &Path, source: io::Error) -> ProjectError {
    ProjectError::Io {
        path: path.into(),
        source,
    }
}

fn invalid_document(path: PathBuf, error: impl ToString) -> ProjectIssue {
    ProjectIssue::InvalidDocument {
        path,
        reason: error.to_string(),
    }
}

fn collect_documents(dir: &Path, paths: &mut Vec<PathBuf>, issues: &mut Vec<ProjectIssue>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            issues.push(invalid_document(dir.into(), err));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                issues.push(invalid_document(dir.into(), err));
                continue;
            }
        };
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => collect_documents(&path, paths, issues),
            Ok(kind)
                if kind.is_file()
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md")) =>
            {
                paths.push(path)
            }
            Ok(_) => {}
            Err(err) => issues.push(invalid_document(path, err)),
        }
    }
}

fn read_document(path: &Path) -> std::result::Result<Document, String> {
    let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let (frontmatter, body) =
        split_document(&text).ok_or("Die YAML-Metadaten müssen von --- umschlossen sein")?;
    let metadata = serde_saphyr::from_str(frontmatter).map_err(|err| err.to_string())?;
    Ok(Document {
        path: path.into(),
        metadata,
        frontmatter: frontmatter.into(),
        body: body.into(),
        original: text,
    })
}
