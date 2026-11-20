use std::{fs, path::Path};

use writer::{
    DocumentKind, Id, Project,
    ai::brainstorm::{Brainstorm, IdeaPart, Role},
    knowledge::{Claim, ClaimHolder, ClaimState, Knowledge, KnowledgeKind},
    passage::SceneText,
    sources::{Resolution, SourceForm, Sources},
    storage::Store,
    timeline::{Relation, Timeline},
};

fn write_document(root: &Path, path: &str, id: Id, kind: &str, yaml: &str, body: &str) {
    fs::write(
        root.join(path),
        format!("---\nid: {id}\nkind: {kind}\n{yaml}---\n{body}"),
    )
    .unwrap();
}

#[test]
fn resolves_a_moved_passage_and_rejects_duplicate_quotations_with_the_same_context() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Buch");
    Project::create(&path, "Testbuch").unwrap();
    let id = Id::from_u128(1);
    let dir = "Manuskript/Szene.md";
    let quote = "Die Pforte fiel hörbar zu.";
    let around = format!(
        "{}\n\n{quote}\n\n{}",
        "Vorspann ".repeat(5),
        "Nachsatz ".repeat(5)
    );
    write_document(&path, dir, id, "scene", "", &around);
    let initial = Sources::build(&Project::open(&path).unwrap());
    let selected = initial
        .entries()
        .iter()
        .find(|source| source.anchor.quote() == quote)
        .unwrap();
    fs::rename(path.join(dir), path.join("Manuskript/Versetzt.md")).unwrap();
    let moved = format!("Ein neuer Absatz.\n\n{around}");
    write_document(&path, "Manuskript/Versetzt.md", id, "scene", "", &moved);
    let new_start = moved.find(quote).unwrap();
    let current = Sources::build(&Project::open(&path).unwrap());
    assert_eq!(
        current.resolve(&selected.anchor),
        Resolution::Unique {
            document: id,
            range: new_start..new_start + quote.len()
        }
    );
    write_document(
        &path,
        "Manuskript/Versetzt.md",
        id,
        "scene",
        "",
        &format!("{around}\n\n{around}"),
    );
    assert_eq!(
        Sources::build(&Project::open(&path).unwrap()).resolve(&selected.anchor),
        Resolution::Ambiguous
    );
}

#[test]
fn distinguishes_ordinary_bullets_from_a_concept_written_as_prose() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Buch");
    Project::create(&path, "Testbuch").unwrap();
    let event = Id::from_u128(2);
    let id = Id::from_u128(1);
    let mut text = SceneText::new(id, "Die Pforte fiel hörbar zu.\n\n- Der Schlüssel\n");
    let prose = text.anchor(0.."Die Pforte fiel hörbar zu.".len()).unwrap();
    text.set_concept(&prose, true).unwrap();
    write_document(
        &path,
        "Manuskript/Szene.md",
        id,
        "scene",
        &format!("scene_timing: {{after: {event}}}\n"),
        &text.markdown(),
    );
    let sources = Sources::build(&Project::open(&path).unwrap());
    let entries = sources.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].kind, DocumentKind::Scene);
    assert_eq!(entries[0].form, SourceForm::Prose);
    assert!(entries[0].concept);
    assert_eq!(entries[0].timing.as_ref().unwrap().after, Some(event));
    assert_eq!(entries[1].form, SourceForm::Bullet);
    assert!(!entries[1].concept);
}

#[test]
fn figure_knowledge_begins_at_the_claim_event_and_survives_reopening() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Buch");
    let project = Project::create(&path, "Testbuch").unwrap();
    let mut timeline = Timeline::from_project(&project);
    let before = timeline.create_event("Aufbruch", "", None);
    let when_known = timeline.create_event("Fund", "", None);
    let elsewhere = timeline.create_event("Parallel", "", None);
    timeline
        .relate(before, when_known, Relation::Before, String::new())
        .unwrap();
    let mut knowledge = Knowledge::from_project(&project);
    let character = knowledge.create(KnowledgeKind::Character, "Hüterin", "", None);
    let author = knowledge.create(
        KnowledgeKind::Concept,
        "Das Versteck",
        "Der Schlüssel liegt im Brunnen.",
        Some(Claim {
            holder: ClaimHolder::Author,
            state: ClaimState::Knows,
            from: None,
        }),
    );
    let figure = knowledge.create(
        KnowledgeKind::Concept,
        "Fund der Hüterin",
        "Sie hält den Schlüssel für echt.",
        Some(Claim {
            holder: ClaimHolder::Character(character),
            state: ClaimState::Believes,
            from: Some(when_known),
        }),
    );
    let mut store = Store::new(&path).unwrap();
    knowledge.stage_changes(&project, &mut store).unwrap();
    store.flush().unwrap();
    let knowledge = Knowledge::from_project(&Project::open(&path).unwrap());
    assert_eq!(
        knowledge
            .author_knowledge()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        [author]
    );
    assert_eq!(knowledge.known_by(character, before, &timeline).count(), 0);
    assert_eq!(
        knowledge.known_by(character, elsewhere, &timeline).count(),
        0
    );
    assert_eq!(
        knowledge
            .known_by(character, when_known, &timeline)
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        [figure]
    );
    assert_eq!(
        knowledge.entry(figure).unwrap().claim.unwrap().state,
        ClaimState::Believes
    );
}

#[test]
fn long_blocks_are_covered_without_splitting_any_grapheme() {
    let directory = tempfile::tempdir().unwrap();
    let project = Project::create(directory.path(), "Testbuch").unwrap();
    let id = Id::from_u128(1);
    let prose = "a".repeat(2199) + "e\u{301}" + &"ö".repeat(1500) + "Schlusswort";
    write_document(
        project.root(),
        "Manuskript/Szene.md",
        id,
        "scene",
        "",
        &prose,
    );
    let sources = Sources::build(&Project::open(project.root()).unwrap());
    let quotes: Vec<_> = sources
        .entries()
        .iter()
        .map(|entry| entry.anchor.quote())
        .collect();
    assert!(quotes.len() >= 3);
    assert!(quotes.iter().all(|quote| quote.len() <= 2200));
    assert_eq!(quotes[0].len(), 2199);
    assert!(quotes[1].starts_with("e\u{301}"));
    assert_eq!(quotes.concat(), prose);
}

#[test]
fn indexes_only_unpromoted_marked_chat_ideas_at_their_original_location() {
    let directory = tempfile::tempdir().unwrap();
    let project = Project::create(directory.path(), "Testbuch").unwrap();
    let mut timeline = Timeline::from_project(&project);
    let a = timeline.create_event("Anfang", "", None);
    let b = timeline.create_event("Ende", "", None);
    let mut store = Store::new(project.root()).unwrap();
    timeline.stage_event(a, &project, &mut store).unwrap();
    timeline.stage_event(b, &project, &mut store).unwrap();
    store.flush().unwrap();
    let project = Project::open(project.root()).unwrap();
    let mut chat = Brainstorm::open(&project, [a, b]).unwrap();
    let quote = "Markierte Idee";
    chat.append(Role::Author, quote).unwrap();
    let reply = format!("Unmarkierte Möglichkeit\n\n{quote}\n\nNächster Gedanke");
    let start = reply.find(quote).unwrap();
    let message = chat.append(Role::Codex, &reply).unwrap();
    chat.mark(message, start..start + quote.len()).unwrap();
    let reused = "Schon übernommen";
    let message = chat.append(Role::Codex, reused).unwrap();
    let promoted = chat.mark(message, ..reused.len()).unwrap();
    chat.promote(
        IdeaPart {
            idea: promoted,
            title: reused.into(),
            body: reused.into(),
        },
        &[],
    )
    .unwrap();
    write_document(
        project.root(),
        "Planung/Prüfung.md",
        Id::from_u128(10),
        "consistency_decision",
        "",
        "Die Entscheidung gehört nicht zum Manuskript.",
    );
    let project = Project::open(project.root()).unwrap();
    let sources = Sources::build(&project);
    assert!(
        !sources
            .entries()
            .iter()
            .any(|source| source.kind == DocumentKind::ConsistencyDecision)
    );
    let [source] = sources
        .entries()
        .iter()
        .filter(|source| source.kind == DocumentKind::Brainstorm)
        .collect::<Vec<_>>()[..]
    else {
        panic!("Nur die noch nicht übernommene Markierung soll erscheinen");
    };
    let document = chat.document_id();
    let offset = project
        .document(document)
        .unwrap()
        .body()
        .rfind(quote)
        .unwrap();
    assert_eq!(source.anchor.quote(), quote);
    assert_eq!(
        sources.resolve(&source.anchor),
        Resolution::Unique {
            document,
            range: offset..offset + quote.len()
        }
    );
}
