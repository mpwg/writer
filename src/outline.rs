use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{Document, DocumentKind, Id, Project, ProjectError, storage::Store};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SceneTiming {
    pub after: Option<Id>,
    pub before: Option<Id>,
    pub time: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scene {
    pub id: Id,
    pub title: String,
    pub bullets: String,
    pub event: Option<Id>,
    pub characters: Vec<Id>,
    pub timing: SceneTiming,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chapter {
    pub id: Id,
    pub title: String,
    pub scenes: Vec<Id>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OutlineError {
    #[error("Szene {0} fehlt")]
    MissingScene(Id),
    #[error("Kapitel {0} fehlt")]
    MissingChapter(Id),
    #[error("Position {index} liegt außerhalb der zulässigen Positionen 0 bis {len}")]
    InvalidPosition { index: usize, len: usize },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OutlineIssue {
    #[error(
        "Die Gliederungsangaben von Dokument {document} konnten nicht gelesen werden: {reason}"
    )]
    InvalidMetadata { document: Id, reason: String },
    #[error("Szene {scene} steht mehrfach im Kapitelplan")]
    RepeatedScene { scene: Id },
}

#[derive(Default, Deserialize)]
struct SceneMetadata {
    scene_event: Option<Id>,
    #[serde(default)]
    characters: Vec<Id>,
    #[serde(default)]
    scene_timing: StoredSceneTiming,
}

#[derive(Default, Deserialize, Serialize)]
struct StoredSceneTiming {
    #[serde(flatten)]
    timing: SceneTiming,
    #[serde(flatten)]
    other: BTreeMap<String, serde_json::Value>,
}

#[derive(Default, Deserialize)]
struct ChapterMetadata {
    chapter_order: Option<usize>,
}

#[derive(Default, Serialize)]
struct SceneFields<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scene_event: Option<Option<Id>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scene_timing: Option<StoredSceneTiming>,
    #[serde(skip_serializing_if = "Option::is_none")]
    characters: Option<&'a [Id]>,
}

#[derive(Default, Serialize)]
struct ChapterFields<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chapter_order: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenes: Option<&'a [Id]>,
}

#[derive(Default)]
pub struct Outline {
    scenes: BTreeMap<Id, Scene>,
    chapters: Vec<Chapter>,
    metadata_issues: Vec<OutlineIssue>,
}

impl Outline {
    pub fn from_project(project: &Project) -> Self {
        let mut outline = Self::default();
        let mut chapters = Vec::new();
        for document in project.documents() {
            let id = document.id();
            match document.kind() {
                DocumentKind::Scene => {
                    let metadata: SceneMetadata =
                        outline.parse_metadata(id, document.frontmatter());
                    outline.scenes.insert(
                        id,
                        Scene {
                            id,
                            title: document.title().into(),
                            bullets: document.body().into(),
                            event: metadata.scene_event,
                            characters: metadata.characters,
                            timing: metadata.scene_timing.timing,
                        },
                    );
                }
                DocumentKind::Chapter => {
                    let metadata: ChapterMetadata =
                        outline.parse_metadata(id, document.frontmatter());
                    chapters.push((
                        metadata.chapter_order,
                        document.path(),
                        Chapter {
                            id,
                            title: document.title().into(),
                            scenes: document.scenes().to_vec(),
                        },
                    ));
                }
                _ => {}
            }
        }
        chapters.sort_by_key(|(order, path, _)| (order.is_none(), *order, *path));
        outline.chapters = chapters
            .into_iter()
            .map(|(_, _, chapter)| chapter)
            .collect();
        outline
    }

    fn parse_metadata<T: for<'de> Deserialize<'de> + Default>(
        &mut self,
        document: Id,
        yaml: &str,
    ) -> T {
        match serde_saphyr::from_str(yaml) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.metadata_issues.push(OutlineIssue::InvalidMetadata {
                    document,
                    reason: error.to_string(),
                });
                T::default()
            }
        }
    }

    pub fn derive_scene(&mut self, event: Id, bullets: &str) -> Id {
        let id = Id::new_v4();
        let title = bullets
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(|line| {
                line.strip_prefix("- ")
                    .or_else(|| line.strip_prefix("* "))
                    .or_else(|| line.strip_prefix("+ "))
                    .unwrap_or(line)
            })
            .unwrap_or("Neue Szene");
        self.scenes.insert(
            id,
            Scene {
                id,
                title: title.into(),
                bullets: bullets.into(),
                event: Some(event),
                characters: Vec::new(),
                timing: SceneTiming::default(),
            },
        );
        id
    }

    pub fn update_scene(
        &mut self,
        id: Id,
        title: &str,
        bullets: &str,
        characters: Vec<Id>,
        timing: SceneTiming,
    ) -> Result<(), OutlineError> {
        let scene = self
            .scenes
            .get_mut(&id)
            .ok_or(OutlineError::MissingScene(id))?;
        scene.title = title.into();
        scene.bullets = bullets.into();
        scene.characters = characters;
        scene.timing = timing;
        Ok(())
    }

    pub fn scene(&self, id: Id) -> Option<&Scene> {
        self.scenes.get(&id)
    }

    pub fn unplaced_scenes(&self) -> impl Iterator<Item = &Scene> {
        let placed: BTreeSet<_> = self
            .chapters
            .iter()
            .flat_map(|chapter| chapter.scenes.iter())
            .copied()
            .collect();
        self.scenes
            .values()
            .filter(move |scene| !placed.contains(&scene.id))
    }

    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    pub fn chapter_of(&self, scene: Id) -> Option<Id> {
        self.chapters
            .iter()
            .find(|chapter| chapter.scenes.contains(&scene))
            .map(|chapter| chapter.id)
    }

    pub fn create_chapter(&mut self, title: &str) -> Id {
        let id = Id::new_v4();
        self.chapters.push(Chapter {
            id,
            title: title.into(),
            scenes: Vec::new(),
        });
        id
    }

    pub fn rename_chapter(&mut self, chapter: Id, title: &str) -> Result<(), OutlineError> {
        let index = self.chapter_index(chapter)?;
        self.chapters[index].title = title.into();
        Ok(())
    }

    /// Moves a chapter to a zero-based position in the resulting chapter list.
    pub fn move_chapter(&mut self, chapter: Id, index: usize) -> Result<(), OutlineError> {
        let from = self.chapter_index(chapter)?;
        validate_position(index, self.chapters.len() - 1)?;
        let chapter = self.chapters.remove(from);
        self.chapters.insert(index, chapter);
        Ok(())
    }

    /// Moves a chapter to the current position of the target chapter.
    pub fn move_chapter_to(&mut self, chapter: Id, target: Id) -> Result<(), OutlineError> {
        self.move_chapter(chapter, self.chapter_index(target)?)
    }

    /// Places a scene at a zero-based position after removing its previous placement.
    pub fn place(&mut self, scene: Id, chapter: Id, index: usize) -> Result<(), OutlineError> {
        self.require_scene(scene)?;
        let target = self.chapter_index(chapter)?;
        let available = self.chapters[target]
            .scenes
            .iter()
            .filter(|&&id| id != scene)
            .count();
        validate_position(index, available)?;
        self.remove_placements(scene);
        self.chapters[target].scenes.insert(index, scene);
        Ok(())
    }

    /// Appends a scene after its previous placement has been removed.
    pub fn append_scene(&mut self, scene: Id, chapter: Id) -> Result<(), OutlineError> {
        let index = self.chapter_index(chapter)?;
        let end = self.chapters[index]
            .scenes
            .iter()
            .filter(|&&id| id != scene)
            .count();
        self.place(scene, chapter, end)
    }

    /// Places a scene at the current position of another scene in the destination chapter.
    pub fn place_at_scene(
        &mut self,
        scene: Id,
        chapter: Id,
        target: Id,
    ) -> Result<(), OutlineError> {
        let index = self.chapter_index(chapter)?;
        let position = self.chapters[index]
            .scenes
            .iter()
            .position(|&id| id == target)
            .ok_or(OutlineError::MissingScene(target))?;
        self.place(scene, chapter, position)
    }

    pub fn remove_from_chapter(&mut self, scene: Id) -> Result<bool, OutlineError> {
        self.require_scene(scene)?;
        Ok(self.remove_placements(scene))
    }

    pub fn issues(&self) -> Vec<OutlineIssue> {
        let mut issues = self.metadata_issues.clone();
        let mut counts = BTreeMap::<_, usize>::new();
        for &scene in self.chapters.iter().flat_map(|chapter| &chapter.scenes) {
            *counts.entry(scene).or_default() += 1;
        }
        issues.extend(
            counts
                .into_iter()
                .filter(|&(_, count)| count > 1)
                .map(|(scene, _)| OutlineIssue::RepeatedScene { scene }),
        );
        issues
    }

    pub fn stage_changes(&self, project: &Project, store: &mut Store) -> crate::Result<()> {
        let previous = Self::from_project(project);
        let order_changed = !self
            .chapters
            .iter()
            .map(|chapter| chapter.id)
            .eq(previous.chapters.iter().map(|chapter| chapter.id));
        let previous_chapters: BTreeMap<_, _> = previous
            .chapters
            .iter()
            .map(|chapter| (chapter.id, chapter))
            .collect();
        store.stage_batch(|store| {
            for scene in self.scenes.values() {
                let prior = previous.scene(scene.id);
                if prior != Some(scene) {
                    stage_scene(store, project.document(scene.id), prior, scene)?;
                }
            }
            for (index, chapter) in self.chapters.iter().enumerate() {
                let prior = previous_chapters.get(&chapter.id).copied();
                if order_changed || prior != Some(chapter) {
                    stage_chapter(
                        store,
                        project.document(chapter.id),
                        chapter,
                        order_changed.then_some(index),
                    )?;
                }
            }
            Ok(())
        })
    }

    fn chapter_index(&self, chapter: Id) -> Result<usize, OutlineError> {
        self.chapters
            .iter()
            .position(|candidate| candidate.id == chapter)
            .ok_or(OutlineError::MissingChapter(chapter))
    }

    fn require_scene(&self, scene: Id) -> Result<(), OutlineError> {
        if self.scenes.contains_key(&scene) {
            Ok(())
        } else {
            Err(OutlineError::MissingScene(scene))
        }
    }

    fn remove_placements(&mut self, scene: Id) -> bool {
        let mut removed = false;
        for chapter in &mut self.chapters {
            let previous_len = chapter.scenes.len();
            chapter.scenes.retain(|&id| id != scene);
            removed |= chapter.scenes.len() != previous_len;
        }
        removed
    }
}

fn validate_position(index: usize, len: usize) -> Result<(), OutlineError> {
    if index > len {
        Err(OutlineError::InvalidPosition { index, len })
    } else {
        Ok(())
    }
}

fn stage_scene(
    store: &mut Store,
    document: Option<&Document>,
    previous: Option<&Scene>,
    scene: &Scene,
) -> crate::Result<()> {
    let Some(document) = document else {
        return store.stage_document(
            &Path::new("Manuskript").join(format!("{}.md", scene.id)),
            None,
            SceneFields {
                id: Some(scene.id),
                kind: Some("scene"),
                title: Some(&scene.title),
                scene_event: scene.event.map(Some),
                scene_timing: (scene.timing != SceneTiming::default()).then(|| StoredSceneTiming {
                    timing: scene.timing.clone(),
                    ..StoredSceneTiming::default()
                }),
                characters: (!scene.characters.is_empty()).then_some(scene.characters.as_slice()),
            },
            &scene.bullets,
        );
    };
    let previous =
        previous.ok_or_else(|| document_error(document, "Eine Szenendatei wird erwartet"))?;
    let path = store.relative_path(document)?;
    let title = (scene.title != previous.title).then_some(scene.title.as_str());
    let event = (scene.event != previous.event).then_some(scene.event);
    let timing_changed = scene.timing != previous.timing;
    let characters =
        (scene.characters != previous.characters).then_some(scene.characters.as_slice());
    if title.is_none() && event.is_none() && !timing_changed && characters.is_none() {
        return store.stage_body(&path, document, &scene.bullets);
    }
    let scene_timing = if event.is_some() || timing_changed || characters.is_some() {
        let mut metadata: SceneMetadata = serde_saphyr::from_str(document.frontmatter())
            .map_err(|error| document_error(document, error))?;
        if timing_changed {
            metadata.scene_timing.timing.clone_from(&scene.timing);
            Some(metadata.scene_timing)
        } else {
            None
        }
    } else {
        None
    };
    store.stage_document(
        &path,
        Some(document),
        SceneFields {
            title,
            scene_event: event,
            scene_timing,
            characters,
            ..SceneFields::default()
        },
        &scene.bullets,
    )
}

fn stage_chapter(
    store: &mut Store,
    document: Option<&Document>,
    chapter: &Chapter,
    order: Option<usize>,
) -> crate::Result<()> {
    let Some(document) = document else {
        return store.stage_document(
            &Path::new("Manuskript").join(format!("{}.md", chapter.id)),
            None,
            ChapterFields {
                id: Some(chapter.id),
                kind: Some("chapter"),
                title: Some(&chapter.title),
                scenes: Some(&chapter.scenes),
                chapter_order: order,
            },
            "",
        );
    };
    if document.kind() != DocumentKind::Chapter {
        return Err(document_error(document, "Eine Kapiteldatei wird erwartet"));
    }
    let title = (document.title() != chapter.title).then_some(chapter.title.as_str());
    let scenes = (document.scenes() != chapter.scenes).then_some(chapter.scenes.as_slice());
    let chapter_order = if let Some(index) = order {
        let metadata: ChapterMetadata = serde_saphyr::from_str(document.frontmatter())
            .map_err(|error| document_error(document, error))?;
        (metadata.chapter_order != Some(index)).then_some(index)
    } else {
        None
    };
    if title.is_none() && scenes.is_none() && chapter_order.is_none() {
        return Ok(());
    }
    let path = store.relative_path(document)?;
    store.stage_document(
        &path,
        Some(document),
        ChapterFields {
            title,
            scenes,
            chapter_order,
            ..ChapterFields::default()
        },
        document.body(),
    )
}

fn document_error(document: &Document, reason: impl ToString) -> ProjectError {
    ProjectError::Storage {
        path: document.path().to_owned(),
        reason: reason.to_string(),
    }
}
