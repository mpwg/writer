use super::*;
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
        consistency::{Finding, FindingSource, Review, ReviewStatus},
        details::{DetailQueue, Suggestion, Trigger},
        draft::Draft,
        research::{Evidence, Scenario, TemporalScope},
    },
    outline::Outline,
    passage::{Anchor, SceneText},
    sources::{Resolution, Sources},
    timeline::{Relation, Timeline},
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

pub(super) struct Assistance {
    preferences: Option<AiPreferences>,
    session: AiSession,
    enabled: bool,
    cli: Option<PathBuf>,
    poll_task: Option<Task<()>>,
    pending: Option<(RequestId, Pending)>,
    input: Entity<TextareaState>,
    excerpt: Entity<TextareaState>,
    answer: Entity<TextareaState>,
    note: Entity<InputState>,
    pair_seed: Option<Id>,
    brainstorm: Option<Brainstorm>,
    brainstorm_message: Option<Id>,
    idea: Option<Id>,
    links: Vec<Link>,
    login: Option<String>,
    status: Option<String>,
    streaming: String,
    at: Option<Id>,
    answers: Vec<Answer>,
    findings: Vec<Finding>,
    draft: Option<Draft>,
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
            excerpt: cx.new(|cx| TextareaState::new(window, cx).rows(3)),
            answer: cx.new(|cx| TextareaState::new(window, cx).rows(7)),
            note: cx.new(|cx| InputState::new(window, cx)),
            pair_seed: None,
            brainstorm: None,
            brainstorm_message: None,
            idea: None,
            links: Vec::new(),
            login: None,
            status: None,
            streaming: String::new(),
            at: None,
            answers: Vec::new(),
            findings: Vec::new(),
            draft: None,
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
        self.status = None;
        self.answers.clear();
        self.findings.clear();
        self.draft = None;
        self.draft_thread = None;
        for field in [&self.input, &self.answer, &self.excerpt] {
            field.update(cx, |field, cx| field.set_value("", window, cx));
        }
        self.note
            .update(cx, |field, cx| field.set_value("", window, cx));
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
        self.details.set_enabled(false);
        self.delayed_details.clear();
        self.editing_suggestion = None;
    }
}

impl ProjectView {
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
            self.ai.status = Some(error.to_string());
            cx.notify();
            return;
        }
        if enabled {
            self.ai.enabled = true;
            self.ai.details.set_enabled(true);
            self.connect_ai(window, cx);
        } else {
            self.ai.status = None;
        }
        cx.notify();
    }

    fn connect_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ai.enabled {
            return;
        }
        self.ai.poll_task = None;
        self.ai.status = self
            .ai
            .session
            .enable(self.ai.cli.as_deref())
            .err()
            .map(|error| error.to_string());
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
        match self.ai.session.request(request) {
            Ok(id) => {
                self.ai.pending = Some((id, pending));
                self.ai.streaming.clear();
                self.ai.status = None;
                cx.notify();
                true
            }
            Err(error) => {
                self.ai.status = Some(error.to_string());
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
                    self.ai.status = Some(error.to_string());
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
                        self.ai.status = Some(error.to_string());
                        if matches!(
                            pending,
                            Some(Pending::Research(_) | Pending::ResearchSummary(_))
                        ) && !self.ai.answers.is_empty()
                        {
                            self.ai.status = Some(format!("Recherche unvollständig. {error}"));
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
                        Err(error) => self.ai.status = Some(error.to_string()),
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
                    self.ai.status = Some("Recherche abgeschlossen.".into());
                } else {
                    self.ai.status = Some("Die Zusammenfassung ließ sich nicht belegen. Die geprüften Einzelantworten sind unten aufgeführt.".into());
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
            self.ai.status = Some(error.to_string());
            cx.notify();
            return;
        }
        let Some(project) = &self.project else { return };
        match Brainstorm::open(project, pair) {
            Ok(chat) => {
                self.ai.brainstorm = Some(chat);
                self.ai.status = None;
            }
            Err(error) => self.ai.status = Some(error.to_string()),
        }
        cx.notify();
    }

    fn send_chat(&mut self, cx: &mut Context<Self>) {
        if self.ai.pending.is_some() {
            return;
        }
        let prompt = self.input_prompt(cx);
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
                self.request_ai(request, pending, cx);
            }
            Err(error) => {
                self.ai.status = Some(error.to_string());
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

    fn mark_idea(&mut self, cx: &mut Context<Self>) {
        let Some((chat, message)) = self.ai.brainstorm.as_mut().zip(self.ai.brainstorm_message)
        else {
            return;
        };
        let range = self.ai.answer.read(cx).selected_range();
        match chat.mark(message, range) {
            Ok(id) => self.ai.idea = Some(id),
            Err(error) => self.ai.status = Some(error.to_string()),
        }
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
            Err(error) => self.ai.status = Some(error.to_string()),
        }
        cx.notify();
    }

    fn ask_research(&mut self, cx: &mut Context<Self>) {
        let question = self.input_prompt(cx);
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
            self.ai.status = Some("Das Buch enthält noch keine Quellen.".into());
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
            self.ai.status = Some(
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
            self.ai.status = Some("Die belegten Einzelergebnisse überschreiten den Kontextrahmen für eine gemeinsame Antwort.".into());
            return;
        }
        self.ai.status = Some(
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
            return;
        };
        self.ai.findings.clear();
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
            self.ai.status = Some(
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
            self.ai.status = Some(
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
        let reason = self.ai.note.read(cx).value().trim().to_owned();
        let result = Review::from_project(project)
            .dismiss(finding, (!reason.is_empty()).then_some(reason), store)
            .map(|_| ());
        if self.report(result, cx) {
            self.flush(cx);
        }
    }

    pub(super) fn scene_opened(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            self.ai.status = Some("Die belegende Textstelle wurde geändert.".into());
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
            self.ai.status = Some("Markiere die auszuformulierenden Stichpunkte.".into());
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

    fn accept_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let Some((draft, scene)) = self.ai.draft.as_ref().zip(self.scene_text.as_mut()) else {
            return;
        };
        let answer = self.ai.answer.read(cx).selected_text().to_string();
        match draft.accept(&answer, scene) {
            Ok(()) => {
                let text = scene.text().to_owned();
                self.editor
                    .update(cx, |editor, cx| editor.replace_all(text, window, cx));
                self.stage_editor(window, cx);
            }
            Err(error) => {
                self.ai.status = Some(error.to_string());
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

impl ProjectView {
    fn request_button(&self, button: Button) -> Button {
        button.disabled(
            self.ai.pending.is_some() || !matches!(self.ai.session.status(), AiStatus::Ready),
        )
    }

    pub(super) fn assistance_view(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let status = match self.ai.session.status() {
            AiStatus::Disabled => "Aus".to_owned(),
            AiStatus::Connecting => "Codex verbindet sich …".into(),
            AiStatus::SignInRequired => "Anmeldung bei ChatGPT erforderlich".into(),
            AiStatus::SigningIn => "Warte auf ChatGPT-Anmeldung …".into(),
            AiStatus::Ready => "Mit ChatGPT verbunden".into(),
            AiStatus::Unavailable(error) => error.to_string(),
        };
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Codex für dieses Projekt"))
            .child(status)
            .child(
                Button::new("enable-codex")
                    .label(if self.ai.enabled {
                        "KI ausschalten"
                    } else {
                        "KI einschalten"
                    })
                    .on_click(
                        cx.listener(|view, _, window, cx| {
                            view.set_ai(!view.ai.enabled, window, cx)
                        }),
                    ),
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
                                view.ai.status =
                                    view.ai.session.login().err().map(|error| error.to_string());
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
                                view.ai.status = view
                                    .ai
                                    .session
                                    .cancel_login()
                                    .err()
                                    .map(|error| error.to_string());
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
            .child(self.ai_activity(cx))
            .child("Frage, Gesprächsbeitrag oder Anweisung")
            .child(Textarea::new(&self.ai.input))
            .child(self.research_view(project, cx))
            .child(self.brainstorm_view(project, cx))
            .child(self.consistency_view(project, cx));
        if let Some(draft) = &self.ai.draft {
            view = view
                .child(div().text_lg().child("Entwurf aus der Szenenansicht"))
                .child(draft.text().to_owned());
        }
        view
    }

    fn ai_activity(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .when_some(self.ai.status.clone(), |panel, message| {
                panel.child(message)
            })
            .when(self.ai.pending.is_some(), |panel| {
                panel.child(
                    Button::new("interrupt")
                        .label("Anfrage abbrechen")
                        .on_click(cx.listener(|view, _, _, cx| {
                            if let Err(error) = view.ai.session.interrupt() {
                                view.ai.status = Some(error.to_string());
                            }
                            view.ai.delayed_details.clear();
                            cx.notify();
                        })),
                )
            })
            .when(!self.ai.streaming.is_empty(), |panel| {
                panel.child(self.ai.streaming.clone())
            })
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
            .child(div().text_lg().child("Frage zur Geschichte"))
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

    fn brainstorm_view(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let timeline = Timeline::from_project(project);
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_lg().child("Brainstorming am Ereignispaar"))
            .child(
                if let Some(title) = self
                    .ai
                    .pair_seed
                    .and_then(|id| timeline.event(id))
                    .map(|event| event.title.as_str())
                {
                    format!("Erstes Ereignis · {title}")
                } else {
                    "Erstes Ereignis auswählen".into()
                },
            )
            .child(div().flex().flex_wrap().gap_2().children(
                timeline.ordered_events().into_iter().map(|event| {
                    let id = event.id;
                    Button::new(format!("brainstorm-first-{id}"))
                        .label(event.title.clone())
                        .disabled(self.ai.pending.is_some())
                        .when(self.ai.pair_seed == Some(id), |button| button.primary())
                        .on_click(cx.listener(move |view, _, _, cx| view.select_pair_seed(id, cx)))
                }),
            ))
            .when_some(self.ai.pair_seed, |panel, seed| {
                panel.child("Zweites Ereignis").child(
                    div().flex().flex_wrap().gap_2().children(
                        timeline
                            .ordered_events()
                            .into_iter()
                            .filter(|event| event.id != seed)
                            .map(|event| {
                                let id = event.id;
                                Button::new(format!("brainstorm-second-{id}"))
                                    .label(event.title.clone())
                                    .disabled(self.ai.pending.is_some())
                                    .on_click(
                                        cx.listener(move |view, _, _, cx| view.open_pair(id, cx)),
                                    )
                            }),
                    ),
                )
            });
        if let Some(chat) = &self.ai.brainstorm {
            panel = panel
                .child(
                    self.request_button(Button::new("send-chat").primary().label("Beitrag senden"))
                        .on_click(cx.listener(|view, _, _, cx| view.send_chat(cx))),
                )
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
                        .child(div().text_sm().child(role))
                        .child(message.text.clone())
                        .when(message.role == Role::Codex, |row| {
                            row.child(
                                Button::new(format!("message-{id}"))
                                    .label("Textstelle auswählen")
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.open_chat_message(id, window, cx)
                                    })),
                            )
                        })
                }))
                .when(self.ai.brainstorm_message.is_some(), |panel| {
                    panel
                        .child(Textarea::new(&self.ai.answer).readonly(true))
                        .child(
                            Button::new("mark-idea")
                                .label("Auswahl als Idee markieren")
                                .on_click(cx.listener(|view, _, _, cx| view.mark_idea(cx))),
                        )
                })
                .children(chat.ideas().iter().map(|idea| {
                    let id = idea.id;
                    Button::new(format!("idea-{id}"))
                        .label(format!("Noch offen · {}", truncate(&idea.quote, 200)))
                        .when(self.ai.idea == Some(id), |button| button.primary())
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.ai.idea = Some(id);
                            view.ai.links.clear();
                            view.ai
                                .note
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            cx.notify();
                        }))
                }))
                .when(self.ai.idea.is_some(), |panel| {
                    panel.child(self.promote_view(&timeline, cx))
                });
        }
        panel
    }

    fn promote_view(&self, timeline: &Timeline, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child("Titel des neuen Ereignisses")
            .child(Input::new(&self.ai.note))
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
                Button::new("promote")
                    .primary()
                    .label("Idee als Ereignis übernehmen")
                    .on_click(cx.listener(|view, _, window, cx| view.promote_idea(window, cx))),
            )
    }

    fn consistency_view(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let review = Review::from_project(project);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_lg().child("Konsistenz"))
            .child(
                self.request_button(Button::new("check-consistency").label("Konsistenz prüfen"))
                    .on_click(cx.listener(|view, _, _, cx| view.start_consistency(cx))),
            )
            .when(!self.ai.findings.is_empty(), |panel| {
                panel
                    .child("Begründung für eine Entscheidung (optional)")
                    .child(Input::new(&self.ai.note))
            })
            .children(
                self.ai
                    .findings
                    .iter()
                    .enumerate()
                    .filter_map(|(index, finding)| {
                        let status = review.status(finding);
                        if status == ReviewStatus::Dismissed {
                            return None;
                        }
                        Some(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(finding.title.clone())
                                .children(finding.sources.iter().enumerate().filter_map(
                                    |(i, source)| {
                                        let document = project.document(source.document)?;
                                        let scene = SceneText::from_markdown(
                                            document.id(),
                                            document.body(),
                                        );
                                        let anchor = unique_quote(scene.text(), &source.quote)
                                            .and_then(|range| scene.anchor(range).ok());
                                        Some(match anchor {
                                            Some(anchor) => div().child(self.source_button(
                                                10_000 + index * 8 + i,
                                                document.title(),
                                                &anchor,
                                                cx,
                                            )),
                                            None => div().child(format!(
                                                "{} · {}",
                                                document.title(),
                                                source.quote
                                            )),
                                        })
                                    },
                                ))
                                .child(
                                    Button::new(("dismiss-finding", index))
                                        .label(if status == ReviewStatus::Uncertain {
                                            "Fundstellen erneut prüfen"
                                        } else {
                                            "Als beabsichtigt markieren"
                                        })
                                        .disabled(status == ReviewStatus::Uncertain)
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.dismiss_finding(index, cx)
                                        })),
                                ),
                        )
                    }),
            )
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
            self.ai.status = Some("Antwort in der Weltkartei gesichert.".into());
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
                                view.navigate(Page::Codex, window, cx);
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
            .child(self.ai_activity(cx))
            .when_some(draft_here, |pane, _| {
                pane.child("Entwurf")
                    .child(Textarea::new(&self.ai.answer).readonly(true))
                    .child(
                        Button::new("accept-draft")
                            .primary()
                            .label("Markierten Ausschnitt übernehmen")
                            .on_click(
                                cx.listener(|view, _, window, cx| view.accept_draft(window, cx)),
                            ),
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
