use tempfile::tempdir;
use writer::{
    Id, Project,
    ai::brainstorm::{Brainstorm, BrainstormError, IdeaPart, Link, LinkDirection, Role},
    storage::Store,
    timeline::{Relation, TemporalOrder, Timeline},
};

#[test]
fn pair_order_opens_one_markdown_conversation() {
    let (directory, project, first, second) = book();
    let mut chat = Brainstorm::open(&project, [first, second]).unwrap();
    let prompt = "Wie kommt die Figur heil an?";
    let reply = "Die Wache dreht sich um.\n\nDann läuft sie.";
    chat.append(Role::Author, prompt).unwrap();
    let message = chat.append(Role::Codex, reply).unwrap();
    let idea = chat
        .mark(message, .."Die Wache dreht sich um.".len())
        .unwrap();

    let project = Project::open(project.root()).unwrap();
    let reopened = Brainstorm::open(&project, [second, first]).unwrap();
    assert_eq!(reopened.messages(), chat.messages());
    assert_eq!(reopened.ideas(), chat.ideas());
    assert_eq!(reopened.ideas()[0].id, idea);
    assert_eq!(reopened.ideas()[0].quote, "Die Wache dreht sich um.");
    let paths: Vec<_> = std::fs::read_dir(directory.path().join("book/Brainstorming"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 1);
    let file = std::fs::read_to_string(&paths[0]).unwrap();
    assert!(file.contains(&format!("## Anfrage\n\n{prompt}")));
    assert!(file.contains(&format!("## Codex\n\n{reply}")));
}

#[test]
fn reopening_a_chat_preserves_external_markdown_edits() {
    let (_directory, project, first, second) = book();
    let mut chat = Brainstorm::open(&project, [first, second]).unwrap();
    chat.append(Role::Codex, "Alter Text").unwrap();
    let project = Project::open(project.root()).unwrap();
    let path = project.document(chat.document_id()).unwrap().path();
    let updated = std::fs::read_to_string(path)
        .unwrap()
        .replace("Alter Text", "Geänderter Text");
    std::fs::write(path, updated).unwrap();

    assert!(matches!(
        chat.append(Role::Author, "Frage"),
        Err(BrainstormError::Changed)
    ));
    let mut reopened =
        Brainstorm::open(&Project::open(project.root()).unwrap(), [first, second]).unwrap();
    reopened.append(Role::Author, "Frage").unwrap();
    assert_eq!(reopened.messages()[0].text, "Geänderter Text");
}

#[test]
fn only_confirmation_changes_events_and_retries_reuse_the_same_event() {
    let (_directory, project, first, second) = book();
    let mut chat = Brainstorm::open(&project, [first, second]).unwrap();
    let text = "Die Wache dreht sich um.";
    let message = chat.append(Role::Codex, text).unwrap();
    let idea = chat.mark(message, ..text.len()).unwrap();
    let choice = IdeaPart {
        idea,
        title: "Ablenkung".into(),
        body: text.into(),
    };
    let links = [
        Link {
            event: first,
            direction: LinkDirection::Incoming,
            kind: Relation::Cause,
            note: "Signal".into(),
        },
        Link {
            event: second,
            direction: LinkDirection::Outgoing,
            kind: Relation::Before,
            note: String::new(),
        },
    ];
    let updated = Project::open(project.root()).unwrap();
    assert_eq!(Timeline::from_project(&updated).ordered_events().len(), 2);

    let id = chat.promote(choice.clone(), &links).unwrap();
    let updated = Project::open(project.root()).unwrap();
    let mut reopened = Brainstorm::open(&updated, [second, first]).unwrap();
    assert_eq!(reopened.promote(choice, &links).unwrap(), id);

    let updated = Project::open(project.root()).unwrap();
    let timeline = Timeline::from_project(&updated);
    assert_eq!(timeline.ordered_events().len(), 3);
    assert_eq!(timeline.compare(first, id), TemporalOrder::Before);
    assert_eq!(timeline.compare(id, second), TemporalOrder::Before);
    assert!(timeline.relations().iter().any(|link| {
        link.from == first && link.to == id && link.kind == Relation::Cause && link.note == "Signal"
    }));
    assert!(
        timeline
            .event(id)
            .unwrap()
            .body
            .contains(&format!("#nachricht-{message}"))
    );
}

#[test]
fn an_invalid_promotion_writes_neither_event_nor_relations() {
    let (_directory, project, first, second) = book();
    let mut chat = Brainstorm::open(&project, [first, second]).unwrap();
    let text = "Umweg";
    let message = chat.append(Role::Codex, text).unwrap();
    let idea = chat.mark(message, ..text.len()).unwrap();
    let result = chat.promote(
        IdeaPart {
            idea,
            title: text.into(),
            body: text.into(),
        },
        &[
            Link {
                event: second,
                direction: LinkDirection::Incoming,
                kind: Relation::Before,
                note: String::new(),
            },
            Link {
                event: first,
                direction: LinkDirection::Outgoing,
                kind: Relation::Before,
                note: String::new(),
            },
        ],
    );
    assert!(matches!(result, Err(BrainstormError::Timeline(_))));
    let timeline = Timeline::from_project(&Project::open(project.root()).unwrap());
    assert_eq!(timeline.ordered_events().len(), 2);
    assert_eq!(timeline.relations().len(), 1);
}

fn book() -> (tempfile::TempDir, Project, Id, Id) {
    let directory = tempdir().unwrap();
    let root = directory.path().join("book");
    let project = Project::create(&root, "Testbuch").unwrap();
    let mut timeline = Timeline::from_project(&project);
    let first = timeline.create_event("Erstkontakt", "Mara sieht eine Tür.", None);
    let second = timeline.create_event("Fristablauf", "Die Tür schließt sich.", None);
    timeline
        .relate(first, second, Relation::Before, String::new())
        .unwrap();
    let mut store = Store::new(&root).unwrap();
    timeline.stage_event(first, &project, &mut store).unwrap();
    timeline.stage_event(second, &project, &mut store).unwrap();
    store.flush().unwrap();
    (directory, Project::open(&root).unwrap(), first, second)
}
