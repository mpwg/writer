use serde::{Deserialize, Serialize};

use crate::Id;

pub(crate) const VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
pub(crate) struct Manifest {
    pub title: String,
    pub format_version: u32,
    #[serde(default)]
    pub id: Option<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    Scene,
    Chapter,
    Event,
    Character,
    Place,
    Concept,
    Idea,
    Brainstorm,
    DraftConversation,
    ConsistencyDecision,
}

#[derive(Clone, Deserialize)]
pub(crate) struct Frontmatter {
    pub id: Id,
    pub kind: DocumentKind,
    pub title: Option<String>,
    #[serde(default)]
    pub references: Vec<Id>,
    #[serde(default)]
    pub scenes: Vec<Id>,
}

pub(crate) fn split_document(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    let opening = lines.next()?;
    if opening.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let content = &text[opening.len()..];
    let mut offset = 0;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some((&content[..offset], &content[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}
