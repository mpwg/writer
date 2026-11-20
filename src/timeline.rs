use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{DocumentKind, Id, Project, ProjectError, storage::Store};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Before,
    Cause,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporalOrder {
    Before,
    After,
    Same,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    pub id: Id,
    pub title: String,
    pub body: String,
    pub time: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct EventRelation {
    #[serde(skip)]
    pub from: Id,
    pub to: Id,
    pub kind: Relation,
    #[serde(default)]
    pub note: String,
    pub time: Option<String>,
    #[serde(flatten)]
    other: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TimelineError {
    #[error("Ereignis {0} fehlt")]
    MissingEvent(Id),
    #[error("Die gewählte Verbindung fehlt")]
    MissingRelation,
    #[error("Zwischen diesen Ereignissen besteht bereits eine Verbindung dieses Typs")]
    DuplicateRelation,
    #[error("Die Verbindung würde einen Zeitkreis schließen: {}", display_path(.path))]
    Cycle { path: Vec<Id> },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TimelineIssue {
    #[error("Die Verbindung von {from} verweist auf ein fehlendes Ereignis {to}")]
    MissingEvent { from: Id, to: Id },
    #[error("Der Zeitkreis lässt keine eindeutige Reihenfolge zu: {}", display_path(.path))]
    Cycle { path: Vec<Id> },
    #[error("Die Zeitangaben von Ereignis {event} konnten nicht gelesen werden: {reason}")]
    InvalidMetadata { event: Id, reason: String },
}

#[derive(Default, Deserialize)]
struct EventMetadata {
    event_time: Option<String>,
    #[serde(default)]
    relations: Vec<EventRelation>,
}

impl EventMetadata {
    fn parse(id: Id, yaml: &str) -> Result<Self, serde_saphyr::Error> {
        let mut metadata: Self = serde_saphyr::from_str(yaml)?;
        for relation in &mut metadata.relations {
            relation.from = id;
        }
        Ok(metadata)
    }
}

#[derive(Default, Serialize)]
struct EventFields<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_time: Option<Option<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    relations: Option<Vec<&'a EventRelation>>,
}

#[derive(Default)]
pub struct Timeline {
    events: BTreeMap<Id, Event>,
    relations: Vec<EventRelation>,
    metadata_issues: Vec<TimelineIssue>,
}

impl Timeline {
    pub fn from_project(project: &Project) -> Self {
        let mut timeline = Self::default();
        for document in project
            .documents()
            .filter(|document| document.kind() == DocumentKind::Event)
        {
            let id = document.id();
            let metadata = match EventMetadata::parse(id, document.frontmatter()) {
                Ok(metadata) => metadata,
                Err(error) => {
                    timeline
                        .metadata_issues
                        .push(TimelineIssue::InvalidMetadata {
                            event: id,
                            reason: error.to_string(),
                        });
                    EventMetadata::default()
                }
            };
            timeline.events.insert(
                id,
                Event {
                    id,
                    title: document.title().into(),
                    body: document.body().into(),
                    time: metadata.event_time,
                },
            );
            timeline.relations.extend(metadata.relations);
        }
        timeline
    }

    pub fn create_event(&mut self, title: &str, body: &str, time: Option<String>) -> Id {
        let id = Id::new_v4();
        self.events.insert(
            id,
            Event {
                id,
                title: title.into(),
                body: body.into(),
                time,
            },
        );
        id
    }

    pub(crate) fn insert_event(&mut self, event: Event) -> bool {
        use std::collections::btree_map::Entry;

        match self.events.entry(event.id) {
            Entry::Vacant(entry) => {
                entry.insert(event);
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    pub fn update_event(
        &mut self,
        id: Id,
        title: &str,
        body: &str,
        time: Option<String>,
    ) -> Result<(), TimelineError> {
        let event = self
            .events
            .get_mut(&id)
            .ok_or(TimelineError::MissingEvent(id))?;
        event.title = title.into();
        event.body = body.into();
        event.time = time;
        Ok(())
    }

    pub fn event(&self, id: Id) -> Option<&Event> {
        self.events.get(&id)
    }

    pub fn relations(&self) -> &[EventRelation] {
        &self.relations
    }

    pub fn relate(
        &mut self,
        from: Id,
        to: Id,
        kind: Relation,
        note: String,
    ) -> Result<(), TimelineError> {
        self.relate_at(from, to, kind, note, None)
    }

    pub fn relate_at(
        &mut self,
        from: Id,
        to: Id,
        kind: Relation,
        note: String,
        time: Option<String>,
    ) -> Result<(), TimelineError> {
        for id in [from, to] {
            if !self.events.contains_key(&id) {
                return Err(TimelineError::MissingEvent(id));
            }
        }
        if let Some(relation) = self
            .relations
            .iter_mut()
            .find(|relation| (relation.from, relation.to, relation.kind) == (from, to, kind))
        {
            relation.note = note;
            relation.time = time;
            return Ok(());
        }
        if let Some(path) = self.graph().path(to, from) {
            return Err(TimelineError::Cycle {
                path: std::iter::once(from).chain(path).collect(),
            });
        }
        self.relations.push(EventRelation {
            from,
            to,
            kind,
            note,
            time,
            other: BTreeMap::new(),
        });
        Ok(())
    }

    /// Changes the kind of an existing edge while preserving its metadata and endpoints.
    ///
    /// # Errors
    ///
    /// Returns an error if the selected edge is missing or the destination kind exists.
    pub fn change_relation_kind(
        &mut self,
        from: Id,
        to: Id,
        previous: Relation,
        kind: Relation,
    ) -> Result<(), TimelineError> {
        let matches = |relation: &EventRelation| relation.from == from && relation.to == to;
        let index = self
            .relations
            .iter()
            .position(|relation| matches(relation) && relation.kind == previous)
            .ok_or(TimelineError::MissingRelation)?;
        if previous != kind
            && self
                .relations
                .iter()
                .any(|relation| matches(relation) && relation.kind == kind)
        {
            return Err(TimelineError::DuplicateRelation);
        }
        // Both kinds imply temporal precedence in the same direction.
        self.relations[index].kind = kind;
        Ok(())
    }

    pub fn remove_relation(&mut self, from: Id, to: Id, kind: Relation) -> bool {
        let previous_len = self.relations.len();
        self.relations
            .retain(|relation| (relation.from, relation.to, relation.kind) != (from, to, kind));
        self.relations.len() != previous_len
    }

    pub fn compare(&self, a: Id, b: Id) -> TemporalOrder {
        if !self.events.contains_key(&a) || !self.events.contains_key(&b) {
            return TemporalOrder::Unknown;
        }
        if a == b {
            return TemporalOrder::Same;
        }
        let graph = self.graph();
        match (graph.path(a, b).is_some(), graph.path(b, a).is_some()) {
            (true, false) => TemporalOrder::Before,
            (false, true) => TemporalOrder::After,
            _ => TemporalOrder::Unknown,
        }
    }

    pub fn ordered_events(&self) -> Vec<&Event> {
        let graph = self.graph();
        let mut pending: BTreeMap<_, usize> = self.events.keys().map(|&id| (id, 0)).collect();
        for to in graph.edges.values().flatten() {
            pending.entry(*to).and_modify(|count| *count += 1);
        }
        let mut ready: BTreeSet<_> = pending
            .iter()
            .filter_map(|(&id, &count)| (count == 0).then_some(id))
            .collect();
        let mut ordered = Vec::with_capacity(pending.len());
        while let Some(id) = ready.pop_first() {
            pending.remove(&id);
            ordered.extend(self.event(id));
            for successor in graph.successors(id) {
                if let Some(count) = pending.get_mut(&successor) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(successor);
                    }
                }
            }
        }
        // Keep every card accessible while an externally introduced cycle is repaired.
        ordered.extend(pending.into_keys().filter_map(|id| self.event(id)));
        ordered
    }

    /// Groups connected events by the longest predecessor chain, leaving isolated events out.
    pub fn event_columns(&self) -> Vec<Vec<&Event>> {
        let graph = self.graph();
        let connected: BTreeSet<_> = graph
            .edges
            .keys()
            .chain(graph.edges.values().flatten())
            .copied()
            .collect();
        let mut columns = Vec::<Vec<_>>::new();
        let mut ranks = BTreeMap::<Id, usize>::new();
        let mut placed = BTreeSet::new();
        for event in self
            .ordered_events()
            .into_iter()
            .filter(|event| connected.contains(&event.id))
        {
            let rank = ranks.remove(&event.id).unwrap_or_default();
            if columns.len() <= rank {
                columns.resize_with(rank + 1, Vec::new);
            }
            columns[rank].push(event);
            placed.insert(event.id);
            for successor in graph.successors(event.id).filter(|id| !placed.contains(id)) {
                let successor_rank = ranks.entry(successor).or_default();
                *successor_rank = (*successor_rank).max(rank + 1);
            }
        }
        columns
    }

    pub fn issues(&self) -> Vec<TimelineIssue> {
        let mut issues = self.metadata_issues.clone();
        issues.extend(self.relations.iter().filter_map(|relation| {
            (!self.events.contains_key(&relation.to)).then_some(TimelineIssue::MissingEvent {
                from: relation.from,
                to: relation.to,
            })
        }));
        issues.extend(
            self.graph()
                .cycles(self.events.keys().copied())
                .map(|path| TimelineIssue::Cycle { path }),
        );
        issues
    }

    pub fn stage_event(&self, id: Id, project: &Project, store: &mut Store) -> crate::Result<()> {
        let error = |reason| ProjectError::Storage {
            path: project.root().to_owned(),
            reason,
        };
        let event = self
            .event(id)
            .ok_or_else(|| error(TimelineError::MissingEvent(id).to_string()))?;
        let relations: Vec<_> = self
            .relations
            .iter()
            .filter(|relation| relation.from == id)
            .collect();
        let Some(previous) = project.document(id) else {
            return store.stage_document(
                &Path::new("Planung").join(format!("{id}.md")),
                None,
                EventFields {
                    id: Some(id),
                    kind: Some("event"),
                    title: Some(&event.title),
                    event_time: event.time.as_deref().map(Some),
                    relations: (!relations.is_empty()).then_some(relations),
                },
                &event.body,
            );
        };
        if previous.kind() != DocumentKind::Event {
            return Err(error(format!(
                "Kennung {id} gehört zu einem Dokument ohne Ereignistyp"
            )));
        }
        let relative = store.relative_path(previous)?;
        let metadata = EventMetadata::parse(id, previous.frontmatter());
        let (time, links) = match &metadata {
            Ok(metadata) => (
                metadata.event_time.as_deref(),
                metadata.relations.as_slice(),
            ),
            Err(_) => (None, &[][..]),
        };
        let time_changed = event.time.as_deref() != time;
        let relations_changed = !relations.iter().copied().eq(links);
        if (time_changed || relations_changed)
            && let Err(source) = metadata
        {
            return Err(ProjectError::Storage {
                path: previous.path().to_owned(),
                reason: source.to_string(),
            });
        }
        let title_changed = previous.title() != event.title;
        if !title_changed && !time_changed && !relations_changed {
            if previous.body() != event.body {
                store.stage_body(&relative, previous, &event.body)?;
            }
            return Ok(());
        }
        store.stage_document(
            &relative,
            Some(previous),
            EventFields {
                title: title_changed.then_some(&event.title),
                event_time: time_changed.then_some(event.time.as_deref()),
                relations: relations_changed.then_some(relations),
                ..EventFields::default()
            },
            &event.body,
        )
    }

    fn graph(&self) -> Graph {
        let mut graph = Graph::default();
        for relation in &self.relations {
            if self.events.contains_key(&relation.to) {
                graph
                    .edges
                    .entry(relation.from)
                    .or_default()
                    .insert(relation.to);
            }
        }
        graph
    }
}

#[derive(Default)]
struct Graph {
    edges: BTreeMap<Id, BTreeSet<Id>>,
}

impl Graph {
    fn successors(&self, id: Id) -> impl DoubleEndedIterator<Item = Id> + '_ {
        self.edges.get(&id).into_iter().flatten().copied()
    }

    fn path(&self, start: Id, target: Id) -> Option<Vec<Id>> {
        use std::collections::btree_map::Entry;

        let mut parents = BTreeMap::from([(start, start)]);
        let mut queue = VecDeque::from([start]);
        while let Some(id) = queue.pop_front() {
            if id == target {
                let mut path = vec![id];
                let mut current = id;
                while current != start {
                    current = parents[&current];
                    path.push(current);
                }
                path.reverse();
                return Some(path);
            }
            for successor in self.successors(id) {
                if let Entry::Vacant(entry) = parents.entry(successor) {
                    entry.insert(id);
                    queue.push_back(successor);
                }
            }
        }
        None
    }

    fn cycles(&self, roots: impl Iterator<Item = Id>) -> impl Iterator<Item = Vec<Id>> {
        enum Visit {
            Enter(Id),
            Exit(Id),
        }
        let mut cycles = Vec::new();
        let mut path = Vec::new();
        let mut active = BTreeMap::new();
        let mut complete = BTreeSet::new();
        let mut visits = Vec::new();
        for root in roots {
            visits.push(Visit::Enter(root));
            while let Some(visit) = visits.pop() {
                match visit {
                    Visit::Enter(id) if complete.contains(&id) => {}
                    Visit::Enter(id) => {
                        if let Some(&start) = active.get(&id) {
                            let mut cycle = path[start..].to_vec();
                            cycle.push(id);
                            cycles.push(cycle);
                        } else {
                            active.insert(id, path.len());
                            path.push(id);
                            visits.push(Visit::Exit(id));
                            visits.extend(self.successors(id).rev().map(Visit::Enter));
                        }
                    }
                    Visit::Exit(id) => {
                        path.pop();
                        active.remove(&id);
                        complete.insert(id);
                    }
                }
            }
        }
        cycles.into_iter()
    }
}

fn display_path(path: &[Id]) -> String {
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" → ")
}
