use std::collections::BTreeMap;

use crate::{Id, passage::Anchor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Pause,
    Leave,
    Manual,
}

#[derive(Debug)]
pub struct Suggestion {
    pub id: Id,
    pub source: Anchor,
    pub target: Id,
    pub text: String,
}

#[derive(Default)]
pub struct DetailQueue {
    enabled: bool,
    opened: BTreeMap<Id, u64>,
    requested: BTreeMap<Id, u64>,
    suggestions: BTreeMap<Id, Vec<Suggestion>>,
}

impl DetailQueue {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.requested.clear();
            self.suggestions.clear();
        }
    }

    pub fn open(&mut self, scene: Id, revision: u64) {
        self.opened.insert(scene, revision);
    }

    pub fn should_request(&mut self, trigger: Trigger, scene: Id, revision: u64) -> bool {
        if !self.enabled || trigger == Trigger::Pause {
            return false;
        }
        if trigger == Trigger::Leave
            && (self.opened.get(&scene) == Some(&revision)
                || self.requested.get(&scene) == Some(&revision))
        {
            return false;
        }
        self.requested.insert(scene, revision);
        true
    }

    pub fn suggestions(&self, scene: Id) -> &[Suggestion] {
        self.suggestions.get(&scene).map_or(&[], Vec::as_slice)
    }

    pub fn add(&mut self, scene: Id, suggestions: Vec<Suggestion>) {
        if self.enabled {
            self.suggestions
                .entry(scene)
                .or_default()
                .extend(suggestions);
        }
    }

    pub fn discard(&mut self, scene: Id, id: Id) {
        if let Some(suggestions) = self.suggestions.get_mut(&scene) {
            suggestions.retain(|suggestion| suggestion.id != id);
        }
    }
}
