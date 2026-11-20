use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Id, Project,
    outline::Outline,
    passage::{Anchor, SceneText, Snapshot, TextError, TextFormat},
};

#[derive(Debug)]
pub struct ManuscriptChapter {
    pub id: Option<Id>,
    pub title: String,
    pub scenes: Vec<Id>,
}

#[derive(Debug)]
pub struct ManuscriptScene {
    pub id: Id,
    pub title: String,
    pub text: SceneText,
    saved: Snapshot,
}

#[derive(Debug)]
pub struct Manuscript {
    title: String,
    chapters: Vec<ManuscriptChapter>,
    scenes: BTreeMap<Id, ManuscriptScene>,
    scene_order: Vec<Id>,
    undo: Vec<Vec<(Id, Snapshot)>>,
}

impl Manuscript {
    pub fn from_project(project: &Project) -> Self {
        let outline = Outline::from_project(project);
        let scenes: BTreeMap<_, _> = project
            .scenes()
            .map(|document| {
                let id = document.id();
                let text = SceneText::from_markdown(id, document.body());
                (
                    id,
                    ManuscriptScene {
                        id,
                        title: document.title().into(),
                        saved: text.snapshot().clone(),
                        text,
                    },
                )
            })
            .collect();
        let mut placed = BTreeSet::new();
        let mut chapters: Vec<_> = outline
            .chapters()
            .iter()
            .map(|chapter| ManuscriptChapter {
                id: Some(chapter.id),
                title: chapter.title.clone(),
                scenes: chapter
                    .scenes
                    .iter()
                    .copied()
                    .filter(|id| scenes.contains_key(id) && placed.insert(*id))
                    .collect(),
            })
            .collect();
        let unplaced: Vec<_> = outline.unplaced_scenes().map(|scene| scene.id).collect();
        if !unplaced.is_empty() {
            chapters.push(ManuscriptChapter {
                id: None,
                title: "Ohne Kapitel".into(),
                scenes: unplaced,
            });
        }
        let scene_order = chapters
            .iter()
            .flat_map(|chapter| &chapter.scenes)
            .copied()
            .collect();
        Self {
            title: project.title().into(),
            chapters,
            scenes,
            scene_order,
            undo: Vec::new(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn chapters(&self) -> &[ManuscriptChapter] {
        &self.chapters
    }

    pub fn scene(&self, id: Id) -> Option<&ManuscriptScene> {
        self.scenes.get(&id)
    }

    pub fn changed_scene_ids(&self) -> impl Iterator<Item = Id> + '_ {
        self.scene_order.iter().copied().filter(|id| {
            let scene = &self.scenes[id];
            scene.text.snapshot() != &scene.saved
        })
    }

    pub fn acknowledge_saved(&mut self, id: Id) {
        if let Some(scene) = self.scenes.get_mut(&id) {
            scene.saved = scene.text.snapshot().clone();
        }
    }

    pub fn replace(
        &mut self,
        start: &Anchor,
        end: &Anchor,
        markdown: &str,
    ) -> Result<(), TextError> {
        let find = |id| {
            self.scene_order
                .iter()
                .position(|&scene| scene == id)
                .ok_or(TextError::WrongDocument)
        };
        let first = find(start.document_id())?;
        let last = find(end.document_id())?;
        let start = self.scenes[&self.scene_order[first]]
            .text
            .resolve(start)?
            .start;
        let end = self.scenes[&self.scene_order[last]].text.resolve(end)?.end;
        if first > last || (first == last && start > end) {
            return Err(TextError::InvalidRange);
        }
        let mut changed = Vec::new();
        for index in first..=last {
            let id = self.scene_order[index];
            let Some(scene) = self.scenes.get_mut(&id) else {
                continue;
            };
            let from = if index == first { start } else { 0 };
            let to = if index == last {
                end
            } else {
                scene.text.text().len()
            };
            let insert = if index == first { markdown } else { "" };
            let next = scene.text.snapshot().replaced(from..to, insert, false);
            if &next != scene.text.snapshot() {
                changed.push((id, scene.text.swap_snapshot(next)));
            }
        }
        if !changed.is_empty() {
            self.undo.push(changed);
        }
        Ok(())
    }

    pub fn edit_scene(&mut self, id: Id, markdown: &str) -> Result<(), TextError> {
        self.scene_edit(id, |scene| scene.edit(markdown))
    }

    pub fn set_concept(&mut self, target: &Anchor, enabled: bool) -> Result<(), TextError> {
        self.scene_edit(target.document_id(), |scene| {
            scene.set_concept(target, enabled)
        })
    }

    pub fn format_scene(&mut self, target: &Anchor, format: TextFormat) -> Result<(), TextError> {
        self.scene_edit(target.document_id(), |scene| scene.format(target, format))
    }

    pub fn undo(&mut self) -> bool {
        let Some(changes) = self.undo.pop() else {
            return false;
        };
        for (id, before) in changes {
            if let Some(scene) = self.scenes.get_mut(&id) {
                scene.text.swap_snapshot(before);
            }
        }
        true
    }

    fn scene_edit(
        &mut self,
        id: Id,
        edit: impl FnOnce(&mut SceneText) -> Result<(), TextError>,
    ) -> Result<(), TextError> {
        let scene = self.scenes.get_mut(&id).ok_or(TextError::WrongDocument)?;
        edit(&mut scene.text)?;
        if let Some(previous) = scene.text.take_previous() {
            self.undo.push(vec![(id, previous)]);
        }
        Ok(())
    }
}
