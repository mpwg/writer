use std::{collections::BTreeMap, ops::Range};

use pulldown_cmark::{Event, Parser, Tag};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    DocumentKind, Id, Project,
    ai::brainstorm::Brainstorm,
    knowledge::{Claim, Knowledge},
    outline::{Outline, SceneTiming},
    passage::{Anchor, SceneText, TextError},
    timeline::Timeline,
};

const MAX_SOURCE_BYTES: usize = 2200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceForm {
    Bullet,
    Prose,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub document: Id,
    pub title: String,
    pub kind: DocumentKind,
    pub anchor: Anchor,
    pub event: Option<Id>,
    pub timing: Option<SceneTiming>,
    pub claim: Option<Claim>,
    pub concept: bool,
    pub form: SourceForm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Unique { document: Id, range: Range<usize> },
    Ambiguous,
    Missing,
}

pub struct Sources {
    documents: BTreeMap<Id, SceneText>,
    entries: Vec<Source>,
}

impl Sources {
    pub fn build(project: &Project) -> Self {
        let outline = Outline::from_project(project);
        let timeline = Timeline::from_project(project);
        let knowledge = Knowledge::from_project(project);
        let mut sources = Self {
            documents: BTreeMap::new(),
            entries: Vec::new(),
        };
        for file in project.documents() {
            if file.kind() == DocumentKind::ConsistencyDecision {
                continue;
            }
            let id = file.id();
            let scene = outline.scene(id);
            let event = timeline.event(id);
            let claim = knowledge.entry(id).and_then(|entry| entry.claim);
            let origin = scene
                .and_then(|scene| scene.event)
                .or_else(|| event.map(|event| event.id))
                .or_else(|| claim.and_then(|claim| claim.from));
            let timing = scene.map(|scene| scene.timing.clone()).or_else(|| {
                event.map(|event| SceneTiming {
                    time: event.time.clone(),
                    ..SceneTiming::default()
                })
            });
            let (text, blocks) = if file.kind() == DocumentKind::Brainstorm {
                let blocks = Brainstorm::marked_idea_ranges(file)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|source| {
                        !project
                            .document(source.idea)
                            .is_some_and(|document| document.kind() == DocumentKind::Event)
                    })
                    .map(|source| (source.range, SourceForm::Prose))
                    .collect();
                (SceneText::new(id, file.body()), blocks)
            } else {
                let text = SceneText::from_markdown(id, file.body());
                let blocks = block_ranges(text.text());
                (text, blocks)
            };
            for (block, form) in blocks {
                for (range, concept) in split_concepts(&text, block) {
                    let Some(range) = trim_range(text.text(), range) else {
                        continue;
                    };
                    for range in chunk_ranges(text.text(), range) {
                        if let Ok(anchor) = text.anchor(range) {
                            sources.entries.push(Source {
                                document: id,
                                title: file.title().into(),
                                kind: file.kind(),
                                anchor,
                                event: origin,
                                timing: timing.clone(),
                                claim,
                                concept,
                                form,
                            });
                        }
                    }
                }
            }
            sources.documents.insert(id, text);
        }
        sources
    }

    pub fn entries(&self) -> &[Source] {
        &self.entries
    }

    pub fn resolve(&self, anchor: &Anchor) -> Resolution {
        let document = anchor.document_id();
        let Some(text) = self.documents.get(&document) else {
            return Resolution::Missing;
        };
        match text.resolve(anchor) {
            Ok(range) => Resolution::Unique { document, range },
            Err(TextError::AmbiguousAnchor) => Resolution::Ambiguous,
            Err(_) => Resolution::Missing,
        }
    }
}

struct Block {
    depth: usize,
    range: Range<usize>,
    form: SourceForm,
}

fn block_ranges(text: &str) -> Vec<(Range<usize>, SourceForm)> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut ranges = Vec::new();
    let mut depth = 0;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                depth += 1;
                let form = match tag {
                    Tag::Item => SourceForm::Bullet,
                    Tag::Paragraph | Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::HtmlBlock
                        if blocks.is_empty() =>
                    {
                        SourceForm::Prose
                    }
                    _ => continue,
                };
                if let Some(parent) = blocks.last_mut()
                    && parent.range.start < range.start
                {
                    ranges.push((parent.range.start..range.start, parent.form));
                    parent.range.start = range.start;
                }
                blocks.push(Block { depth, range, form });
            }
            Event::End(_) => {
                if blocks.last().is_some_and(|block| block.depth == depth)
                    && let Some(block) = blocks.pop()
                {
                    if let Some(parent) = blocks.last_mut() {
                        parent.range.start = block.range.end;
                    }
                    ranges.push((block.range, block.form));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    ranges
}

fn split_concepts(text: &SceneText, range: Range<usize>) -> Vec<(Range<usize>, bool)> {
    let concepts = text.concepts();
    let first = concepts.partition_point(|concept| concept.range.end <= range.start);
    let mut blocks = Vec::new();
    let mut cursor = range.start;
    for concept in &concepts[first..] {
        if concept.range.start >= range.end {
            break;
        }
        if concept.range.is_empty() {
            continue;
        }
        let start = concept.range.start.max(cursor);
        if cursor < start {
            blocks.push((cursor..start, false));
        }
        cursor = concept.range.end.min(range.end);
        blocks.push((start..cursor, true));
    }
    if cursor < range.end {
        blocks.push((cursor..range.end, false));
    }
    blocks
}

fn trim_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let original = text.get(range.clone())?;
    let trimmed = original.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = range.start + original.len() - original.trim_start().len();
    Some(start..start + trimmed.len())
}

fn chunk_ranges(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut chunks = Vec::new();
    let mut start = range.start;
    for (offset, grapheme) in text[range.clone()].grapheme_indices(true) {
        let position = range.start + offset;
        if position > start && position - start + grapheme.len() > MAX_SOURCE_BYTES {
            chunks.push(start..position);
            start = position;
        }
    }
    chunks.push(start..range.end);
    chunks
}
