use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    Document, DocumentKind, Id, Project, ProjectError,
    storage::Store,
    timeline::{TemporalOrder, Timeline},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeKind {
    Character,
    Place,
    Concept,
    Idea,
}

impl KnowledgeKind {
    fn from_document(kind: DocumentKind) -> Option<Self> {
        match kind {
            DocumentKind::Character => Some(Self::Character),
            DocumentKind::Place => Some(Self::Place),
            DocumentKind::Concept => Some(Self::Concept),
            DocumentKind::Idea => Some(Self::Idea),
            _ => None,
        }
    }

    fn metadata(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Place => "place",
            Self::Concept => "concept",
            Self::Idea => "idea",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum ClaimHolder {
    Author,
    Character(Id),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    Knows,
    Believes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct Claim {
    pub holder: ClaimHolder,
    pub state: ClaimState,
    pub from: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeEntry {
    pub id: Id,
    pub kind: KnowledgeKind,
    pub title: String,
    pub body: String,
    pub claim: Option<Claim>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KnowledgeError {
    #[error("Der Karteieintrag {0} fehlt.")]
    MissingEntry(Id),
    #[error("Die Wissensangaben des Karteieintrags {id} sind ungültig: {reason}")]
    InvalidClaim { id: Id, reason: String },
}

#[derive(Default)]
pub struct Knowledge {
    entries: BTreeMap<Id, KnowledgeEntry>,
    issues: Vec<KnowledgeError>,
}

#[derive(Default, Deserialize)]
struct Metadata {
    knowledge_claim: Option<StoredClaim>,
}

#[derive(Deserialize, Serialize)]
struct StoredClaim {
    holder: ClaimHolder,
    state: ClaimState,
    from: Option<Id>,
    #[serde(flatten)]
    other: BTreeMap<String, serde_json::Value>,
}

impl StoredClaim {
    fn with_claim(claim: Claim, other: BTreeMap<String, serde_json::Value>) -> Self {
        Self {
            holder: claim.holder,
            state: claim.state,
            from: claim.from,
            other,
        }
    }
}

impl From<StoredClaim> for Claim {
    fn from(stored: StoredClaim) -> Self {
        Self {
            holder: stored.holder,
            state: stored.state,
            from: stored.from,
        }
    }
}

#[derive(Default, Serialize)]
struct Patch<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    knowledge_claim: Option<Option<StoredClaim>>,
}

impl Knowledge {
    pub fn from_project(project: &Project) -> Self {
        let mut knowledge = Self::default();
        for file in project.documents() {
            let Some(kind) = KnowledgeKind::from_document(file.kind()) else {
                continue;
            };
            let id = file.id();
            let claim = match serde_saphyr::from_str::<Metadata>(file.frontmatter()) {
                Ok(metadata) => metadata.knowledge_claim.map(Claim::from),
                Err(error) => {
                    knowledge.issues.push(KnowledgeError::InvalidClaim {
                        id,
                        reason: error.to_string(),
                    });
                    None
                }
            };
            knowledge.entries.insert(
                id,
                KnowledgeEntry {
                    id,
                    kind,
                    title: file.title().into(),
                    body: file.body().into(),
                    claim,
                },
            );
        }
        knowledge
    }

    pub fn entries(&self) -> impl Iterator<Item = &KnowledgeEntry> {
        self.entries.values()
    }

    pub fn entry(&self, id: Id) -> Option<&KnowledgeEntry> {
        self.entries.get(&id)
    }

    pub fn issues(&self) -> &[KnowledgeError] {
        &self.issues
    }

    pub fn create(
        &mut self,
        kind: KnowledgeKind,
        title: &str,
        body: &str,
        claim: Option<Claim>,
    ) -> Id {
        let id = Id::new_v4();
        self.entries.insert(
            id,
            KnowledgeEntry {
                id,
                kind,
                title: title.into(),
                body: body.into(),
                claim,
            },
        );
        id
    }

    pub fn update(
        &mut self,
        id: Id,
        title: &str,
        body: &str,
        claim: Option<Claim>,
    ) -> Result<(), KnowledgeError> {
        let entry = self
            .entries
            .get_mut(&id)
            .ok_or(KnowledgeError::MissingEntry(id))?;
        entry.title = title.into();
        entry.body = body.into();
        entry.claim = claim;
        Ok(())
    }

    pub fn author_knowledge(&self) -> impl Iterator<Item = &KnowledgeEntry> {
        self.entries().filter(|entry| {
            entry
                .claim
                .is_some_and(|claim| claim.holder == ClaimHolder::Author)
        })
    }

    pub fn known_by<'a>(
        &'a self,
        character: Id,
        at: Id,
        timeline: &'a Timeline,
    ) -> impl Iterator<Item = &'a KnowledgeEntry> {
        self.entries().filter(move |entry| {
            entry.claim.is_some_and(|claim| {
                claim.holder == ClaimHolder::Character(character)
                    && claim.from.is_none_or(|from| {
                        matches!(
                            timeline.compare(from, at),
                            TemporalOrder::Before | TemporalOrder::Same
                        )
                    })
            })
        })
    }

    pub fn stage_changes(&self, project: &Project, store: &mut Store) -> crate::Result<()> {
        let previous = Self::from_project(project);
        store.stage_batch(|store| {
            for entry in self.entries() {
                let prior = previous.entry(entry.id);
                if prior != Some(entry) {
                    stage_entry(entry, prior, project.document(entry.id), store)?;
                }
            }
            Ok(())
        })
    }
}

fn stage_entry(
    entry: &KnowledgeEntry,
    previous: Option<&KnowledgeEntry>,
    file: Option<&Document>,
    store: &mut Store,
) -> crate::Result<()> {
    let Some(file) = file else {
        let folder = if entry.kind == KnowledgeKind::Idea {
            "Brainstorming"
        } else {
            "Welt"
        };
        let path = Path::new(folder).join(format!("{}.md", entry.id));
        return store.stage_document(
            &path,
            None,
            Patch {
                id: Some(entry.id),
                kind: Some(entry.kind.metadata()),
                title: Some(&entry.title),
                knowledge_claim: entry
                    .claim
                    .map(|claim| Some(StoredClaim::with_claim(claim, BTreeMap::new()))),
            },
            &entry.body,
        );
    };
    let path = store.relative_path(file)?;
    let previous = previous.ok_or_else(|| ProjectError::Storage {
        path: file.path().into(),
        reason: format!(
            "Kennung {} gehört zu einem Dokument ohne Karteieintrag",
            entry.id
        ),
    })?;
    let title = (entry.title != previous.title).then_some(entry.title.as_str());
    if title.is_none() && entry.claim == previous.claim {
        return store.stage_body(&path, file, &entry.body);
    }
    let knowledge_claim = if entry.claim != previous.claim {
        let claim = entry
            .claim
            .map(|claim| {
                let metadata: Metadata =
                    serde_saphyr::from_str(file.frontmatter()).map_err(|error| {
                        ProjectError::Storage {
                            path: file.path().into(),
                            reason: error.to_string(),
                        }
                    })?;
                let other = metadata
                    .knowledge_claim
                    .map_or_else(BTreeMap::new, |claim| claim.other);
                Ok(StoredClaim::with_claim(claim, other))
            })
            .transpose()?;
        Some(claim)
    } else {
        None
    };
    store.stage_document(
        &path,
        Some(file),
        Patch {
            title,
            knowledge_claim,
            ..Patch::default()
        },
        &entry.body,
    )
}
