use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    path::Path,
};

use pulldown_cmark::{Event, Parser, TagEnd};
use serde::{Deserialize, Serialize};

use crate::{
    DocumentKind, Id, Project, ProjectError, Result,
    passage::SceneText,
    storage::Store,
    timeline::{Relation, Timeline},
};

const DECISION_NAMESPACE: Id = Id::from_u128(0xa4e0f869_2a1c_4d71_9b94_5706d174952b);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Finding {
    pub title: String,
    pub sources: Vec<FindingSource>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FindingSource {
    pub document: Id,
    pub quote: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    Open,
    Dismissed,
    Uncertain,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ReviewedFinding {
    pub finding: Finding,
    pub status: ReviewStatus,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSection {
    Body,
    EventTime,
    RelationNote { to: Id, relation: Relation },
    RelationTime { to: Id, relation: Relation },
}

#[derive(Debug, Eq, PartialEq)]
pub struct SourceLocation {
    pub document: Id,
    pub section: SourceSection,
    /// Byte offsets in the editable source text, with concept markers removed from scenes.
    pub body_range: Option<Range<usize>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataSource {
    pub source: FindingSource,
    pub section: SourceSection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decision {
    pub id: Id,
    pub title: String,
    pub sources: Vec<FindingSource>,
    pub reason: Option<String>,
    signature: Vec<Statement>,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
#[error("Die Entscheidung in Dokument {document} konnte nicht gelesen werden: {reason}")]
pub struct ReviewIssue {
    pub document: Id,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct Statement {
    document: Id,
    section: SourceSection,
    paragraph: String,
    quote: String,
}

#[derive(Deserialize)]
struct DecisionMetadata {
    finding_sources: Vec<FindingSource>,
    statement_signature: Vec<Statement>,
}

#[derive(Serialize)]
struct DecisionFields<'a> {
    id: Id,
    kind: &'static str,
    title: &'a str,
    references: BTreeSet<Id>,
    finding_sources: &'a [FindingSource],
    statement_signature: &'a [Statement],
}

struct IndexedPassage {
    section: SourceSection,
    paragraph: String,
    body_range: Option<Range<usize>>,
}

pub struct Review<'a> {
    project: &'a Project,
    passages: BTreeMap<Id, Vec<IndexedPassage>>,
    metadata_sources: Vec<MetadataSource>,
    decisions: Vec<Decision>,
    issues: Vec<ReviewIssue>,
}

pub fn check(
    project: &Project,
    findings: impl IntoIterator<Item = Finding>,
) -> Vec<ReviewedFinding> {
    let review = Review::from_project(project);
    findings
        .into_iter()
        .map(|finding| ReviewedFinding {
            status: review.status(&finding),
            finding,
        })
        .collect()
}

impl<'a> Review<'a> {
    pub fn from_project(project: &'a Project) -> Self {
        let mut review = Self {
            project,
            passages: BTreeMap::new(),
            metadata_sources: Vec::new(),
            decisions: Vec::new(),
            issues: Vec::new(),
        };
        for document in project.documents() {
            match document.kind() {
                DocumentKind::ConsistencyDecision => match read_decision(document.frontmatter()) {
                    Ok(metadata) => review.decisions.push(Decision {
                        id: document.id(),
                        title: document.title().into(),
                        sources: metadata.finding_sources,
                        reason: (!document.body().trim().is_empty())
                            .then(|| document.body().to_owned()),
                        signature: metadata.statement_signature,
                    }),
                    Err(reason) => review.issues.push(ReviewIssue {
                        document: document.id(),
                        reason,
                    }),
                },
                DocumentKind::Brainstorm => {}
                DocumentKind::Scene => {
                    let scene = SceneText::from_markdown(document.id(), document.body());
                    review
                        .passages
                        .insert(document.id(), body_passages(scene.text()));
                }
                _ => {
                    review
                        .passages
                        .insert(document.id(), body_passages(document.body()));
                }
            }
        }
        let timeline = Timeline::from_project(project);
        for document in project
            .documents()
            .filter(|document| document.kind() == DocumentKind::Event)
        {
            let id = document.id();
            if let Some(time) = timeline.event(id).and_then(|event| event.time.as_deref()) {
                review.add_metadata(id, SourceSection::EventTime, time);
            }
        }
        for link in timeline.relations() {
            review.add_metadata(
                link.from,
                SourceSection::RelationNote {
                    to: link.to,
                    relation: link.kind,
                },
                &link.note,
            );
            if let Some(time) = &link.time {
                review.add_metadata(
                    link.from,
                    SourceSection::RelationTime {
                        to: link.to,
                        relation: link.kind,
                    },
                    time,
                );
            }
        }
        review
    }

    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    pub fn issues(&self) -> &[ReviewIssue] {
        &self.issues
    }

    pub fn metadata_sources(&self) -> &[MetadataSource] {
        &self.metadata_sources
    }

    fn add_metadata(&mut self, document: Id, section: SourceSection, text: &str) {
        let mut content = body_passages(text);
        for passage in &mut content {
            passage.section = section;
            if let Some(quote) = passage.body_range.take().and_then(|range| text.get(range)) {
                self.metadata_sources.push(MetadataSource {
                    source: FindingSource {
                        document,
                        quote: quote.into(),
                    },
                    section,
                });
            }
        }
        self.passages.entry(document).or_default().extend(content);
    }

    pub fn locate(&self, source: &FindingSource) -> Option<SourceLocation> {
        let quote = normalize_quote(&source.quote);
        let passage = self.unique_passage(source.document, &quote)?;
        Some(SourceLocation {
            document: source.document,
            section: passage.section,
            body_range: passage.body_range.clone(),
        })
    }

    pub fn status(&self, finding: &Finding) -> ReviewStatus {
        let Some(signature) = self.signature(finding) else {
            return ReviewStatus::Uncertain;
        };
        if self.find_decision(&signature).is_some() {
            ReviewStatus::Dismissed
        } else {
            ReviewStatus::Open
        }
    }

    /// Stages the confirmed decision. The caller flushes the project store and reloads its snapshot.
    pub fn dismiss(
        &self,
        finding: &Finding,
        reason: Option<String>,
        store: &mut Store,
    ) -> Result<Decision> {
        let error = |reason| ProjectError::Storage {
            path: self.project.root().to_owned(),
            reason,
        };
        let signature = self.signature(finding).ok_or_else(|| {
            error("Die Aussagen konnten keiner eindeutigen Fundstelle zugeordnet werden".into())
        })?;
        if let Some(decision) = self.find_decision(&signature) {
            return Ok(decision.clone());
        }

        let identity =
            serde_json::to_vec(&signature).map_err(|source| error(source.to_string()))?;
        let decision = Decision {
            id: Id::new_v5(&DECISION_NAMESPACE, &identity),
            title: finding.title.clone(),
            sources: finding.sources.clone(),
            reason: reason.filter(|reason| !reason.trim().is_empty()),
            signature,
        };
        let relative = Path::new("Welt").join(format!("Entscheidung-{}.md", decision.id));
        store.stage_document(
            &relative,
            None,
            DecisionFields {
                id: decision.id,
                kind: "consistency_decision",
                title: &decision.title,
                references: decision
                    .sources
                    .iter()
                    .map(|source| source.document)
                    .collect(),
                finding_sources: &decision.sources,
                statement_signature: &decision.signature,
            },
            decision.reason.as_deref().unwrap_or_default(),
        )?;
        Ok(decision)
    }

    fn unique_passage(&self, document: Id, quote: &str) -> Option<&IndexedPassage> {
        if quote.is_empty() {
            return None;
        }
        let terms: Vec<_> = quote.split_whitespace().collect();
        let mut result = None;
        for passage in self.passages.get(&document)? {
            let paragraph: Vec<_> = passage.paragraph.split_whitespace().collect();
            for _ in paragraph.windows(terms.len()).filter(|part| *part == terms) {
                if result.replace(passage).is_some() {
                    return None;
                }
            }
        }
        result
    }

    fn signature(&self, finding: &Finding) -> Option<Vec<Statement>> {
        let mut signature = Vec::new();
        for source in &finding.sources {
            let quote = normalize_quote(&source.quote);
            let passage = self.unique_passage(source.document, &quote)?;
            signature.push(Statement {
                document: source.document,
                section: passage.section,
                paragraph: passage.paragraph.clone(),
                quote,
            });
        }
        sorted_signature(&mut signature).then_some(signature)
    }

    fn find_decision(&self, signature: &[Statement]) -> Option<&Decision> {
        self.decisions
            .iter()
            .find(|decision| decision.signature == signature)
    }
}

fn read_decision(yaml: &str) -> std::result::Result<DecisionMetadata, String> {
    let mut metadata: DecisionMetadata =
        serde_saphyr::from_str(yaml).map_err(|error| error.to_string())?;
    if metadata.finding_sources.len() != metadata.statement_signature.len()
        || !sorted_signature(&mut metadata.statement_signature)
    {
        return Err(
            "Mindestens zwei unterscheidbare Aussagen mit Fundstelle werden benötigt".into(),
        );
    }
    Ok(metadata)
}

fn sorted_signature(signature: &mut [Statement]) -> bool {
    signature.sort();
    signature.len() >= 2 && signature.windows(2).all(|pair| pair[0] != pair[1])
}

fn body_passages(body: &str) -> Vec<IndexedPassage> {
    let mut passages = Vec::new();
    let mut text = String::new();
    let mut body_range: Option<Range<usize>> = None;
    for (event, range) in Parser::new(body).into_offset_iter() {
        let end = match &event {
            Event::Start(tag) => Some(tag.to_end()),
            Event::End(tag) => Some(*tag),
            _ => None,
        };
        if matches!(
            end,
            Some(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock)
        ) || matches!(event, Event::Rule)
        {
            push_passage(&mut passages, &mut text, body_range.take());
        } else if let Event::Text(part) | Event::Code(part) = event {
            text.push_str(&part);
            if let Some(current) = &mut body_range {
                current.end = range.end;
            } else {
                body_range = Some(range);
            }
        } else if matches!(event, Event::SoftBreak | Event::HardBreak) {
            text.push(' ');
        }
    }
    push_passage(&mut passages, &mut text, body_range);
    passages
}

fn push_passage(
    passages: &mut Vec<IndexedPassage>,
    text: &mut String,
    body_range: Option<Range<usize>>,
) {
    let paragraph = normalize(text);
    if !paragraph.is_empty() {
        passages.push(IndexedPassage {
            section: SourceSection::Body,
            paragraph,
            body_range,
        });
    }
    text.clear();
}

fn normalize_quote(markdown: &str) -> String {
    body_passages(markdown)
        .into_iter()
        .map(|passage| passage.paragraph)
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize(text: &str) -> String {
    let mut output = String::new();
    let mut previous = None::<char>;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            character if character.is_alphanumeric() => output.extend(character.to_lowercase()),
            '.' | ','
                if previous.is_some_and(|c| c.is_ascii_digit())
                    && characters.peek().is_some_and(char::is_ascii_digit) =>
            {
                output.push(character)
            }
            '-' | '−' if characters.peek().is_some_and(char::is_ascii_digit) => {
                output.push_str(" - ")
            }
            '+' | '=' | '<' | '>' | '≤' | '≥' | '≠' | '/' => {
                output.push(' ');
                output.push(character);
                output.push(' ');
            }
            _ => output.push(' '),
        }
        previous = Some(character);
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}
