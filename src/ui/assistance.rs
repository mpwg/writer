use super::*;
use gpui_kit::{
    base::StyledExt as _,
    component::{
        resizable::{h_resizable, resizable_panel},
        switch::Switch,
    },
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, VecDeque},
    hash::{Hash, Hasher},
    ops::Range,
};
use writer::{
    DocumentKind,
    ai::{
        AiEvent, AiPreferences, AiRequest, AiSession, AiStatus, RequestId,
        brainstorm::{Brainstorm, IdeaPart, Link, LinkDirection, Role},
        consistency::{Finding, FindingSource, Review, ReviewStatus, SourceSection},
        details::{DetailQueue, Suggestion, Trigger},
        draft::Draft,
        research::{Evidence, Scenario, TemporalScope},
    },
    outline::Outline,
    passage::{Anchor, SceneText},
    sources::{Resolution, Sources},
    timeline::{Event, Relation, TemporalOrder, Timeline},
};

enum Pending {
    Brainstorm {
        pair: [Id; 2],
        prompt: String,
        retried: bool,
    },
    Draft {
        anchor: Anchor,
        prompt: String,
    },
    Detail {
        scene: Id,
        snapshot: SceneText,
    },
    Research(ResearchRun),
    ResearchSummary(Vec<Citation>),
    Consistency(ConsistencyRun),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Activity {
    Connection,
    Brainstorm,
    Research,
    Consistency,
    Scene,
}

impl Pending {
    fn activity(&self) -> Activity {
        match self {
            Self::Brainstorm { .. } => Activity::Brainstorm,
            Self::Research(_) | Self::ResearchSummary(_) => Activity::Research,
            Self::Consistency(_) => Activity::Consistency,
            Self::Draft { .. } | Self::Detail { .. } => Activity::Scene,
        }
    }
}

#[derive(Clone)]
struct Citation {
    evidence: Evidence,
    title: String,
    anchor: Anchor,
    metadata: bool,
    holder: Option<String>,
}

struct ResearchRun {
    question: String,
    at: Option<Id>,
    rest: VecDeque<Vec<Citation>>,
    current: Vec<Citation>,
}

struct ConsistencyRun {
    batches: Vec<Vec<Citation>>,
    left: usize,
    right: usize,
    current: Vec<Citation>,
}

impl ConsistencyRun {
    fn advance(&mut self) -> bool {
        self.right += 1;
        if self.right >= self.batches.len() {
            self.left += 1;
            self.right = self.left;
        }
        let Some(left) = self.batches.get(self.left) else {
            return false;
        };
        self.current.clone_from(left);
        if self.left != self.right {
            self.current.extend_from_slice(&self.batches[self.right]);
        }
        true
    }
}

#[derive(Deserialize)]
struct ModelAnswer {
    statement: String,
    sources: Vec<usize>,
}

struct Answer {
    statement: String,
    citations: Vec<Citation>,
}

#[derive(Deserialize)]
struct ModelFinding {
    title: String,
    sources: Vec<usize>,
}

#[derive(Deserialize)]
struct ModelDetail {
    quote: String,
    target: Id,
    text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FindingSelection {
    Open(usize),
    Marked(Id),
}

pub(super) struct Assistance {
    preferences: Option<AiPreferences>,
    session: AiSession,
    enabled: bool,
    cli: Option<PathBuf>,
    poll_task: Option<Task<()>>,
    pending: Option<(RequestId, Pending)>,
    input: Entity<TextareaState>,
    brainstorm_input: Entity<TextareaState>,
    research_input: Entity<TextareaState>,
    excerpt: Entity<TextareaState>,
    answer: Entity<TextareaState>,
    note: Entity<InputState>,
    consistency_reason: Entity<InputState>,
    settings_origin: Option<(Page, Option<FocusHandle>)>,
    pair_seed: Option<Id>,
    brainstorm: Option<Brainstorm>,
    brainstorm_message: Option<Id>,
    idea: Option<Id>,
    links: Vec<Link>,
    login: Option<String>,
    messages: BTreeMap<Activity, String>,
    streaming: String,
    at: Option<Id>,
    answers: Vec<Answer>,
    findings: Vec<Finding>,
    selected_finding: Option<FindingSelection>,
    show_decisions: bool,
    explaining_finding: Option<usize>,
    draft: Option<Draft>,
    choosing_draft_target: bool,
    draft_thread: Option<(Id, String)>,
    details: DetailQueue,
    delayed_details: BTreeMap<Id, SceneText>,
    editing_suggestion: Option<Id>,
}

impl Assistance {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ProjectView>) -> Self {
        let preferences = AiPreferences::load().ok();
        Self {
            session: preferences
                .as_ref()
                .map_or_else(AiSession::new, AiSession::with_preferences),
            preferences,
            enabled: false,
            cli: None,
            poll_task: None,
            pending: None,
            input: cx.new(|cx| TextareaState::new(window, cx).rows(4)),
            brainstorm_input: cx.new(|cx| TextareaState::new(window, cx).rows(3)),
            research_input: cx.new(|cx| TextareaState::new(window, cx).rows(3)),
            excerpt: cx.new(|cx| TextareaState::new(window, cx).rows(3)),
            answer: cx.new(|cx| TextareaState::new(window, cx).rows(7)),
            note: cx.new(|cx| InputState::new(window, cx)),
            consistency_reason: cx.new(|cx| InputState::new(window, cx)),
            settings_origin: None,
            pair_seed: None,
            brainstorm: None,
            brainstorm_message: None,
            idea: None,
            links: Vec::new(),
            login: None,
            messages: BTreeMap::new(),
            streaming: String::new(),
            at: None,
            answers: Vec::new(),
            findings: Vec::new(),
            selected_finding: None,
            show_decisions: false,
            explaining_finding: None,
            draft: None,
            choosing_draft_target: false,
            draft_thread: None,
            details: DetailQueue::default(),
            delayed_details: BTreeMap::new(),
            editing_suggestion: None,
        }
    }

    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<ProjectView>) {
        self.stop();
        self.pair_seed = None;
        self.clear_brainstorm();
        self.at = None;
        self.answers.clear();
        self.findings.clear();
        self.selected_finding = None;
        self.show_decisions = false;
        self.explaining_finding = None;
        self.settings_origin = None;
        self.draft = None;
        self.choosing_draft_target = false;
        self.draft_thread = None;
        for field in [
            &self.input,
            &self.brainstorm_input,
            &self.research_input,
            &self.answer,
            &self.excerpt,
        ] {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
        for field in [&self.note, &self.consistency_reason] {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
    }

    fn clear_brainstorm(&mut self) {
        self.brainstorm = None;
        self.brainstorm_message = None;
        self.idea = None;
        self.links.clear();
    }

    fn stop(&mut self) {
        self.session.disable();
        self.enabled = false;
        self.pending = None;
        self.login = None;
        self.poll_task = None;
        self.streaming.clear();
        self.messages.clear();
        self.details.set_enabled(false);
        self.delayed_details.clear();
        self.editing_suggestion = None;
    }
}

impl ProjectView {
    pub(super) fn open_project_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let origin = self.page;
        let focus = window.focused(cx);
        self.navigate(Page::Settings, window, cx);
        if origin != Page::Settings && self.page == Page::Settings {
            self.ai.settings_origin = Some((origin, focus));
        }
    }

    fn close_project_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let origin = self
            .ai
            .settings_origin
            .as_ref()
            .map_or(Page::Timeline, |(page, _)| *page);
        self.navigate(origin, window, cx);
        if self.page == origin
            && let Some((_, Some(focus))) = self.ai.settings_origin.take()
        {
            focus.focus(window, cx);
        }
    }

    pub(super) fn restore_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        if self
            .ai
            .preferences
            .as_ref()
            .and_then(|prefs| prefs.is_enabled(project.root(), project.id()).ok())
            == Some(true)
        {
            self.set_ai(true, window, cx);
        }
    }

    fn set_ai(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        if !enabled {
            self.ai.stop();
        }
        if let Some(prefs) = &self.ai.preferences
            && let Err(error) = prefs.set_enabled(project.root(), project.id(), enabled)
        {
            self.ai
                .messages
                .insert(Activity::Connection, error.to_string());
            cx.notify();
            return;
        }
        if enabled {
            self.ai.enabled = true;
            self.ai.details.set_enabled(true);
            self.connect_ai(window, cx);
        }
        cx.notify();
    }

    fn connect_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ai.enabled {
            return;
        }
        self.ai.poll_task = None;
        self.ai.messages.remove(&Activity::Connection);
        if let Err(error) = self.ai.session.enable(self.ai.cli.as_deref()) {
            self.ai
                .messages
                .insert(Activity::Connection, error.to_string());
        }
        self.ai.login = None;
        self.ai.poll_task = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(180))
                    .await;
                let result = view.update_in(cx, |view, window, cx| view.poll_ai(window, cx));
                if !matches!(result, Ok(true)) {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn choose_codex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Codex-Datei wählen".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            if let Ok(Ok(Some(paths))) = dialog.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = view.update_in(cx, |view, window, cx| {
                    view.ai.cli = Some(path);
                    view.connect_ai(window, cx);
                });
            }
        })
        .detach();
    }

    fn request_ai(&mut self, request: AiRequest, pending: Pending, cx: &mut Context<Self>) -> bool {
        if self.ai.pending.is_some() {
            return false;
        }
        let activity = pending.activity();
        match self.ai.session.request(request) {
            Ok(id) => {
                self.ai.pending = Some((id, pending));
                self.ai.streaming.clear();
                self.ai.messages.remove(&activity);
                cx.notify();
                true
            }
            Err(error) => {
                self.ai.messages.insert(activity, error.to_string());
                cx.notify();
                false
            }
        }
    }

    fn poll_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.ai.enabled {
            return false;
        }
        let previous = self.ai.session.status().clone();
        let events = self.ai.session.poll();
        if !events.is_empty() || &previous != self.ai.session.status() {
            cx.notify();
        }
        for event in events {
            match event {
                AiEvent::LoginUrl(url) => self.ai.login = Some(url),
                AiEvent::ConnectionError(error) => {
                    self.ai
                        .messages
                        .insert(Activity::Connection, error.to_string());
                    self.ai.login = None;
                }
                AiEvent::Delta { request_id, text } => {
                    if self
                        .ai
                        .pending
                        .as_ref()
                        .is_some_and(|(id, _)| *id == request_id)
                    {
                        self.ai.streaming.push_str(&text);
                    }
                }
                AiEvent::Completed {
                    request_id,
                    text,
                    thread_id,
                } => {
                    if self
                        .ai
                        .pending
                        .as_ref()
                        .is_some_and(|(id, _)| *id == request_id)
                        && let Some((_, pending)) = self.ai.pending.take()
                    {
                        self.ai.streaming.clear();
                        self.complete_ai(pending, text, thread_id, window, cx);
                    }
                }
                AiEvent::Failed { request_id, error } => {
                    if self
                        .ai
                        .pending
                        .as_ref()
                        .is_some_and(|(id, _)| *id == request_id)
                    {
                        self.ai.streaming.clear();
                        let pending = self.ai.pending.take().map(|(_, pending)| pending);
                        if let Some(activity) = pending.as_ref().map(Pending::activity) {
                            let message =
                                if activity == Activity::Research && !self.ai.answers.is_empty() {
                                    format!("Recherche unvollständig. {error}")
                                } else {
                                    error.to_string()
                                };
                            self.ai.messages.insert(activity, message);
                        }
                        if matches!(error, writer::ai::AiError::ThreadUnavailable)
                            && let Some(Pending::Brainstorm {
                                pair,
                                prompt,
                                retried: false,
                            }) = pending
                            && let Some(chat) = self
                                .ai
                                .brainstorm
                                .as_ref()
                                .filter(|chat| chat.pair() == pair)
                        {
                            self.request_ai(
                                chat.fallback_request(&prompt),
                                Pending::Brainstorm {
                                    pair,
                                    prompt,
                                    retried: true,
                                },
                                cx,
                            );
                        }
                    }
                }
            }
        }
        if matches!(self.ai.session.status(), AiStatus::Ready) {
            self.ai.login = None;
            if self.ai.pending.is_none()
                && let Some((scene, snapshot)) = self.ai.delayed_details.pop_first()
            {
                self.send_detail(scene, snapshot, cx);
            }
        }
        self.ai.enabled
    }

    fn complete_ai(
        &mut self,
        pending: Pending,
        text: String,
        thread_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match pending {
            Pending::Brainstorm { pair, .. } => {
                if let Some(chat) = self
                    .ai
                    .brainstorm
                    .as_mut()
                    .filter(|chat| chat.pair() == pair)
                {
                    match chat.record_reply(text, thread_id) {
                        Ok(id) => self.open_chat_message(id, window, cx),
                        Err(error) => {
                            self.ai
                                .messages
                                .insert(Activity::Brainstorm, error.to_string());
                        }
                    }
                }
            }
            Pending::Draft { anchor, prompt } => {
                let scene = anchor.document_id();
                self.ai.draft_thread = thread_id.map(|thread| (scene, thread));
                let draft = Draft::new(anchor, text.clone());
                if let Some((project, store)) = self.project.as_ref().zip(self.store.as_mut()) {
                    let result = draft.stage(project, store, &prompt);
                    if self.report(result, cx) {
                        self.flush(cx);
                    }
                }
                self.ai
                    .answer
                    .update(cx, |answer, cx| answer.set_value(text.clone(), window, cx));
                self.ai.draft = Some(draft);
                self.ai.choosing_draft_target = false;
            }
            Pending::Detail { scene, snapshot } => {
                let Some(project) = &self.project else { return };
                let suggestions = read_model::<Vec<ModelDetail>>(&text)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|detail| {
                        let found = unique_quote(snapshot.text(), &detail.quote)?;
                        let target = project.document(detail.target)?;
                        if !world_kind(target.kind()) || detail.text.trim().is_empty() {
                            return None;
                        }
                        Some(Suggestion {
                            id: Id::new_v4(),
                            source: snapshot.anchor(found).ok()?,
                            target: detail.target,
                            text: detail.text,
                        })
                    })
                    .collect();
                self.ai.details.add(scene, suggestions);
            }
            Pending::Research(run) => self.continue_research(run, &text, cx),
            Pending::ResearchSummary(citations) => {
                let answers = self.verified_answers(&text, &citations);
                if let Some(answers) = answers.filter(|items| !items.is_empty()) {
                    self.ai.answers = answers;
                    self.ai
                        .messages
                        .insert(Activity::Research, "Recherche abgeschlossen.".into());
                } else {
                    self.ai.messages.insert(Activity::Research, "Die Zusammenfassung ließ sich nicht belegen. Die geprüften Einzelantworten sind unten aufgeführt.".into());
                }
            }
            Pending::Consistency(run) => self.continue_consistency(run, &text, cx),
        }
    }

    fn input_prompt(&self, cx: &App) -> String {
        self.ai.input.read(cx).value().trim().to_owned()
    }

    pub(super) fn jump_brainstorm(
        &mut self,
        id: Id,
        message: Option<Id>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai.pending.is_some() {
            return;
        }
        let Some(project) = &self.project else { return };
        let pair = project.document(id).and_then(Brainstorm::pair_for_document);
        if let Some([a, b]) = pair {
            self.select_pair_seed(a, cx);
            self.open_pair(b, cx);
            if let Some(message) = message {
                self.open_chat_message(message, window, cx);
                self.ai
                    .answer
                    .update(cx, |answer, cx| answer.focus(window, cx));
            }
        }
    }

    pub(super) fn start_pair(&mut self, event: Id, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Codex, window, cx);
        if self.page == Page::Codex {
            self.select_pair_seed(event, cx);
        }
    }

    fn select_pair_seed(&mut self, event: Id, cx: &mut Context<Self>) {
        if self.ai.pending.is_none() && self.ai.pair_seed != Some(event) {
            self.ai.pair_seed = Some(event);
            self.ai.clear_brainstorm();
            cx.notify();
        }
    }

    fn open_pair(&mut self, second: Id, cx: &mut Context<Self>) {
        if self.ai.pending.is_some() {
            return;
        }
        let Some(seed) = self.ai.pair_seed else {
            return;
        };
        let mut pair = [seed, second];
        pair.sort_unstable();
        if self
            .ai
            .brainstorm
            .as_ref()
            .is_some_and(|chat| chat.pair() == pair)
        {
            return;
        }
        self.ai.clear_brainstorm();
        if let Err(error) = self.refresh_project() {
            self.ai
                .messages
                .insert(Activity::Brainstorm, error.to_string());
            cx.notify();
            return;
        }
        let Some(project) = &self.project else { return };
        match Brainstorm::open(project, pair) {
            Ok(chat) => {
                self.ai.brainstorm = Some(chat);
                self.ai.messages.remove(&Activity::Brainstorm);
            }
            Err(error) => {
                self.ai
                    .messages
                    .insert(Activity::Brainstorm, error.to_string());
            }
        }
        cx.notify();
    }

    fn send_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai.pending.is_some() {
            return;
        }
        let prompt = self.ai.brainstorm_input.read(cx).value().trim().to_owned();
        if prompt.is_empty() {
            return;
        }
        let Some(chat) = self.ai.brainstorm.as_mut() else {
            return;
        };
        let pending = Pending::Brainstorm {
            pair: chat.pair(),
            prompt: prompt.clone(),
            retried: false,
        };
        let request = chat.request(&prompt);
        match chat.append(Role::Author, &prompt) {
            Ok(_) => {
                if self.request_ai(request, pending, cx) {
                    self.ai.brainstorm_input.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                    });
                }
            }
            Err(error) => {
                self.ai
                    .messages
                    .insert(Activity::Brainstorm, error.to_string());
                cx.notify();
            }
        }
    }

    fn open_chat_message(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(message) = self
            .ai
            .brainstorm
            .as_ref()
            .and_then(|chat| chat.messages().iter().find(|msg| msg.id == id))
        else {
            return;
        };
        self.ai.brainstorm_message = Some(id);
        self.ai.answer.update(cx, |answer, cx| {
            answer.set_value(message.text.clone(), window, cx)
        });
        cx.notify();
    }

    fn mark_idea(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((chat, message)) = self.ai.brainstorm.as_mut().zip(self.ai.brainstorm_message)
        else {
            return;
        };
        let range = self.ai.answer.read(cx).selected_range();
        match chat.mark(message, range) {
            Ok(id) => self.edit_idea_promotion(Some(id), window, cx),
            Err(error) => {
                self.ai
                    .messages
                    .insert(Activity::Brainstorm, error.to_string());
            }
        }
        cx.notify();
    }

    fn edit_idea_promotion(
        &mut self,
        idea: Option<Id>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai.idea = idea;
        self.ai.links.clear();
        self.ai
            .note
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn promote_idea(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let Some((chat, idea)) = self.ai.brainstorm.as_mut().zip(self.ai.idea) else {
            return;
        };
        let Some(source) = chat.ideas().iter().find(|entry| entry.id == idea) else {
            return;
        };
        let title = self.ai.note.read(cx).value();
        let title = if title.trim().is_empty() {
            source.quote.lines().next().unwrap_or_default()
        } else {
            title.trim()
        };
        match chat.promote(
            IdeaPart {
                idea,
                title: title.into(),
                body: source.quote.clone(),
            },
            &self.ai.links,
        ) {
            Ok(id) => {
                let result = self.refresh_project();
                if self.report(result, cx) {
                    self.navigate(Page::Timeline, window, cx);
                    self.select_event(id, window, cx);
                }
            }
            Err(error) => {
                self.ai
                    .messages
                    .insert(Activity::Brainstorm, error.to_string());
            }
        }
        cx.notify();
    }

    fn ask_research(&mut self, cx: &mut Context<Self>) {
        let question = self.ai.research_input.read(cx).value().trim().to_owned();
        if question.is_empty() || self.ai.pending.is_some() || !self.flush(cx) {
            return;
        }
        let refresh = self.refresh_project();
        if !self.report(refresh, cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let mut all = VecDeque::from(citation_batches(project));
        let Some(current) = all.pop_front() else {
            self.ai.messages.insert(
                Activity::Research,
                "Das Buch enthält noch keine Quellen.".into(),
            );
            cx.notify();
            return;
        };
        self.ai.answers.clear();
        let run = ResearchRun {
            question,
            at: self.ai.at,
            rest: all,
            current,
        };
        let request = research_request(project, &run);
        self.request_ai(request, Pending::Research(run), cx);
    }

    fn continue_research(&mut self, mut run: ResearchRun, text: &str, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        let Some(answers) = self.verified_answers(text, &run.current) else {
            self.ai.messages.insert(
                Activity::Research,
                "Recherche unvollständig. Codex hat keine prüfbaren Quellenangaben geliefert."
                    .into(),
            );
            return;
        };
        self.ai.answers.extend(answers);
        if let Some(batch) = run.rest.pop_front() {
            run.current = batch;
            let request = research_request(project, &run);
            self.request_ai(request, Pending::Research(run), cx);
            return;
        }
        if self.ai.answers.len() > 1 {
            let mut citations: Vec<Citation> = Vec::new();
            for answer in &self.ai.answers {
                for source in &answer.citations {
                    if !citations.iter().any(|item| item.anchor == source.anchor) {
                        citations.push(source.clone());
                    }
                }
            }
            let timeline = Timeline::from_project(project);
            let context = citation_text(&citations, Some((&timeline, run.at)));
            if context.len() <= 25_000 {
                let prompt = format!(
                    "Fasse die geprüften Textquellen zu einer Antwort zusammen. Halte offene oder unbelegte Aussagen offen. Unterscheide insbesondere Figurenwissen, Planung und Prosa. Wenn mehrere zeitlich ungeordnete Treffer die Frage nach der letzten Begegnung offenlassen, benenne das. Gib eine JSON-Liste mit statement und sources (Array von Quellennummern) aus. Frage: {}",
                    truncate(&run.question, 3000)
                );
                self.request_ai(
                    AiRequest::new(prompt).with_context(context),
                    Pending::ResearchSummary(citations),
                    cx,
                );
                return;
            }
            self.ai.messages.insert(Activity::Research, "Die belegten Einzelergebnisse überschreiten den Kontextrahmen für eine gemeinsame Antwort.".into());
            return;
        }
        self.ai.messages.insert(
            Activity::Research,
            if self.ai.answers.is_empty() {
                "Keine belegte Antwort gefunden."
            } else {
                "Recherche abgeschlossen."
            }
            .into(),
        );
    }

    fn verified_answers(&self, text: &str, citations: &[Citation]) -> Option<Vec<Answer>> {
        let live = Sources::build(self.project.as_ref()?);
        Some(
            read_model::<Vec<ModelAnswer>>(text)?
                .into_iter()
                .filter_map(|answer| {
                    let references: Option<Vec<_>> = answer
                        .sources
                        .iter()
                        .map(|&index| {
                            let source = citations.get(index)?;
                            matches!(live.resolve(&source.anchor), Resolution::Unique { .. })
                                .then(|| source.clone())
                        })
                        .collect();
                    let citations = references.filter(|items| !items.is_empty())?;
                    Some(Answer {
                        statement: answer.statement,
                        citations,
                    })
                })
                .collect(),
        )
    }

    fn start_consistency(&mut self, cx: &mut Context<Self>) {
        if self.ai.pending.is_some() || !self.flush(cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let mut batches: Vec<Vec<_>> = citation_batches(project)
            .into_iter()
            .map(|batch| {
                batch
                    .into_iter()
                    .filter(|entry| entry.evidence.scenario != Scenario::OpenIdea)
                    .collect()
            })
            .filter(|batch: &Vec<_>| !batch.is_empty())
            .collect();
        let review = Review::from_project(project);
        let mut meta = Vec::new();
        for item in review.metadata_sources() {
            let source = &item.source;
            let title = project
                .document(source.document)
                .map_or("Ereignis", |doc| doc.title());
            let text = SceneText::new(source.document, source.quote.clone());
            if let Ok(anchor) = text.anchor(0..source.quote.len()) {
                meta.push(Citation {
                    evidence: Evidence {
                        document: source.document,
                        event: Some(source.document),
                        scenario: Scenario::Planning,
                    },
                    title: title.into(),
                    anchor,
                    metadata: true,
                    holder: None,
                });
            }
        }
        batches.extend(meta.chunks(3).map(<[Citation]>::to_vec));
        let Some(current) = batches.first().cloned() else {
            self.ai.messages.insert(
                Activity::Consistency,
                "Das Buch enthält noch keine prüfbaren Aussagen.".into(),
            );
            cx.notify();
            return;
        };
        self.ai.findings.clear();
        self.ai.selected_finding = None;
        self.ai.explaining_finding = None;
        let run = ConsistencyRun {
            batches,
            left: 0,
            right: 0,
            current,
        };
        self.request_ai(consistency_request(&run), Pending::Consistency(run), cx);
    }

    fn continue_consistency(
        &mut self,
        mut run: ConsistencyRun,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.project else { return };
        let sources = Sources::build(project);
        let review = Review::from_project(project);
        let Some(findings) = read_model::<Vec<ModelFinding>>(text) else {
            self.ai.messages.insert(
                Activity::Consistency,
                "Konsistenzprüfung unvollständig. Die Fundstellen konnten nicht gelesen werden."
                    .into(),
            );
            return;
        };
        for model in findings {
            let related: Option<Vec<_>> = model
                .sources
                .iter()
                .map(|&index| {
                    let source = run.current.get(index)?;
                    let finding = FindingSource {
                        document: source.evidence.document,
                        quote: source.anchor.quote().to_owned(),
                    };
                    let available = if source.metadata {
                        review.locate(&finding).is_some()
                    } else {
                        matches!(sources.resolve(&source.anchor), Resolution::Unique { .. })
                    };
                    available.then_some(finding)
                })
                .collect();
            if let Some(sources) =
                related.filter(|sources| sources.len() == 2 && sources[0] != sources[1])
                && !self.ai.findings.iter().any(|finding| {
                    finding.sources == sources || finding.sources.iter().rev().eq(&sources)
                })
            {
                self.ai.findings.push(Finding {
                    title: model.title,
                    sources,
                });
            }
        }
        if run.advance() {
            self.request_ai(consistency_request(&run), Pending::Consistency(run), cx);
        } else {
            self.ai.messages.insert(
                Activity::Consistency,
                if self.ai.findings.is_empty() {
                    "Keinen belegten Widerspruch gefunden."
                } else {
                    "Konsistenzprüfung abgeschlossen."
                }
                .into(),
            );
        }
    }

    fn dismiss_finding(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let Some(finding) = self.ai.findings.get(index) else {
            return;
        };
        let Some((project, store)) = self.project.as_ref().zip(self.store.as_mut()) else {
            return;
        };
        let reason = self
            .ai
            .consistency_reason
            .read(cx)
            .value()
            .trim()
            .to_owned();
        let result = Review::from_project(project)
            .dismiss(finding, (!reason.is_empty()).then_some(reason), store)
            .map(|_| ());
        if self.report(result, cx) && self.flush(cx) {
            self.ai.selected_finding = None;
            self.ai.explaining_finding = None;
            cx.notify();
        }
    }

    pub(super) fn scene_opened(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai.choosing_draft_target = false;
        if let Some((id, text)) = self.selected_scene.zip(self.scene_text.as_ref()) {
            self.ai.details.open(id, scene_revision(text));
            self.ai.draft = self
                .project
                .as_ref()
                .and_then(|project| Draft::history(project, id).pop().map(|turn| turn.draft));
            if let Some(draft) = &self.ai.draft {
                self.ai.answer.update(cx, |input, cx| {
                    input.set_value(draft.text().to_owned(), window, cx)
                });
            }
        }
    }

    pub(super) fn scene_left(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.detail_for_scene(Trigger::Leave, cx);
    }

    fn detail_for_scene(&mut self, trigger: Trigger, cx: &mut Context<Self>) {
        if !matches!(self.ai.session.status(), AiStatus::Ready) {
            return;
        }
        let Some((id, text)) = self.selected_scene.zip(self.scene_text.as_ref()) else {
            return;
        };
        if !self
            .ai
            .details
            .should_request(trigger, id, scene_revision(text))
        {
            return;
        }
        let snapshot = SceneText::from_markdown(id, &text.markdown());
        if self.ai.pending.is_some() {
            self.ai.delayed_details.insert(id, snapshot);
        } else {
            self.send_detail(id, snapshot, cx);
        }
    }

    fn send_detail(&mut self, scene: Id, snapshot: SceneText, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        let mut context = format!(
            "Szene:\n{}\n\nMögliche Zielnotizen:\n",
            truncate(snapshot.text(), 15_000)
        );
        for document in project.documents().filter(|doc| world_kind(doc.kind())) {
            context.push_str(&format!(
                "{} {}\n",
                document.id(),
                truncate(document.title(), 160)
            ));
            if context.len() > 24_000 {
                break;
            }
        }
        let request = AiRequest::new("Finde belegte Ergänzungen für bestehende Zielnotizen. Antworte nur als JSON-Liste mit quote (wörtliches Zitat aus der Szene), target (UUID einer Zielnotiz) und text (vorgeschlagene Ergänzung). Verwende [] ohne passende Ergänzung.")
            .with_context(context);
        self.request_ai(request, Pending::Detail { scene, snapshot }, cx);
    }

    fn accept_detail(&mut self, scene: Id, id: Id, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let refresh = self.refresh_project();
        if !self.report(refresh, cx) {
            return;
        }
        let Some(suggestion) = self
            .ai
            .details
            .suggestions(scene)
            .iter()
            .find(|suggestion| suggestion.id == id)
        else {
            return;
        };
        let Some((project, store)) = self.project.as_ref().zip(self.store.as_mut()) else {
            return;
        };
        if !matches!(
            Sources::build(project).resolve(&suggestion.source),
            Resolution::Unique { .. }
        ) {
            self.ai.messages.insert(
                Activity::Scene,
                "Die belegende Textstelle wurde geändert.".into(),
            );
            cx.notify();
            return;
        }
        let Some(target) = project.document(suggestion.target) else {
            return;
        };
        let edit = self.ai.excerpt.read(cx).value();
        if edit.trim().is_empty() {
            return;
        }
        let updated = format!("{}\n\n{}", target.body().trim_end(), edit.trim());
        let result = store
            .relative_path(target)
            .and_then(|path| store.stage_body(&path, target, &updated));
        if self.report(result, cx) && self.flush(cx) {
            self.ai.details.discard(scene, id);
            self.ai.editing_suggestion = None;
        }
        cx.notify();
    }

    fn request_draft(&mut self, revise: bool, cx: &mut Context<Self>) {
        if self.ai.pending.is_some() || !self.flush(cx) {
            return;
        }
        let Some((scene, project)) = self.scene_text.as_ref().zip(self.project.as_ref()) else {
            return;
        };
        let anchor = if revise {
            self.ai.draft.as_ref().map(|draft| draft.target().clone())
        } else {
            scene
                .anchor(self.editor.read(cx).selected_range())
                .ok()
                .filter(|anchor| !anchor.quote().trim().is_empty())
        };
        let Some(anchor) = anchor else {
            self.ai.messages.insert(
                Activity::Scene,
                "Markiere die auszuformulierenden Stichpunkte.".into(),
            );
            cx.notify();
            return;
        };
        let user = self.input_prompt(cx);
        let outline = Outline::from_project(project);
        let knowledge = writer::knowledge::Knowledge::from_project(project);
        let timeline = Timeline::from_project(project);
        let mut context = format!(
            "Auswahl:\n{}\n\nGanze Szene:\n{}\n",
            truncate(anchor.quote(), 4000),
            truncate(scene.text(), 10_000)
        );
        if let Some(scene_info) = outline.scene(anchor.document_id())
            && let Some(at) = scene_info.event
        {
            context.push_str("\nFigurenwissen zu diesem Zeitpunkt:\n");
            for &character in &scene_info.characters {
                for entry in knowledge.known_by(character, at, &timeline) {
                    if context.len() > 19_000 {
                        break;
                    }
                    context.push_str(truncate(&entry.body, 1000));
                    context.push('\n');
                }
            }
        }
        context.push_str("\nStilbeispiele, nur sprachliche Orientierung:\n");
        for other in project
            .scenes()
            .filter(|item| item.id() != anchor.document_id())
            .take(3)
        {
            context.push_str(truncate(other.body(), 1200));
            context.push('\n');
        }
        let thread = self
            .ai
            .draft_thread
            .as_ref()
            .filter(|(scene, _)| *scene == anchor.document_id());
        if revise && thread.is_none() {
            context.push_str("\nBisheriges Gespräch:\n");
            for turn in Draft::history(project, anchor.document_id())
                .iter()
                .rev()
                .take(3)
                .rev()
            {
                context.push_str(truncate(&turn.prompt, 300));
                context.push('\n');
                context.push_str(truncate(turn.draft.text(), 900));
                context.push('\n');
            }
        }
        let instruction = "Formuliere die Auswahl als Prosa aus. Beachte Perspektive und verfügbares Figurenwissen. Stelle bei einer entscheidenden Wissenslücke eine kurze Rückfrage. Lass unwichtige Fragen offen. Gib nur den Entwurf oder die Rückfrage aus.";
        let mut request = AiRequest::new(format!("{instruction}\n\n{}", truncate(&user, 1800)))
            .with_context(context)
            .with_thread_history();
        if revise && let Some((_, thread_id)) = thread {
            request = request.resume(thread_id);
        }
        self.request_ai(
            request,
            Pending::Draft {
                anchor,
                prompt: user,
            },
            cx,
        );
    }

    fn choose_draft_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ai.choosing_draft_target = true;
        self.editor.update(cx, |editor, cx| {
            let cursor = editor.selected_range().end;
            editor.set_selected_range(cursor..cursor, cx);
            editor.focus(window, cx);
        });
        cx.notify();
    }

    fn accept_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let Some((draft, scene)) = self.ai.draft.as_ref().zip(self.scene_text.as_mut()) else {
            return;
        };
        let answer = self.ai.answer.read(cx).selected_text().to_string();
        let result = if self.ai.choosing_draft_target {
            let selected = self.editor.read(cx).selected_range();
            if selected.is_empty() {
                self.ai.messages.insert(
                    Activity::Scene,
                    "Markiere die neue Zielpassage im Szenentext.".into(),
                );
                cx.notify();
                return;
            }
            scene
                .anchor(selected)
                .map_err(Into::into)
                .and_then(|target| draft.accept_at(&answer, &target, scene))
        } else {
            draft.accept(&answer, scene)
        };
        match result {
            Ok(()) => {
                let text = scene.text().to_owned();
                self.ai.choosing_draft_target = false;
                self.ai.messages.remove(&Activity::Scene);
                self.editor
                    .update(cx, |editor, cx| editor.replace_all(text, window, cx));
                self.stage_editor(window, cx);
            }
            Err(error) => {
                self.ai.messages.insert(Activity::Scene, error.to_string());
                cx.notify();
            }
        }
    }
}

fn world_kind(kind: DocumentKind) -> bool {
    matches!(
        kind,
        DocumentKind::Character | DocumentKind::Place | DocumentKind::Concept | DocumentKind::Idea
    )
}

fn scene_revision(text: &SceneText) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    text.markdown().hash(&mut hasher);
    hasher.finish()
}

fn unique_quote(text: &str, quote: &str) -> Option<Range<usize>> {
    if quote.trim().is_empty() {
        return None;
    }
    let mut places = text.match_indices(quote);
    let (index, _) = places.next()?;
    places
        .next()
        .is_none()
        .then_some(index..index + quote.len())
}

fn truncate(text: &str, bytes: usize) -> &str {
    &text[..text.floor_char_boundary(bytes.min(text.len()))]
}

fn read_model<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    let text = text.trim();
    let json = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .and_then(|text| text.strip_suffix("```"))
        .unwrap_or(text)
        .trim();
    serde_json::from_str(json).ok()
}

fn citation_batches(project: &Project) -> Vec<Vec<Citation>> {
    use writer::{
        knowledge::{ClaimHolder, Knowledge},
        sources::SourceForm,
    };
    let knowledge = Knowledge::from_project(project);
    let sources = Sources::build(project);
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut bytes = 0;
    for source in sources
        .entries()
        .iter()
        .filter(|source| source.kind != DocumentKind::DraftConversation)
    {
        let claim = knowledge
            .entry(source.document)
            .and_then(|entry| entry.claim.as_ref());
        let scenario = match source.kind {
            DocumentKind::Brainstorm | DocumentKind::Idea => Scenario::OpenIdea,
            DocumentKind::Event | DocumentKind::Chapter => Scenario::Planning,
            DocumentKind::Scene if source.concept => Scenario::Concept,
            DocumentKind::Scene if source.form == SourceForm::Bullet => Scenario::Planning,
            DocumentKind::Scene => Scenario::Prose,
            _ if claim.is_some_and(|claim| matches!(claim.holder, ClaimHolder::Character(_))) => {
                Scenario::CharacterKnowledge
            }
            _ => Scenario::AuthorKnowledge,
        };
        let event = claim.and_then(|claim| claim.from).or(source.event);
        let size = source.anchor.quote().len() + 280;
        if !current.is_empty() && bytes + size > 8500 {
            batches.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes += size;
        current.push(Citation {
            evidence: Evidence {
                document: source.document,
                event,
                scenario,
            },
            title: source.title.clone(),
            anchor: source.anchor.clone(),
            metadata: false,
            holder: claim.and_then(|claim| match claim.holder {
                ClaimHolder::Character(id) => project.document(id).map(|doc| {
                    format!(
                        "{} · {}",
                        doc.title(),
                        match claim.state {
                            writer::knowledge::ClaimState::Knows => "weiß",
                            writer::knowledge::ClaimState::Believes => "glaubt",
                        }
                    )
                }),
                ClaimHolder::Author => None,
            }),
        });
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn citation_text(sources: &[Citation], chronology: Option<(&Timeline, Option<Id>)>) -> String {
    use std::fmt::Write;
    let mut context = String::new();
    for (index, source) in sources.iter().enumerate() {
        let timing = chronology.map(|(timeline, at)| source.evidence.scope(timeline, at));
        let _ = writeln!(
            context,
            "{index}: {} | {} | {} | {}\n{}\n",
            truncate(&source.title, 160),
            scenario_label(source.evidence.scenario),
            timing.map_or("", scope_label),
            source.holder.as_deref().unwrap_or_default(),
            if source.metadata {
                truncate(source.anchor.quote(), 2200)
            } else {
                source.anchor.quote()
            }
        );
    }
    if let Some((timeline, _)) = chronology {
        use writer::timeline::TemporalOrder;
        let _ = writeln!(context, "Belegte zeitliche Reihenfolge zwischen Quellen:");
        for (i, earlier) in sources.iter().enumerate() {
            for (j, later) in sources.iter().enumerate() {
                if earlier
                    .evidence
                    .event
                    .zip(later.evidence.event)
                    .is_some_and(|(a, b)| timeline.compare(a, b) == TemporalOrder::Before)
                {
                    let _ = writeln!(context, "{i} liegt vor {j}");
                }
                if context.len() >= 26_000 {
                    let _ = writeln!(context, "Weitere Reihenfolgevergleiche wurden gekürzt.");
                    return context;
                }
            }
        }
    }
    context
}

fn research_request(project: &Project, run: &ResearchRun) -> AiRequest {
    let timeline = Timeline::from_project(project);
    AiRequest::new(format!("Beantworte die Buchfrage ausschließlich anhand der nummerierten Quellen. Trenne Wissen der Figuren, Autorenwissen, Planung, Konzepte und ausgeschriebene Prosa. Kennzeichne Vorschläge als offen. Chronologisch ungeordnete Treffen bleiben offen. Antworte nur als JSON-Liste mit statement (Antworttext) und sources (Liste der Quellennummern). Verwende [] ohne Beleg. Frage: {}", truncate(&run.question, 3500)))
        .with_context(citation_text(&run.current, Some((&timeline, run.at))))
}

fn consistency_request(run: &ConsistencyRun) -> AiRequest {
    AiRequest::new("Prüfe die nummerierten Aussagen auf inhaltliche Widersprüche. Berücksichtige Planung, Figurenperspektive und Schreibstatus. Gib nur eine JSON-Liste mit title und sources (je zwei verschiedene Quellennummern) aus. Verwende [] ohne begründete Funde.")
        .with_context(citation_text(&run.current, None))
}

fn scenario_label(scenario: Scenario) -> &'static str {
    match scenario {
        Scenario::Prose => "Prosa",
        Scenario::Concept => "Konzept",
        Scenario::Planning => "Planung",
        Scenario::OpenIdea => "Noch offen",
        Scenario::CharacterKnowledge => "Figurenwissen",
        Scenario::AuthorKnowledge => "Autorenwissen",
    }
}

fn scope_label(scope: TemporalScope) -> &'static str {
    match scope {
        TemporalScope::AtOrBefore => "bis zum Bezugspunkt",
        TemporalScope::After => "später",
        TemporalScope::Unordered => "zeitlich offen",
    }
}

fn setting_value(label: &'static str, value: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap_1()
        .child(div().font_medium().child(label))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(value.into()),
        )
}

fn brainstorm_event(event: &Event, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius)
        .bg(cx.theme().muted)
        .p_3()
        .child(div().font_semibold().child(event.title.clone()))
        .when_some(event.time.clone(), |card, time| {
            card.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(time),
            )
        })
        .when(!event.body.trim().is_empty(), |card| {
            card.child(truncate(event.body.trim(), 240).to_owned())
        })
}

impl ProjectView {
    fn request_button(&self, button: Button) -> Button {
        button.disabled(
            self.ai.pending.is_some() || !matches!(self.ai.session.status(), AiStatus::Ready),
        )
    }

    pub(super) fn settings_page(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let saving = self.store.as_ref().is_some_and(Store::is_pending);
        let conflict = self.store.as_ref().and_then(Store::conflict).is_some();
        let files = if conflict {
            "Dateikonflikt"
        } else if saving {
            "Dateiänderungen ausstehend"
        } else {
            "Auf Festplatte gesichert"
        };
        div()
            .flex()
            .flex_col()
            .gap_5()
            .max_w(rems(48.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_xl().font_semibold().child("Projekteinstellungen"))
                    .child(
                        Button::new("close-settings").ghost().label("Schließen").on_click(
                            cx.listener(|view, _, window, cx| {
                                view.close_project_settings(window, cx);
                            }),
                        ),
                    ),
            )
            .child(
                div().flex().flex_col().gap_3()
                    .child(div().text_lg().font_semibold().child("KI-Unterstützung"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(setting_value("KI für dieses Projekt", "Mit ChatGPT über Codex verbinden", cx))
                            .child(
                                Switch::new("enable-codex")
                                    .accessibility_label("KI für dieses Projekt")
                                    .checked(self.ai.enabled)
                                    .on_change(cx.listener(|view, enabled, window, cx| {
                                        view.set_ai(*enabled, window, cx);
                                    })),
                            ),
                    )
                    .child(self.connection_settings(cx))
                    .child(
                        div().text_sm().text_color(cx.theme().muted_foreground)
                            .child("Beim Verlassen einer geänderten Szene kann die App Detailvorschläge anfordern."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .pt_4()
                    .child(div().text_lg().font_semibold().child("Projektdateien und Versionen"))
                    .child(setting_value("Projektordner", project.root().display().to_string(), cx))
                    .child(setting_value("Automatische Dateisicherung", files, cx))
                    .child(setting_value(
                        "Letzte lokale Git-Version",
                        self.last_version.as_deref().unwrap_or("Keine Version angezeigt"),
                        cx,
                    )),
            )
    }

    fn connection_settings(&self, cx: &mut Context<Self>) -> Div {
        let status = match self.ai.session.status() {
            AiStatus::Disabled => "Aus".to_owned(),
            AiStatus::Connecting => "Codex verbindet sich …".into(),
            AiStatus::SignInRequired => "Anmeldung bei ChatGPT erforderlich".into(),
            AiStatus::SigningIn => "Warte auf ChatGPT-Anmeldung …".into(),
            AiStatus::Ready => "Mit ChatGPT verbunden".into(),
            AiStatus::Unavailable(error) => error.to_string(),
        };
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(status.clone()),
            )
            .when(self.ai.enabled, |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(Button::new("cli").label("Codex-Datei wählen …").on_click(
                            cx.listener(|view, _, window, cx| view.choose_codex(window, cx)),
                        ))
                        .when(
                            matches!(self.ai.session.status(), AiStatus::Unavailable(_)),
                            |row| {
                                row.child(
                                    Button::new("retry-connection")
                                        .label("Erneut verbinden")
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.connect_ai(window, cx)
                                        })),
                                )
                            },
                        ),
                )
            })
            .when(
                matches!(self.ai.session.status(), AiStatus::SignInRequired),
                |panel| {
                    panel.child(
                        Button::new("login")
                            .primary()
                            .label("Mit ChatGPT verbinden")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.ai.messages.remove(&Activity::Connection);
                                if let Err(error) = view.ai.session.login() {
                                    view.ai
                                        .messages
                                        .insert(Activity::Connection, error.to_string());
                                }
                                cx.notify();
                            })),
                    )
                },
            )
            .when(
                matches!(self.ai.session.status(), AiStatus::SigningIn),
                |panel| {
                    panel.child(
                        Button::new("cancel-login")
                            .label("Anmeldung abbrechen")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.ai.messages.remove(&Activity::Connection);
                                if let Err(error) = view.ai.session.cancel_login() {
                                    view.ai
                                        .messages
                                        .insert(Activity::Connection, error.to_string());
                                }
                                view.ai.login = None;
                                cx.notify();
                            })),
                    )
                },
            )
            .when_some(self.ai.login.clone(), |panel, url| {
                panel.child(
                    Button::new("login-browser")
                        .primary()
                        .label("ChatGPT-Anmeldung im Browser öffnen")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
            .when_some(
                self.ai
                    .messages
                    .get(&Activity::Connection)
                    .filter(|message| **message != status),
                |panel, message| panel.child(div().text_sm().child(message.clone())),
            )
    }

    fn ai_setup_action(&self, cx: &mut Context<Self>) -> Option<Button> {
        if matches!(self.ai.session.status(), AiStatus::Ready) {
            return None;
        }
        Some(
            Button::new("ai-settings")
                .outline()
                .label(if self.ai.enabled {
                    "KI-Verbindung einrichten …"
                } else {
                    "KI einschalten …"
                })
                .on_click(
                    cx.listener(|view, _, window, cx| view.open_project_settings(window, cx)),
                ),
        )
    }

    pub(super) fn research_page(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child("Frage zur Geschichte"),
            )
            .child(Textarea::new(&self.ai.research_input).aria_label("Frage zur Geschichte"))
            .children(self.ai_setup_action(cx))
            .child(self.ai_activity(Activity::Research, cx))
            .child(self.research_view(project, cx))
    }

    fn ai_activity(&self, activity: Activity, cx: &mut Context<Self>) -> Div {
        let running = self
            .ai
            .pending
            .as_ref()
            .map(|(_, pending)| pending.activity());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when_some(self.ai.messages.get(&activity), |panel, message| {
                panel.child(message.clone())
            })
            .when(
                running.is_some_and(|running| running != activity),
                |panel| {
                    panel.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Eine andere KI-Anfrage läuft."),
                    )
                },
            )
            .when(running == Some(activity), |panel| {
                panel.child(
                    Button::new("interrupt")
                        .label("Anfrage abbrechen")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if let Err(error) = view.ai.session.interrupt() {
                                view.ai.messages.insert(activity, error.to_string());
                            }
                            view.ai.delayed_details.clear();
                            cx.notify();
                        })),
                )
            })
            .when(
                running == Some(activity) && !self.ai.streaming.is_empty(),
                |panel| panel.child(self.ai.streaming.clone()),
            )
    }

    fn research_view(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let timeline = Timeline::from_project(project);
        let event = self
            .ai
            .at
            .and_then(|id| timeline.event(id))
            .map_or("Keine feste Zeit", |event| event.title.as_str());
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(format!("Zeitbezug · {event}"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(timeline.ordered_events().iter().map(|event| {
                        let id = event.id;
                        Button::new(format!("research-at-{id}"))
                            .label(event.title.clone())
                            .when(self.ai.at == Some(id), |button| button.primary())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.ai.at = (view.ai.at != Some(id)).then_some(id);
                                cx.notify();
                            }))
                    })),
            )
            .child(
                self.request_button(Button::new("ask").primary().label("Quellen durchsuchen"))
                    .on_click(cx.listener(|view, _, _, cx| view.ask_research(cx))),
            )
            .children(self.ai.answers.iter().enumerate().map(|(index, answer)| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(answer.statement.clone())
                    .children(
                        answer
                            .citations
                            .iter()
                            .enumerate()
                            .map(|(source_index, source)| {
                                let scenario = scenario_label(source.evidence.scenario);
                                let timing =
                                    scope_label(source.evidence.scope(&timeline, self.ai.at));
                                self.source_button(
                                    index * 1_000 + source_index,
                                    &format!("{} · {scenario} · {timing}", source.title),
                                    &source.anchor,
                                    cx,
                                )
                            }),
                    )
            }))
    }

    pub(super) fn wide_brainstorm(window: &Window) -> bool {
        window.viewport_size().width >= window.rem_size() * 64.
    }

    pub(super) fn assistance_view(
        &self,
        project: &Project,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let timeline = Timeline::from_project(project);
        let first = self.ai.pair_seed.and_then(|id| timeline.event(id));
        let second = self.ai.brainstorm.as_ref().and_then(|chat| {
            chat.pair()
                .into_iter()
                .find(|id| Some(*id) != self.ai.pair_seed)
                .and_then(|id| timeline.event(id))
        });
        let mut conversation = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_64()
            .gap_3()
            .child(div().text_lg().font_semibold().child("Gespräch"))
            .child(self.ai_activity(Activity::Brainstorm, cx));
        if let Some(chat) = &self.ai.brainstorm {
            conversation = conversation
                .children(chat.messages().iter().map(|message| {
                    let id = message.id;
                    let role = match message.role {
                        Role::Author => "Ich",
                        Role::Codex => "Codex",
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .pt_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(role),
                        )
                        .child(message.text.clone())
                        .when(message.role == Role::Codex, |row| {
                            row.child(
                                Button::new(format!("message-{id}"))
                                    .ghost()
                                    .label("Textstelle auswählen")
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.open_chat_message(id, window, cx)
                                    })),
                            )
                        })
                }))
                .when(self.ai.brainstorm_message.is_some(), |panel| {
                    panel
                        .child(
                            Textarea::new(&self.ai.answer)
                                .readonly(true)
                                .aria_label("Antwort von Codex"),
                        )
                        .child(
                            Button::new("mark-idea")
                                .label("Auswahl als Idee markieren")
                                .on_click(
                                    cx.listener(|view, _, window, cx| view.mark_idea(window, cx)),
                                ),
                        )
                })
                .children(chat.ideas().iter().map(|idea| {
                    let id = idea.id;
                    Button::new(format!("idea-{id}"))
                        .label(format!("Noch offen · {}", truncate(&idea.quote, 200)))
                        .when(self.ai.idea == Some(id), |button| button.primary())
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.edit_idea_promotion(Some(id), window, cx);
                        }))
                }))
                .when_some(
                    self.ai
                        .idea
                        .and_then(|id| chat.ideas().iter().find(|idea| idea.id == id)),
                    |panel, idea| panel.child(self.promote_view(&timeline, &idea.quote, cx)),
                )
                .child(Textarea::new(&self.ai.brainstorm_input).aria_label("Gesprächsbeitrag"))
                .child(
                    self.request_button(Button::new("send-chat").primary().label("Beitrag senden"))
                        .on_click(cx.listener(|view, _, window, cx| view.send_chat(window, cx))),
                );
        } else {
            conversation = conversation.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("Wähle zwei Ereignisse für das Gespräch."),
            );
        }
        let events = self.brainstorm_events(&timeline, first, second, cx);
        let conversation = conversation.children(self.ai_setup_action(cx));
        let wide = Self::wide_brainstorm(window)
            && self
                .store
                .as_ref()
                .is_none_or(|store| store.conflict().is_none());
        let workspace = div().flex().min_w_0();
        let workspace = if wide {
            workspace.flex_1().min_h_0().child(
                h_resizable("brainstorm-split")
                    .child(
                        resizable_panel()
                            .size(window.rem_size() * 23.)
                            .size_range(window.rem_size() * 17. ..window.rem_size() * 40.)
                            .child(
                                events
                                    .pr_4()
                                    .min_h_0()
                                    .overflow_y_scrollbar()
                                    .id("brainstorm-events"),
                            ),
                    )
                    .child(
                        resizable_panel().child(
                            conversation
                                .pl_4()
                                .min_h_0()
                                .overflow_y_scrollbar()
                                .id("brainstorm-conversation"),
                        ),
                    ),
            )
        } else {
            workspace
                .flex_col()
                .gap_5()
                .child(events)
                .child(conversation)
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .when(wide, |layout| layout.flex_1().min_h_0())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_xl().font_semibold().child("Lücke erkunden"))
                    .child(
                        Button::new("back-to-timeline")
                            .ghost()
                            .label("Zur Timeline")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.navigate(Page::Timeline, window, cx)
                            })),
                    ),
            )
            .child(workspace)
    }

    fn brainstorm_events(
        &self,
        timeline: &Timeline,
        first: Option<&Event>,
        second: Option<&Event>,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut events = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_64()
            .gap_3()
            .child(div().text_lg().font_semibold().child("Ereignispaar"));
        if let Some((first, second)) = first.zip(second) {
            let (earlier, relation, later) = match timeline.compare(first.id, second.id) {
                TemporalOrder::Before => (first, "↓ Zeitlich davor", second),
                TemporalOrder::After => (second, "↓ Zeitlich davor", first),
                _ => (first, "Zeitliche Reihenfolge offen", second),
            };
            events = events
                .child(brainstorm_event(earlier, cx))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(relation),
                )
                .child(brainstorm_event(later, cx));
        } else if let Some(first) = first {
            events = events.child(brainstorm_event(first, cx));
        }
        let options = timeline.ordered_events();
        events
            .child(div().font_medium().child("Erstes Ereignis"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(options.iter().map(|event| {
                        let id = event.id;
                        Button::new(format!("brainstorm-first-{id}"))
                            .ghost()
                            .label(event.title.clone())
                            .disabled(self.ai.pending.is_some())
                            .when(first.is_some_and(|event| event.id == id), |button| {
                                button.primary()
                            })
                            .on_click(
                                cx.listener(move |view, _, _, cx| view.select_pair_seed(id, cx)),
                            )
                    })),
            )
            .when_some(first, |events, first| {
                events
                    .child(div().font_medium().child("Zweites Ereignis"))
                    .child(
                        div().flex().flex_wrap().gap_1().children(
                            options
                                .into_iter()
                                .filter(|event| event.id != first.id)
                                .map(|event| {
                                    let id = event.id;
                                    Button::new(format!("brainstorm-second-{id}"))
                                        .ghost()
                                        .label(event.title.clone())
                                        .disabled(self.ai.pending.is_some())
                                        .when(
                                            second.is_some_and(|event| event.id == id),
                                            |button| button.primary(),
                                        )
                                        .on_click(
                                            cx.listener(move |view, _, _, cx| {
                                                view.open_pair(id, cx)
                                            }),
                                        )
                                }),
                        ),
                    )
            })
    }

    fn promote_view(&self, timeline: &Timeline, quote: &str, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_lg().font_semibold().child("Idee übernehmen"))
            .child(quote.to_owned())
            .child("Titel des neuen Ereignisses")
            .child(Input::new(&self.ai.note).aria_label("Titel des neuen Ereignisses"))
            .child("Verbindungen für die Übernahme")
            .children(timeline.ordered_events().into_iter().map(|event| {
                let id = event.id;
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(event.title.clone())
                    .children(
                        [
                            (
                                "Vorhandenes liegt vor",
                                LinkDirection::Incoming,
                                Relation::Before,
                            ),
                            ("Neues liegt vor", LinkDirection::Outgoing, Relation::Before),
                            (
                                "Vorhandenes verursacht Neues",
                                LinkDirection::Incoming,
                                Relation::Cause,
                            ),
                            (
                                "Neues verursacht Vorhandenes",
                                LinkDirection::Outgoing,
                                Relation::Cause,
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(choice, (label, direction, kind))| {
                            let selected = self.ai.links.iter().any(|link| {
                                link.event == id && link.direction == direction && link.kind == kind
                            });
                            Button::new(format!("promote-{id}-{choice}"))
                                .label(label)
                                .when(selected, |button| button.primary())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if let Some(index) = view.ai.links.iter().position(|link| {
                                        link.event == id
                                            && link.direction == direction
                                            && link.kind == kind
                                    }) {
                                        view.ai.links.remove(index);
                                    } else {
                                        view.ai.links.push(Link {
                                            event: id,
                                            direction,
                                            kind,
                                            note: String::new(),
                                        });
                                    }
                                    cx.notify();
                                }))
                        }),
                    )
            }))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("promote")
                            .primary()
                            .label("Idee als Ereignis übernehmen")
                            .on_click(
                                cx.listener(|view, _, window, cx| view.promote_idea(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("cancel-idea-promotion")
                            .label("Abbrechen")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.edit_idea_promotion(None, window, cx);
                                window.focus(&view.keyboard_focus, cx);
                            })),
                    ),
            )
    }

    pub(super) fn consistency_page(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let review = Review::from_project(project);
        let findings: Vec<_> = self
            .ai
            .findings
            .iter()
            .enumerate()
            .filter_map(|(index, finding)| {
                let status = review.status(finding);
                (status != ReviewStatus::Dismissed).then_some((index, finding, status))
            })
            .collect();
        let decision = match self.ai.selected_finding {
            Some(FindingSelection::Marked(id)) if self.ai.show_decisions => {
                review.decisions().iter().find(|decision| decision.id == id)
            }
            _ => None,
        };
        let selected = match self.ai.selected_finding {
            Some(FindingSelection::Open(index)) => findings.iter().find(|(i, _, _)| *i == index),
            _ => None,
        }
        .or_else(|| findings.first())
        .filter(|_| decision.is_none());
        let list = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_48()
            .gap_2()
            .child(div().text_lg().font_semibold().child("Offene Prüffunde"))
            .when(findings.is_empty(), |list| {
                list.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child("Keine offenen Prüffunde."),
                )
            })
            .children(findings.iter().map(|&(index, finding, _)| {
                let key = FindingSelection::Open(index);
                Button::new(("finding", index))
                    .ghost()
                    .label(finding.title.clone())
                    .when(
                        selected.is_some_and(|(current, _, _)| *current == index),
                        |button| button.primary(),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| view.select_finding(key, cx)))
            }))
            .child(
                Button::new("marked-findings")
                    .outline()
                    .label(if self.ai.show_decisions {
                        "Vermerkte ausblenden"
                    } else {
                        "Vermerkte anzeigen"
                    })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.ai.show_decisions = !view.ai.show_decisions;
                        if !view.ai.show_decisions
                            && matches!(view.ai.selected_finding, Some(FindingSelection::Marked(_)))
                        {
                            view.ai.selected_finding = None;
                        }
                        cx.notify();
                    })),
            )
            .when(self.ai.show_decisions, |list| {
                list.when(review.decisions().is_empty(), |list| {
                    list.child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("Keine vermerkten Prüffunde."),
                    )
                })
                .children(review.decisions().iter().map(|decision| {
                    let id = decision.id;
                    Button::new(format!("decision-{id}"))
                        .ghost()
                        .label(decision.title.clone())
                        .when(
                            self.ai.selected_finding == Some(FindingSelection::Marked(id)),
                            |button| button.primary(),
                        )
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.select_finding(FindingSelection::Marked(id), cx);
                        }))
                }))
            });

        let details = div()
            .flex()
            .flex_col()
            .gap_3()
            .min_w_64()
            .flex_grow_1()
            .flex_basis(rems(22.))
            .children(selected.map(|&(index, finding, status)| {
                self.finding_detail(project, &review, index, finding, status, cx)
            }))
            .when_some(decision, |pane, decision| {
                pane.child(
                    div()
                        .text_lg()
                        .font_semibold()
                        .child(decision.title.clone()),
                )
                .children(decision.sources.iter().enumerate().map(|(index, source)| {
                    self.consistency_source(project, &review, 10_000 + index, source, cx)
                }))
                .child(setting_value(
                    "Als beabsichtigt vermerkt",
                    decision
                        .reason
                        .as_deref()
                        .unwrap_or("Keine Begründung angegeben")
                        .to_owned(),
                    cx,
                ))
            });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_xl().font_semibold().child("Konsistenzprüfung"))
                    .child(
                        self.request_button(
                            Button::new("check-consistency")
                                .primary()
                                .label("Konsistenz prüfen"),
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.start_consistency(cx))),
                    ),
            )
            .children(self.ai_setup_action(cx))
            .child(self.ai_activity(Activity::Consistency, cx))
            .child(div().flex().flex_wrap().gap_6().child(list).child(details))
    }

    fn select_finding(&mut self, selection: FindingSelection, cx: &mut Context<Self>) {
        self.ai.selected_finding = Some(selection);
        self.ai.explaining_finding = None;
        cx.notify();
    }

    fn finding_detail(
        &self,
        project: &Project,
        review: &Review,
        index: usize,
        finding: &Finding,
        status: ReviewStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_lg().font_semibold().child(finding.title.clone()))
            .children(finding.sources.iter().enumerate().map(|(i, source)| {
                self.consistency_source(project, review, 10_000 + index * 8 + i, source, cx)
            }))
            .when(status == ReviewStatus::Uncertain, |pane| {
                pane.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            "Die Aussagen lassen sich ihren Fundstellen nicht eindeutig zuordnen.",
                        ),
                )
            })
            .when(status == ReviewStatus::Open, |pane| {
                if self.ai.explaining_finding == Some(index) {
                    pane.child("Begründung (optional)")
                        .child(
                            Input::new(&self.ai.consistency_reason)
                                .aria_label("Begründung (optional)"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .justify_end()
                                .child(Button::new("cancel-reason").label("Abbrechen").on_click(
                                    cx.listener(|view, _, _, cx| {
                                        view.ai.explaining_finding = None;
                                        cx.notify();
                                    }),
                                ))
                                .child(
                                    Button::new(("dismiss-finding", index))
                                        .primary()
                                        .label("Vermerken")
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.dismiss_finding(index, cx);
                                        })),
                                ),
                        )
                } else {
                    pane.child(
                        Button::new("open-reason")
                            .label("Als beabsichtigt markieren …")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.ai.explaining_finding = Some(index);
                                view.ai.consistency_reason.update(cx, |reason, cx| {
                                    reason.set_value("", window, cx);
                                    reason.focus(window, cx);
                                });
                                cx.notify();
                            })),
                    )
                }
            })
    }

    fn consistency_source(
        &self,
        project: &Project,
        review: &Review,
        index: usize,
        source: &FindingSource,
        cx: &mut Context<Self>,
    ) -> Div {
        let document = project.document(source.document);
        let location = review.locate(source);
        let scene =
            document.map(|document| SceneText::from_markdown(document.id(), document.body()));
        let quote_range = scene
            .as_ref()
            .and_then(|scene| unique_quote(scene.text(), &source.quote));
        let category = match document.map(writer::Document::kind) {
            Some(DocumentKind::Event) => "Timeline",
            Some(DocumentKind::Chapter) => "Kapitel",
            Some(DocumentKind::Scene)
                if scene
                    .as_ref()
                    .zip(quote_range.as_ref())
                    .is_some_and(|(scene, quote)| {
                        scene.concepts().iter().any(|concept| {
                            quote.start < concept.range.end && concept.range.start < quote.end
                        })
                    }) =>
            {
                "Manuskript · Konzept"
            }
            Some(DocumentKind::Scene) => "Manuskript",
            Some(_) => "Welt",
            None => "Fundstelle fehlt",
        };
        let mut pane = div()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .pt_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(category),
            )
            .child(source.quote.clone());
        if let Some((document, location)) = document.zip(location) {
            let mut title = document.title().to_owned();
            if let SourceSection::RelationNote { to, .. } | SourceSection::RelationTime { to, .. } =
                location.section
                && let Some(target) = project.document(to)
            {
                title = format!("{title} → {}", target.title());
            }
            let id = document.id();
            let kind = document.kind();
            let section = location.section;
            let anchor = quote_range
                .or(location.body_range)
                .and_then(|range| scene.as_ref()?.anchor(range).ok());
            if section != SourceSection::Body || anchor.is_some() || kind == DocumentKind::Chapter {
                pane = pane.child(
                    Button::new(("source", index))
                        .ghost()
                        .label(title)
                        .on_click(cx.listener(move |view, _, window, cx| match section {
                            SourceSection::Body if kind == DocumentKind::Chapter => {
                                view.navigate(Page::Outline, window, cx);
                                if view.page == Page::Outline {
                                    view.select_outline_chapter(id, window, cx);
                                }
                            }
                            SourceSection::Body => {
                                if let Some(anchor) = &anchor {
                                    view.jump_to(anchor, window, cx);
                                }
                            }
                            SourceSection::EventTime => {
                                view.navigate(Page::Timeline, window, cx);
                                if view.page == Page::Timeline {
                                    view.select_event(id, window, cx);
                                    view.event_time
                                        .update(cx, |input, cx| input.focus(window, cx));
                                }
                            }
                            SourceSection::RelationNote { to, relation }
                            | SourceSection::RelationTime { to, relation } => {
                                view.navigate(Page::Timeline, window, cx);
                                if view.page == Page::Timeline {
                                    view.select_relation(id, to, relation, window, cx);
                                    let field = match section {
                                        SourceSection::RelationNote { .. } => &view.relation_note,
                                        _ => &view.relation_time,
                                    };
                                    field.update(cx, |input, cx| input.focus(window, cx));
                                }
                            }
                        })),
                );
            } else {
                pane = pane.child(div().text_sm().child(title));
            }
        } else if let Some(document) = document {
            pane = pane.child(div().text_sm().child(document.title().to_owned()));
        }
        pane
    }

    fn save_draft_note(
        &mut self,
        scene: Id,
        holder: writer::knowledge::ClaimHolder,
        cx: &mut Context<Self>,
    ) {
        use writer::knowledge::{Claim, ClaimState, Knowledge, KnowledgeKind};
        let answer = self.input_prompt(cx);
        if answer.is_empty() || !self.flush(cx) {
            return;
        }
        let Some((project, store)) = self.project.as_ref().zip(self.store.as_mut()) else {
            return;
        };
        let outline = Outline::from_project(project);
        let Some(scene) = outline.scene(scene) else {
            return;
        };
        let mut knowledge = Knowledge::from_project(project);
        knowledge.create(
            KnowledgeKind::Concept,
            &format!("Notiz zu {}", scene.title),
            &answer,
            Some(Claim {
                holder,
                state: ClaimState::Knows,
                from: scene.event.or(scene.timing.after),
            }),
        );
        let result = knowledge.stage_changes(project, store);
        if self.report(result, cx) && self.flush(cx) {
            self.ai.messages.insert(
                Activity::Scene,
                "Antwort in der Weltkartei gesichert.".into(),
            );
            cx.notify();
        }
    }

    fn draft_note_controls(&self, scene: Id, cx: &mut Context<Self>) -> Div {
        use writer::knowledge::ClaimHolder;
        let mut row = div().flex().flex_wrap().gap_2().child(
            Button::new("draft-author-note")
                .label("Antwort als Autorenwissen sichern")
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.save_draft_note(scene, ClaimHolder::Author, cx)
                })),
        );
        if let Some(project) = &self.project {
            let outline = Outline::from_project(project);
            if let Some(entry) = outline.scene(scene) {
                row = row.children(entry.characters.iter().filter_map(|&id| {
                    let character = project.document(id)?;
                    Some(
                        Button::new(format!("draft-note-{id}"))
                            .label(format!("Als Wissen von {} sichern", character.title()))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.save_draft_note(scene, ClaimHolder::Character(id), cx)
                            })),
                    )
                }));
            }
        }
        row
    }

    pub(super) fn scene_assistance(&self, scene: Id, cx: &mut Context<Self>) -> Div {
        let draft_here = self
            .ai
            .draft
            .as_ref()
            .filter(|draft| draft.target().document_id() == scene);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("ask-from-scene")
                            .label("Frage zur Geschichte")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.ai.at = view.project.as_ref().and_then(|project| {
                                    Outline::from_project(project)
                                        .scene(scene)
                                        .and_then(|scene| scene.event)
                                });
                                view.navigate(Page::Research, window, cx);
                            })),
                    )
                    .child(
                        self.request_button(
                            Button::new("start-draft").label("Auswahl ausformulieren"),
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.request_draft(false, cx))),
                    )
                    .child(
                        self.request_button(
                            Button::new("detail-manual").label("Vorschläge anzeigen"),
                        )
                        .on_click(
                            cx.listener(|view, _, _, cx| {
                                view.detail_for_scene(Trigger::Manual, cx)
                            }),
                        ),
                    ),
            )
            .children(self.ai_setup_action(cx))
            .child(self.ai_activity(Activity::Scene, cx))
            .when_some(draft_here, |pane, draft| {
                let target_unavailable = self
                    .scene_text
                    .as_ref()
                    .is_some_and(|text| text.resolve(draft.target()).is_err());
                pane.child("Entwurf")
                    .child(Textarea::new(&self.ai.answer).readonly(true))
                    .child(
                        Button::new("accept-draft")
                            .primary()
                            .label("Markierten Ausschnitt übernehmen")
                            .disabled(target_unavailable || self.ai.choosing_draft_target)
                            .on_click(
                                cx.listener(|view, _, window, cx| view.accept_draft(window, cx)),
                            ),
                    )
                    .when(
                        target_unavailable || self.ai.choosing_draft_target,
                        |pane| {
                            pane.child("Ursprüngliche Zielpassage")
                                .child(draft.target().quote().to_owned())
                                .when(!self.ai.choosing_draft_target, |pane| {
                                    pane.child(
                                        Button::new("choose-draft-target")
                                            .label("Zielpassage neu auswählen")
                                            .on_click(cx.listener(|view, _, window, cx| {
                                                view.choose_draft_target(window, cx)
                                            })),
                                    )
                                })
                                .when(self.ai.choosing_draft_target, |pane| {
                                    pane.child("Markiere die neue Zielpassage im Szenentext.")
                                        .child(
                                            Button::new("accept-draft-at-selection")
                                                .primary()
                                                .label("In neue Auswahl übernehmen")
                                                .on_click(cx.listener(|view, _, window, cx| {
                                                    view.accept_draft(window, cx)
                                                })),
                                        )
                                })
                        },
                    )
                    .child("Antwort auf Rückfrage oder Anweisung zur Überarbeitung")
                    .child(Textarea::new(&self.ai.input))
                    .child(self.draft_note_controls(scene, cx))
                    .child(
                        self.request_button(
                            Button::new("revise-draft").label("Entwurf überarbeiten"),
                        )
                        .on_click(cx.listener(|view, _, _, cx| view.request_draft(true, cx))),
                    )
            })
            .children(self.ai.details.suggestions(scene).iter().map(|suggestion| {
                let id = suggestion.id;
                let proposed = suggestion.text.clone();
                let target = self
                    .project
                    .as_ref()
                    .and_then(|project| project.document(suggestion.target))
                    .map_or("Zielnotiz fehlt", |document| document.title());
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(format!("Vorschlag für {target}"))
                    .child(suggestion.source.quote().to_owned())
                    .child(suggestion.text.clone())
                    .child(
                        Button::new(format!("open-detail-{id}"))
                            .label("Vorschlag bearbeiten")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.ai.editing_suggestion = Some(id);
                                view.ai.excerpt.update(cx, |input, cx| {
                                    input.set_value(proposed.clone(), window, cx)
                                });
                                cx.notify();
                            })),
                    )
                    .when(self.ai.editing_suggestion == Some(id), |pane| {
                        pane.child(Textarea::new(&self.ai.excerpt)).child(
                            Button::new(format!("accept-detail-{id}"))
                                .label("Übernehmen")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.accept_detail(scene, id, cx)
                                })),
                        )
                    })
                    .child(
                        Button::new(format!("drop-detail-{id}"))
                            .label("Verwerfen")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.ai.details.discard(scene, id);
                                cx.notify();
                            })),
                    )
            }))
    }
}

#[cfg(test)]
#[path = "assistance_tests.rs"]
mod tests;
