use std::{
    env,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::project::Page;
use writer::{Id, Project};

const MAX_RECENT_PROJECTS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Location {
    pub page: Page,
    pub scene: Option<Id>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RecentProject {
    pub root: PathBuf,
    pub title: String,
    pub location: Location,
}

#[derive(Default)]
pub(super) struct RecentProjects {
    file: Option<PathBuf>,
    entries: Vec<RecentProject>,
}

impl RecentProjects {
    pub fn default_path() -> Option<PathBuf> {
        let home = |key| {
            env::var_os(key)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let base = if cfg!(windows) {
            home("LOCALAPPDATA")
        } else if cfg!(target_os = "macos") {
            home("HOME").map(|path| path.join("Library/Application Support"))
        } else {
            home("XDG_CONFIG_HOME").or_else(|| home("HOME").map(|path| path.join(".config")))
        };
        base.map(|path| path.join("Schreibwerkstatt").join("recent-projects.json"))
    }

    pub fn read(file: PathBuf) -> Self {
        let entries = File::open(&file)
            .ok()
            .and_then(|file| serde_json::from_reader::<_, Vec<RecentProject>>(file).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| entry.root.is_absolute())
            .take(MAX_RECENT_PROJECTS)
            .collect();
        Self {
            file: Some(file),
            entries,
        }
    }

    pub fn entries(&self) -> &[RecentProject] {
        &self.entries
    }

    pub fn location(&self, root: &Path) -> Option<Location> {
        self.entries
            .iter()
            .find(|entry| entry.root == root)
            .map(|entry| entry.location)
    }

    pub fn remember(&mut self, project: &Project, location: Location) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        let entry = RecentProject {
            root: project.root().to_owned(),
            title: project.title().into(),
            location,
        };
        if self.entries.first() == Some(&entry) {
            return Ok(());
        }
        self.entries.retain(|old| old.root != entry.root);
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_RECENT_PROJECTS);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            serde_json::to_writer(&mut temp, &self.entries)?;
            temp.as_file().sync_all()?;
            temp.persist(file)?;
        }
        Ok(())
    }
}
