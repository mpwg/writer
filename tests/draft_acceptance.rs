use writer::{Id, ai::draft::Draft, passage::SceneText};

#[test]
fn draft_accepts_only_the_selected_excerpt_and_keeps_concept_status() {
    let mut scene = SceneText::new(Id::new_v4(), "Das Signal.\nDanach weiter.");
    let anchor = scene.anchor(0.."Das Signal.".len()).unwrap();
    scene.set_concept(&anchor, true).unwrap();
    let draft = Draft::new(anchor, "Ein Licht blinkte. Jemand antwortete.".into());
    assert!(draft.accept("Erfundener Ausschnitt", &mut scene).is_err());
    assert_eq!(scene.text(), "Das Signal.\nDanach weiter.");
    draft.accept("Ein Licht blinkte.", &mut scene).unwrap();
    assert_eq!(scene.text(), "Ein Licht blinkte.\nDanach weiter.");
    assert_eq!(scene.concepts().len(), 1);
    assert_eq!(
        &scene.text()[scene.concepts()[0].range.clone()],
        "Ein Licht blinkte."
    );
}

#[test]
fn reopens_the_latest_draft_as_an_unapplied_conversation() {
    use std::{fs, path::Path};
    use writer::{Project, storage::Store};

    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    Project::create(root, "Entwurf").unwrap();
    let id = Id::new_v4();
    let scene_file = format!("Manuskript/{id}.md");
    let body = "Das Signal";
    fs::write(
        root.join(&scene_file),
        format!("---\nid: {id}\nkind: scene\n---\n{body}"),
    )
    .unwrap();
    let project = Project::open(root).unwrap();
    let text = SceneText::new(id, body);
    let draft = Draft::new(
        text.anchor(0..body.len()).unwrap(),
        "Eine Lampe blinkte.".into(),
    );
    let mut store = Store::new(root).unwrap();
    draft.stage(&project, &mut store, "Knapp erzählen").unwrap();
    store.flush().unwrap();
    let reopened = Project::open(root).unwrap();
    assert_eq!(reopened.scene(id).unwrap().body(), body);
    let history = Draft::history(&reopened, id);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].prompt, "Knapp erzählen");
    assert_eq!(history[0].draft.text(), "Eine Lampe blinkte.");
    assert_eq!(history[0].draft.target().document_id(), id);
    assert!(
        !root
            .join(Path::new("Manuskript").join("Entwurf.md"))
            .exists()
    );
}
