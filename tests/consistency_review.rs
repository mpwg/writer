use std::{fs, path::Path};

use writer::{
    Id, Project,
    ai::consistency::{Finding, FindingSource, Review, ReviewStatus, check},
    storage::Store,
};

#[test]
fn a_confirmed_decision_survives_reordering_and_cosmetic_changes() {
    let (directory, scene, event) = book();
    let project = Project::open(directory.path()).unwrap();
    let mut store = Store::new(directory.path()).unwrap();
    let finding = finding(scene, event, "100 Jahre");
    let review = Review::from_project(&project);
    assert_eq!(review.status(&finding), ReviewStatus::Open);
    review
        .dismiss(
            &finding,
            Some("Unzuverlässige Erzählerin".into()),
            &mut store,
        )
        .unwrap();
    store.flush().unwrap();

    write_scene(
        directory.path(),
        scene,
        "Sie schaut zur Tür.\n\nDIE FRIST BETRÄGT **100 Jahre**!\n",
    );
    let project = Project::open(directory.path()).unwrap();
    let review = Review::from_project(&project);
    let mut regenerated = finding.clone();
    regenerated.title = "Andere Formulierung des Befunds".into();
    regenerated.sources.reverse();
    assert_eq!(review.status(&regenerated), ReviewStatus::Dismissed);
    assert_eq!(review.decisions().len(), 1);
    assert_eq!(
        review.decisions()[0].reason.as_deref(),
        Some("Unzuverlässige Erzählerin")
    );
    let decision_path = project.document(review.decisions()[0].id).unwrap().path();
    let rendered = fs::read_to_string(decision_path).unwrap();
    assert!(rendered.contains("100 Jahre"));
    assert!(rendered.ends_with("Unzuverlässige Erzählerin"));
}

#[test]
fn changed_quantities_and_negations_reopen_the_finding() {
    let (directory, scene, event) = book();
    let project = Project::open(directory.path()).unwrap();
    let mut store = Store::new(directory.path()).unwrap();
    let did_dismiss = finding(scene, event, "100 Jahre");
    Review::from_project(&project)
        .dismiss(&did_dismiss, None, &mut store)
        .unwrap();
    store.flush().unwrap();

    for (new_text, matching_quote) in [
        ("Die Frist beträgt 50 Jahre.", "50 Jahre"),
        ("Die Frist beträgt nicht 100 Jahre.", "100 Jahre"),
    ] {
        write_scene(directory.path(), scene, new_text);
        let project = Project::open(directory.path()).unwrap();
        let reviewed = check(&project, [finding(scene, event, matching_quote)]);
        assert_eq!(reviewed.len(), 1);
        assert_eq!(reviewed[0].status, ReviewStatus::Open, "{new_text}");
        assert_eq!(reviewed[0].finding.sources.len(), 2);
    }
}

#[test]
fn a_repeated_quote_cannot_hide_a_new_or_uncertain_finding() {
    let (directory, scene, event) = book();
    let project = Project::open(directory.path()).unwrap();
    let mut store = Store::new(directory.path()).unwrap();
    let candidate = finding(scene, event, "100 Jahre");
    Review::from_project(&project)
        .dismiss(&candidate, None, &mut store)
        .unwrap();
    store.flush().unwrap();

    write_scene(
        directory.path(),
        scene,
        "Die Frist beträgt 100 Jahre.\n\n100 Jahre ziehen vorbei.\n",
    );
    let project = Project::open(directory.path()).unwrap();
    let review = Review::from_project(&project);
    assert_eq!(review.status(&candidate), ReviewStatus::Uncertain);
    assert!(review.dismiss(&candidate, None, &mut store).is_err());
    assert!(!store.is_pending());
}

fn finding(scene: Id, event: Id, quote: &str) -> Finding {
    Finding {
        title: "Dauer der Rückkehrfrist".into(),
        sources: vec![
            FindingSource {
                document: scene,
                quote: quote.into(),
            },
            FindingSource {
                document: event,
                quote: "200 Jahre später".into(),
            },
        ],
    }
}

fn book() -> (tempfile::TempDir, Id, Id) {
    let directory = tempfile::tempdir().unwrap();
    Project::create(directory.path(), "Rückkehr").unwrap();
    let [scene, event, target] = [1, 2, 3].map(Id::from_u128);
    write_scene(
        directory.path(),
        scene,
        "Die Frist beträgt 100 Jahre.\n\nSie schaut zur Tür.\n",
    );
    fs::write(
        directory.path().join("Planung/Erstkontakt.md"),
        format!("---\nid: {event}\nkind: event\nrelations:\n  - to: {target}\n    kind: before\n    time: 200 Jahre später\n---\n- Die Station empfängt das Signal.\n"),
    ).unwrap();
    fs::write(
        directory.path().join("Planung/Fristende.md"),
        format!("---\nid: {target}\nkind: event\n---\n"),
    )
    .unwrap();
    (directory, scene, event)
}

fn write_scene(root: &Path, id: Id, body: &str) {
    fs::write(
        root.join("Manuskript/Rückkehr.md"),
        format!("---\nid: {id}\nkind: scene\n---\n{body}"),
    )
    .unwrap();
}

#[test]
fn a_concept_passage_is_located_in_the_editable_scene_text() {
    use writer::ai::consistency::SourceSection;

    let (directory, scene, event) = book();
    let concept = Id::from_u128(4);
    write_scene(
        directory.path(),
        scene,
        &format!(
            "<!-- writer:concept {concept} -->Die Frist beträgt 100 Jahre.<!-- /writer:concept {concept} -->",
        ),
    );
    let project = Project::open(directory.path()).unwrap();
    let review = Review::from_project(&project);
    let candidate = finding(scene, event, "100 Jahre");

    assert_eq!(review.status(&candidate), ReviewStatus::Open);
    let location = review.locate(&candidate.sources[0]).unwrap();
    assert_eq!(location.section, SourceSection::Body);
    assert_eq!(
        location.body_range,
        Some(0.."Die Frist beträgt 100 Jahre.".len())
    );
}

#[test]
fn event_dates_and_link_annotations_are_available_as_verifiable_model_sources() {
    use writer::{ai::consistency::SourceSection, timeline::Relation};

    let (directory, _, event) = book();
    let target = Id::from_u128(3);
    fs::write(
        directory.path().join("Planung/Erstkontakt.md"),
        format!(
            "---\nid: {event}\nkind: event\nevent_time: Zur Wintersonnenwende\nrelations:\n  - to: {target}\n    kind: before\n    time: 200 Jahre später\n    note: Der Rat unterzeichnet die Vereinbarung.\n---\n- Die Station empfängt das Signal.\n",
        ),
    ).unwrap();
    let project = Project::open(directory.path()).unwrap();
    let review = Review::from_project(&project);
    let metadata: Vec<_> = review
        .metadata_sources()
        .iter()
        .map(|entry| {
            let location = review.locate(&entry.source).unwrap();
            assert_eq!(location.body_range, None);
            assert_eq!(location.document, event);
            assert_eq!(location.section, entry.section);
            (entry.section, entry.source.quote.as_str())
        })
        .collect();

    assert_eq!(
        metadata,
        [
            (SourceSection::EventTime, "Zur Wintersonnenwende"),
            (
                SourceSection::RelationNote {
                    to: target,
                    relation: Relation::Before
                },
                "Der Rat unterzeichnet die Vereinbarung."
            ),
            (
                SourceSection::RelationTime {
                    to: target,
                    relation: Relation::Before
                },
                "200 Jahre später"
            ),
        ]
    );
}
