use std::{
    fmt::Write,
    hash::{Hash, Hasher},
    ops::Range,
};

use unicode_segmentation::UnicodeSegmentation;

use crate::Id;

const CONCEPT_OPEN: &str = "<!-- writer:concept ";
const CONCEPT_CLOSE: &str = "<!-- /writer:concept ";
const MARKER_END: &str = " -->";
const CONTEXT_CHARS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    #[error("Die Textauswahl liegt außerhalb der Szene oder trennt ein Schriftzeichen.")]
    InvalidRange,
    #[error("Die Textauswahl gehört zu einer anderen Szene.")]
    WrongDocument,
    #[error("Die Textstelle wurde geändert. Wähle sie erneut aus.")]
    MissingAnchor,
    #[error("Die Textstelle kommt mehrfach vor. Wähle sie erneut aus.")]
    AmbiguousAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Anchor {
    document_id: Id,
    revision: u64,
    range: Range<usize>,
    quote: String,
    before: String,
    after: String,
}

impl Anchor {
    pub fn document_id(&self) -> Id {
        self.document_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn quote(&self) -> &str {
        &self.quote
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concept {
    pub id: Id,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    Bold,
    Italic,
    Heading,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    text: String,
    revision: u64,
    concepts: Vec<Concept>,
}

#[derive(Debug)]
pub struct SceneText {
    document_id: Id,
    state: Snapshot,
    history: Vec<Snapshot>,
}

impl SceneText {
    pub fn new(document_id: Id, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            document_id,
            state: Snapshot {
                revision: text_revision(&text),
                text,
                concepts: Vec::new(),
            },
            history: Vec::new(),
        }
    }

    pub fn from_markdown(document_id: Id, markdown: &str) -> Self {
        let mut scene = Self::new(document_id, String::with_capacity(markdown.len()));
        let mut rest = markdown;
        while let Some((before, tail)) = rest.split_once(CONCEPT_OPEN) {
            scene.state.text.push_str(before);
            let opening_id = tail
                .split_once(MARKER_END)
                .and_then(|(id, body)| Some((id.parse::<Id>().ok()?, body)));
            if let Some((id, body)) = opening_id {
                let closing = format!("{CONCEPT_CLOSE}{id}{MARKER_END}");
                if let Some((concept, after)) = body.split_once(&closing) {
                    let start = scene.state.text.len();
                    scene.state.text.push_str(concept);
                    scene.state.concepts.push(Concept {
                        id,
                        range: start..scene.state.text.len(),
                    });
                    rest = after;
                    continue;
                }
            }
            scene.state.text.push_str(CONCEPT_OPEN);
            rest = tail;
        }
        scene.state.text.push_str(rest);
        scene.state.revision = text_revision(scene.text());
        scene
    }

    pub fn text(&self) -> &str {
        &self.state.text
    }

    pub fn concepts(&self) -> &[Concept] {
        &self.state.concepts
    }

    pub fn markdown(&self) -> String {
        let mut output = String::new();
        let mut end = 0;
        for concept in self.concepts() {
            output.push_str(&self.text()[end..concept.range.start]);
            let id = concept.id;
            let _ = write!(output, "{CONCEPT_OPEN}{id}{MARKER_END}");
            output.push_str(&self.text()[concept.range.clone()]);
            let _ = write!(output, "{CONCEPT_CLOSE}{id}{MARKER_END}");
            end = concept.range.end;
        }
        output.push_str(&self.text()[end..]);
        output
    }

    pub fn anchor(&self, range: Range<usize>) -> Result<Anchor, TextError> {
        let text = self.text();
        let quote = text.get(range.clone()).ok_or(TextError::InvalidRange)?;
        if !grapheme_boundary(text, range.start) || !grapheme_boundary(text, range.end) {
            return Err(TextError::InvalidRange);
        }
        let mut before: Vec<char> = text[..range.start]
            .chars()
            .rev()
            .take(CONTEXT_CHARS)
            .collect();
        before.reverse();
        Ok(Anchor {
            document_id: self.document_id,
            revision: self.state.revision,
            quote: quote.into(),
            before: before.into_iter().collect(),
            after: text[range.end..].chars().take(CONTEXT_CHARS).collect(),
            range,
        })
    }

    pub fn resolve(&self, target: &Anchor) -> Result<Range<usize>, TextError> {
        if target.document_id != self.document_id {
            return Err(TextError::WrongDocument);
        }
        if target.revision == self.state.revision {
            return Ok(target.range.clone());
        }
        let text = self.text();
        let mut candidates = self.matching_ranges(&target.quote);
        let first = candidates.next().ok_or(TextError::MissingAnchor)?;
        let Some(second) = candidates.next() else {
            return if target.quote.is_empty() && !matches_context(text, target, &first) {
                Err(TextError::MissingAnchor)
            } else {
                Ok(first)
            };
        };
        let mut contextual = [first, second]
            .into_iter()
            .chain(candidates)
            .filter(|range| matches_context(text, target, range));
        match (contextual.next(), contextual.next()) {
            (Some(selected), None) => Ok(selected),
            _ => Err(TextError::AmbiguousAnchor),
        }
    }

    pub(crate) fn matching_ranges<'a>(
        &'a self,
        quote: &'a str,
    ) -> impl Iterator<Item = Range<usize>> + 'a {
        let text = self.text();
        text.grapheme_indices(true)
            .map(|(start, _)| start)
            .chain(std::iter::once(text.len()))
            .filter_map(move |start| {
                text[start..].strip_prefix(quote)?;
                let end = start + quote.len();
                grapheme_boundary(text, end).then_some(start..end)
            })
    }

    pub fn replace(&mut self, target: &Anchor, markdown: &str) -> Result<(), TextError> {
        let range = self.resolve(target)?;
        self.commit(self.state.replaced(range, markdown, false));
        Ok(())
    }

    pub fn edit(&mut self, markdown: &str) -> Result<(), TextError> {
        let text = self.text();
        let prefix = text
            .graphemes(true)
            .zip(markdown.graphemes(true))
            .take_while(|(previous, next)| previous == next)
            .map(|(grapheme, _)| grapheme.len())
            .sum::<usize>();
        let suffix = text[prefix..]
            .graphemes(true)
            .rev()
            .zip(markdown[prefix..].graphemes(true).rev())
            .take_while(|(previous, next)| previous == next)
            .map(|(grapheme, _)| grapheme.len())
            .sum::<usize>();
        let range = prefix..text.len() - suffix;
        let replacement = &markdown[prefix..markdown.len() - suffix];
        self.commit(self.state.replaced(range, replacement, false));
        Ok(())
    }

    pub fn set_concept(&mut self, target: &Anchor, enabled: bool) -> Result<(), TextError> {
        let range = self.resolve(target)?;
        let mut next = self.state.clone();
        if enabled {
            next.mark(range);
        } else {
            next.unmark(range);
        }
        self.commit(next);
        Ok(())
    }

    pub fn format(&mut self, target: &Anchor, format: TextFormat) -> Result<(), TextError> {
        let range = self.resolve(target)?;
        let next = match format {
            TextFormat::Bold | TextFormat::Italic => {
                let mark = match format {
                    TextFormat::Bold => "**",
                    _ => "*",
                };
                let mut next = self.state.replaced(range.end..range.end, mark, true);
                next = next.replaced(range.start..range.start, mark, false);
                next
            }
            TextFormat::Heading | TextFormat::List => {
                let text = self.text();
                let prefix = match format {
                    TextFormat::Heading => "# ",
                    _ => "- ",
                };
                let first = text[..range.start].rfind('\n').map_or(0, |last| last + 1);
                let mut offsets = vec![first];
                offsets.extend(
                    text[first..range.end]
                        .match_indices('\n')
                        .map(|(offset, _)| first + offset + 1)
                        .filter(|&offset| offset < range.end),
                );
                let mut next = self.state.clone();
                for offset in offsets.into_iter().rev() {
                    next = next.replaced(offset..offset, prefix, false);
                }
                next
            }
        };
        self.commit(next);
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.history.pop() else {
            return false;
        };
        self.state = previous;
        true
    }

    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.state
    }

    pub(crate) fn swap_snapshot(&mut self, snapshot: Snapshot) -> Snapshot {
        std::mem::replace(&mut self.state, snapshot)
    }

    pub(crate) fn take_previous(&mut self) -> Option<Snapshot> {
        self.history.pop()
    }

    fn commit(&mut self, next: Snapshot) {
        if self.state != next {
            self.history.push(std::mem::replace(&mut self.state, next));
        }
    }
}

impl Snapshot {
    pub(crate) fn replaced(
        &self,
        range: Range<usize>,
        replacement: &str,
        insert_left: bool,
    ) -> Self {
        if self.text[range.clone()] == *replacement {
            return self.clone();
        }
        let concept = self.concepts.iter().position(|concept| {
            if range.is_empty() {
                if insert_left {
                    concept.range.start < range.start && range.start <= concept.range.end
                } else {
                    concept.range.start <= range.start && range.start < concept.range.end
                        || concept.range.is_empty() && concept.range.start == range.start
                }
            } else {
                overlaps(&concept.range, &range)
            }
        });
        let replacement_end = range.start + replacement.len();
        let mut concepts = Vec::with_capacity(self.concepts.len());
        for (index, before) in self.concepts.iter().enumerate() {
            let empty_after_insertion = range.is_empty()
                && before.range.is_empty()
                && before.range.start == range.start
                && concept.is_some_and(|inherited| inherited < index);
            let mapped = if concept == Some(index) {
                before.range.start.min(range.start)
                    ..replacement_end + before.range.end.saturating_sub(range.end)
            } else if before.range.end <= range.start && !empty_after_insertion {
                before.range.clone()
            } else if before.range.start >= range.end {
                replacement_end + (before.range.start - range.end)
                    ..replacement_end + (before.range.end - range.end)
            } else if before.range.end > range.end {
                replacement_end..replacement_end + (before.range.end - range.end)
            } else {
                continue;
            };
            concepts.push(Concept {
                id: before.id,
                range: mapped,
            });
        }
        let mut text = self.text.clone();
        text.replace_range(range, replacement);
        Self {
            revision: text_revision(&text),
            text,
            concepts,
        }
    }

    fn mark(&mut self, mut range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let mut id = None;
        self.concepts.retain(|concept| {
            if overlaps(&concept.range, &range) {
                range.start = range.start.min(concept.range.start);
                range.end = range.end.max(concept.range.end);
                id.get_or_insert(concept.id);
                false
            } else {
                true
            }
        });
        let index = self
            .concepts
            .partition_point(|concept| concept.range.start < range.start);
        self.concepts.insert(
            index,
            Concept {
                id: id.unwrap_or_else(Id::new_v4),
                range,
            },
        );
    }

    fn unmark(&mut self, range: Range<usize>) {
        let mut concepts = Vec::with_capacity(self.concepts.len());
        for concept in self.concepts.drain(..) {
            if !overlaps(&concept.range, &range) {
                concepts.push(concept);
                continue;
            }
            if concept.range.start < range.start {
                concepts.push(Concept {
                    id: concept.id,
                    range: concept.range.start..range.start,
                });
            }
            if range.end < concept.range.end {
                concepts.push(Concept {
                    id: if concept.range.start < range.start {
                        Id::new_v4()
                    } else {
                        concept.id
                    },
                    range: range.end..concept.range.end,
                });
            }
        }
        self.concepts = concepts;
    }
}

fn overlaps(first: &Range<usize>, second: &Range<usize>) -> bool {
    if first.is_empty() {
        second.start <= first.start && first.start <= second.end
    } else {
        !second.is_empty() && first.start < second.end && second.start < first.end
    }
}

fn grapheme_boundary(text: &str, index: usize) -> bool {
    text.is_char_boundary(index)
        && unicode_segmentation::GraphemeCursor::new(index, text.len(), true)
            .is_boundary(text, 0)
            .unwrap_or(false)
}

fn text_revision(text: &str) -> u64 {
    let mut hash = std::hash::DefaultHasher::new();
    text.hash(&mut hash);
    hash.finish()
}

fn matches_context(text: &str, target: &Anchor, range: &Range<usize>) -> bool {
    if target.quote.is_empty() && target.before.is_empty() && target.after.is_empty() {
        return false;
    }
    text[..range.start].ends_with(&target.before) && text[range.end..].starts_with(&target.after)
}
