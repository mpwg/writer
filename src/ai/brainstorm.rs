use std::{
    collections::BTreeSet,
    ops::{Bound, Range, RangeBounds},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::AiRequest;
use crate::{
    Document, DocumentKind, Id, Project, ProjectError,
    storage::Store,
    timeline::{Event, Relation, TemporalOrder, Timeline, TimelineError},
};

type Result<T> = std::result::Result<T, BrainstormError>;

#[derive(Debug, thiserror::Error)]
pub enum BrainstormError {
    #[error("Wähle zwei verschiedene Ereignisse.")]
    InvalidPair,
    #[error("Zu diesem Ereignispaar gibt es mehrere Gesprächsdateien.")]
    DuplicateConversation,
    #[error("Das Gespräch wurde außerhalb der Schreibwerkstatt geändert. Öffne es erneut.")]
    Changed,
    #[error("Die Gesprächsdatei kann nicht gelesen werden.")]
    InvalidConversation,
    #[error("Gib einen Text ein.")]
    EmptyText,
    #[error("Markiere Text in einer Codex-Antwort.")]
    InvalidSelection,
    #[error("Die markierte Idee fehlt.")]
    MissingIdea,
    #[error("Die Kennung der Idee ist bereits belegt.")]
    OccupiedIdea,
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Timeline(#[from] TimelineError),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Author,
    Codex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub id: Id,
    pub role: Role,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Idea {
    pub id: Id,
    pub message: Id,
    pub start: usize,
    pub end: usize,
    pub quote: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeaSource {
    pub idea: Id,
    pub message: Id,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeaPart {
    pub idea: Id,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    pub event: Id,
    pub direction: LinkDirection,
    pub kind: Relation,
    pub note: String,
}

pub struct Brainstorm {
    root: PathBuf,
    relative: PathBuf,
    pair: [Id; 2],
    id: Id,
    title: String,
    pair_context: String,
    messages: Vec<Message>,
    ideas: Vec<Idea>,
    original: Option<Vec<u8>>,
    thread_id: Option<String>,
}

#[derive(Deserialize)]
struct StoredChat {
    #[serde(default)]
    ideas: Vec<Idea>,
    #[serde(default)]
    messages: Vec<MessageHeader>,
}

#[derive(Serialize)]
struct ChatFields<'a> {
    id: Id,
    title: &'a str,
    kind: &'static str,
    references: [Id; 2],
    ideas: &'a [Idea],
    messages: Vec<MessageHeader>,
}

#[derive(Deserialize, Serialize)]
struct MessageHeader {
    id: Id,
    role: Role,
}

struct ParsedMessage {
    body_offset: usize,
    message: Message,
}

impl Brainstorm {
    pub fn open(project: &Project, mut pair: [Id; 2]) -> Result<Self> {
        if pair[0] == pair[1] {
            return Err(BrainstormError::InvalidPair);
        }
        pair.sort_unstable();
        let timeline = Timeline::from_project(project);
        let first = timeline
            .event(pair[0])
            .ok_or(TimelineError::MissingEvent(pair[0]))?;
        let second = timeline
            .event(pair[1])
            .ok_or(TimelineError::MissingEvent(pair[1]))?;
        let mut matches = project.documents().filter(|document| {
            Self::pair_for_document(document).is_some_and(|mut candidate| {
                candidate.sort_unstable();
                candidate == pair
            })
        });
        let document = matches.next();
        if matches.next().is_some() {
            return Err(BrainstormError::DuplicateConversation);
        }
        let root = project.root().to_owned();
        let (first, second, ordered) = match timeline.compare(pair[0], pair[1]) {
            TemporalOrder::Before => (first, second, true),
            TemporalOrder::After => (second, first, true),
            _ => (first, second, false),
        };
        let title = format!("{} / {}", first.title, second.title);
        let order = if ordered {
            "Das erste Ereignis liegt vor dem zweiten."
        } else {
            "Die zeitliche Reihenfolge zwischen den Ereignissen ist offen."
        };
        let pair_context = format!(
            "{order}\n\nErstes Ereignis: {}\n{}\n\nZweites Ereignis: {}\n{}",
            prefix(&first.title, 512),
            prefix(&first.body, 3_500),
            prefix(&second.title, 512),
            prefix(&second.body, 3_500)
        );
        let (id, relative, messages, ideas, original) = match document {
            Some(document) => {
                let (messages, ideas) = read_chat(document)?;
                let relative = Store::new(&root)?.relative_path(document)?;
                (
                    document.id(),
                    relative,
                    messages.into_iter().map(|parsed| parsed.message).collect(),
                    ideas,
                    Some(document.original().as_bytes().to_vec()),
                )
            }
            None => (
                Id::new_v4(),
                PathBuf::from("Brainstorming").join(format!("{}-{}.md", pair[0], pair[1])),
                Vec::new(),
                Vec::new(),
                None,
            ),
        };
        Ok(Self {
            root,
            relative,
            pair,
            id,
            title,
            pair_context,
            messages,
            ideas,
            original,
            thread_id: None,
        })
    }

    pub fn document_id(&self) -> Id {
        self.id
    }

    pub fn pair_for_document(document: &Document) -> Option<[Id; 2]> {
        let [first, second] = document.references() else {
            return None;
        };
        (document.kind() == DocumentKind::Brainstorm && first != second)
            .then_some([*first, *second])
    }

    /// Locate still-valid marked quotations in the original Markdown conversation.
    pub fn marked_idea_ranges(document: &Document) -> Result<Vec<IdeaSource>> {
        let (messages, ideas) = read_chat(document)?;
        Ok(ideas
            .into_iter()
            .filter_map(|idea| {
                let parsed = messages.iter().find(|parsed| {
                    parsed.message.id == idea.message && parsed.message.role == Role::Codex
                })?;
                let quote = parsed.message.text.get(idea.start..idea.end)?;
                (quote == idea.quote && !quote.trim().is_empty()).then_some(IdeaSource {
                    idea: idea.id,
                    message: idea.message,
                    range: parsed.body_offset + idea.start..parsed.body_offset + idea.end,
                })
            })
            .collect())
    }

    pub fn pair(&self) -> [Id; 2] {
        self.pair
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn ideas(&self) -> &[Idea] {
        &self.ideas
    }

    pub fn append(&mut self, role: Role, text: impl Into<String>) -> Result<Id> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(BrainstormError::EmptyText);
        }
        let id = Id::new_v4();
        self.messages.push(Message { id, role, text });
        if let Err(error) = self.save() {
            self.messages.pop();
            return Err(error);
        }
        Ok(id)
    }

    pub fn record_reply(
        &mut self,
        text: impl Into<String>,
        thread_id: Option<String>,
    ) -> Result<Id> {
        let message = self.append(Role::Codex, text)?;
        self.thread_id = thread_id;
        Ok(message)
    }

    pub fn mark(&mut self, message: Id, selection: impl RangeBounds<usize>) -> Result<Id> {
        let reply = self
            .messages
            .iter()
            .find(|reply| reply.id == message && reply.role == Role::Codex)
            .ok_or(BrainstormError::InvalidSelection)?;
        let start = match selection.start_bound() {
            Bound::Included(index) => *index,
            Bound::Excluded(index) => index
                .checked_add(1)
                .ok_or(BrainstormError::InvalidSelection)?,
            Bound::Unbounded => 0,
        };
        let end = match selection.end_bound() {
            Bound::Excluded(index) => *index,
            Bound::Included(index) => index
                .checked_add(1)
                .ok_or(BrainstormError::InvalidSelection)?,
            Bound::Unbounded => reply.text.len(),
        };
        let quote = reply
            .text
            .get(start..end)
            .filter(|quote| !quote.trim().is_empty())
            .ok_or(BrainstormError::InvalidSelection)?;
        if let Some(idea) = self.ideas.iter().find(|idea| {
            idea.message == message && idea.start == start && idea.end == end && idea.quote == quote
        }) {
            return Ok(idea.id);
        }
        let id = Id::new_v4();
        self.ideas.push(Idea {
            id,
            message,
            start,
            end,
            quote: quote.to_owned(),
        });
        if let Err(error) = self.save() {
            self.ideas.pop();
            return Err(error);
        }
        Ok(id)
    }

    pub fn request(&self, prompt: &str) -> AiRequest {
        match &self.thread_id {
            Some(thread_id) => AiRequest::new(prompt)
                .with_context(&self.pair_context)
                .resume(thread_id),
            None => self.fallback_request(prompt),
        }
    }

    pub fn fallback_request(&self, prompt: &str) -> AiRequest {
        const HISTORY_BYTES: usize = 14 * 1024;
        let mut remaining = HISTORY_BYTES;
        let mut messages = Vec::new();
        let skip_duplicate = usize::from(
            self.messages
                .last()
                .is_some_and(|message| message.role == Role::Author && message.text == prompt),
        );
        for message in self.messages.iter().rev().skip(skip_duplicate) {
            let label = role_label(message.role);
            let overhead = label.len() + 4;
            if remaining <= overhead {
                break;
            }
            let excerpt = prefix(&message.text, remaining - overhead);
            messages.push(format!("{label}: {excerpt}\n\n"));
            remaining -= overhead + excerpt.len();
        }
        messages.reverse();
        let context = format!(
            "{}\n\nBisheriges Gespräch:\n\n{}",
            self.pair_context,
            messages.concat()
        );
        AiRequest::new(prompt)
            .with_context(context)
            .with_thread_history()
    }

    /// Create a planned event once for a confirmed marked idea.
    pub fn promote(&mut self, choice: IdeaPart, links: &[Link]) -> Result<Id> {
        let idea = self
            .ideas
            .iter()
            .find(|idea| idea.id == choice.idea)
            .ok_or(BrainstormError::MissingIdea)?;
        if choice.title.trim().is_empty() {
            return Err(BrainstormError::EmptyText);
        }
        let project = Project::open(&self.root)?;
        let mut timeline = Timeline::from_project(&project);
        let id = idea.id;
        let url = format!(
            "../{}#nachricht-{}",
            markdown_path(&self.relative)?,
            idea.message
        );
        let source = format!("[Quelle im Gespräch]({url})");
        let event_exists = if let Some(event) = timeline.event(id) {
            if !event.body.lines().any(|line| line == source) {
                return Err(BrainstormError::OccupiedIdea);
            }
            true
        } else {
            if project.document(id).is_some() {
                return Err(BrainstormError::OccupiedIdea);
            }
            let event = Event {
                id,
                title: choice.title.trim().into(),
                body: format!("{}\n\n{source}\n", choice.body.trim_end()),
                time: None,
            };
            if !timeline.insert_event(event) {
                return Err(BrainstormError::OccupiedIdea);
            }
            false
        };

        let mut changed = BTreeSet::new();
        if !event_exists {
            changed.insert(id);
        }
        for link in links {
            let (from, to) = match link.direction {
                LinkDirection::Incoming => (link.event, id),
                LinkDirection::Outgoing => (id, link.event),
            };
            if timeline.relations().iter().any(|relation| {
                relation.from == from && relation.to == to && relation.kind == link.kind
            }) {
                continue;
            }
            timeline.relate(from, to, link.kind, link.note.clone())?;
            changed.insert(from);
        }
        let mut store = Store::new(&self.root)?;
        store.stage_batch(|store| {
            for id in changed {
                timeline.stage_event(id, &project, store)?;
            }
            Ok(())
        })?;
        store.flush()?;
        Ok(id)
    }

    fn save(&mut self) -> Result<()> {
        let project = Project::open(&self.root)?;
        let previous = project.document(self.id);
        if previous.map(|document| document.original().as_bytes()) != self.original.as_deref() {
            return Err(BrainstormError::Changed);
        }
        let fields = ChatFields {
            id: self.id,
            title: &self.title,
            kind: "brainstorm",
            references: self.pair,
            ideas: &self.ideas,
            messages: self
                .messages
                .iter()
                .map(|message| MessageHeader {
                    id: message.id,
                    role: message.role,
                })
                .collect(),
        };
        let mut body = String::new();
        for message in &self.messages {
            body.push_str(&marker(message.id, message.role));
            body.push_str(&message.text);
            body.push_str("\n\n");
        }
        let mut store = Store::new(&self.root)?;
        store.stage_document(&self.relative, previous, fields, &body)?;
        store.flush()?;
        let project = Project::open(&self.root)?;
        let document = project.document(self.id).ok_or(BrainstormError::Changed)?;
        let (messages, ideas) = read_chat(document)?;
        if !messages
            .iter()
            .map(|parsed| &parsed.message)
            .eq(&self.messages)
            || ideas != self.ideas
        {
            return Err(BrainstormError::Changed);
        }
        self.original = Some(document.original().as_bytes().to_vec());
        Ok(())
    }
}

fn read_chat(document: &Document) -> Result<(Vec<ParsedMessage>, Vec<Idea>)> {
    let stored: StoredChat = serde_saphyr::from_str(document.frontmatter())
        .map_err(|_| BrainstormError::InvalidConversation)?;
    let mut remainder = document.body();
    let mut messages = Vec::with_capacity(stored.messages.len());
    let mut headers = stored.messages.into_iter().peekable();
    while let Some(header) = headers.next() {
        let marker_here = marker(header.id, header.role);
        remainder = remainder
            .strip_prefix(marker_here.as_str())
            .ok_or(BrainstormError::InvalidConversation)?;
        let body_offset = document.body().len() - remainder.len();
        let text = match headers.peek() {
            Some(next) => {
                let marker_next = marker(next.id, next.role);
                let (text, following) = remainder
                    .split_once(&marker_next)
                    .ok_or(BrainstormError::InvalidConversation)?;
                remainder = &remainder[remainder.len() - following.len() - marker_next.len()..];
                text
            }
            None => remainder,
        };
        let text = text.strip_suffix("\n\n").unwrap_or(text);
        messages.push(ParsedMessage {
            body_offset,
            message: Message {
                id: header.id,
                role: header.role,
                text: text.to_owned(),
            },
        });
    }
    Ok((messages, stored.ideas))
}

fn marker(id: Id, role: Role) -> String {
    format!(
        "<a id=\"nachricht-{id}\"></a>\n\n## {}\n\n",
        role_label(role)
    )
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Author => "Anfrage",
        Role::Codex => "Codex",
    }
}

fn prefix(text: &str, max_bytes: usize) -> &str {
    &text[..text.floor_char_boundary(max_bytes.min(text.len()))]
}

fn markdown_path(path: &Path) -> Result<String> {
    use std::fmt::Write;

    let mut encoded = String::new();
    for component in path.iter() {
        if !encoded.is_empty() {
            encoded.push('/');
        }
        let segment = component
            .to_str()
            .ok_or(BrainstormError::InvalidConversation)?;
        for byte in segment.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                encoded.push(char::from(byte));
            } else {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    Ok(encoded)
}
