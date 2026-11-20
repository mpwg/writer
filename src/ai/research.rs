use crate::{
    Id,
    timeline::{TemporalOrder, Timeline},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    Prose,
    Concept,
    Planning,
    OpenIdea,
    CharacterKnowledge,
    AuthorKnowledge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TemporalScope {
    AtOrBefore,
    After,
    Unordered,
}

#[derive(Clone, Copy, Debug)]
pub struct Evidence {
    pub document: Id,
    pub event: Option<Id>,
    pub scenario: Scenario,
}

impl Evidence {
    pub fn scope(&self, timeline: &Timeline, at: Option<Id>) -> TemporalScope {
        match self
            .event
            .zip(at)
            .map(|(event, at)| timeline.compare(event, at))
        {
            Some(TemporalOrder::Before | TemporalOrder::Same) => TemporalScope::AtOrBefore,
            Some(TemporalOrder::After) => TemporalScope::After,
            _ => TemporalScope::Unordered,
        }
    }
}

pub fn sort_evidence(evidence: &mut [Evidence], timeline: &Timeline, at: Option<Id>) {
    evidence.sort_by_key(|entry| entry.scope(timeline, at));
    for group in evidence.chunk_by_mut(|a, b| a.scope(timeline, at) == b.scope(timeline, at)) {
        for start in 0..group.len() {
            let event_at = |index: usize| group[index].event;
            let latest = (start..group.len()).find(|&candidate| {
                !(start..group.len()).any(|other| {
                    event_at(candidate)
                        .zip(event_at(other))
                        .is_some_and(|(a, b)| timeline.compare(a, b) == TemporalOrder::Before)
                })
            });
            if let Some(latest) = latest {
                group[start..=latest].rotate_right(1);
            }
        }
    }
}
