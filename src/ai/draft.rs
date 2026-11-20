use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    DocumentKind, Id, Project, Result as ProjectResult,
    passage::{Anchor, SceneText, TextError},
    storage::Store,
};

pub struct DraftTurn {
    pub draft: Draft,
    pub prompt: String,
    sequence: u64,
}

#[derive(Deserialize)]
struct TurnMetadata {
    draft_target: Anchor,
    draft_prompt: String,
    draft_sequence: u64,
}

#[derive(Serialize)]
struct NewTurn<'a> {
    id: Id,
    kind: &'static str,
    title: &'a str,
    references: [Id; 1],
    draft_target: &'a Anchor,
    draft_prompt: &'a str,
    draft_sequence: u64,
}

pub struct Draft {
    target: Anchor,
    text: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DraftError {
    #[error("Wähle einen unveränderten Ausschnitt des Entwurfs.")]
    UnknownExcerpt,
    #[error(transparent)]
    Text(#[from] TextError),
}

impl Draft {
    pub fn new(target: Anchor, text: String) -> Self {
        Self { target, text }
    }
    pub fn target(&self) -> &Anchor {
        &self.target
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn history(project: &Project, scene: Id) -> Vec<DraftTurn> {
        let mut history: Vec<_> = project
            .documents()
            .filter(|document| document.kind() == DocumentKind::DraftConversation)
            .filter_map(|document| {
                let turn: TurnMetadata = serde_saphyr::from_str(document.frontmatter()).ok()?;
                (turn.draft_target.document_id() == scene).then(|| DraftTurn {
                    draft: Self::new(turn.draft_target, document.body().into()),
                    prompt: turn.draft_prompt,
                    sequence: turn.draft_sequence,
                })
            })
            .collect();
        history.sort_by_key(|turn| turn.sequence);
        history
    }

    pub fn stage(&self, project: &Project, store: &mut Store, prompt: &str) -> ProjectResult<()> {
        let scene = self.target.document_id();
        let id = Id::new_v4();
        let path = PathBuf::from("Brainstorming").join(format!("Entwurf-{id}.md"));
        let sequence = Self::history(project, scene)
            .last()
            .map_or(1, |turn| turn.sequence.saturating_add(1));
        store.stage_document(
            &path,
            None,
            NewTurn {
                id,
                kind: "draft_conversation",
                title: project
                    .scene(scene)
                    .map_or("Szenenentwurf", |document| document.title()),
                references: [scene],
                draft_target: &self.target,
                draft_prompt: prompt,
                draft_sequence: sequence,
            },
            &self.text,
        )
    }

    pub fn accept(&self, excerpt: &str, scene: &mut SceneText) -> Result<(), DraftError> {
        if excerpt.trim().is_empty() || !self.text.contains(excerpt) {
            return Err(DraftError::UnknownExcerpt);
        }
        scene.replace(&self.target, excerpt)?;
        Ok(())
    }
}
