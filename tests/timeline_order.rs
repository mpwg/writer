use writer::{
    Id, Project,
    timeline::{Relation, TemporalOrder, Timeline, TimelineError, TimelineIssue},
};

#[test]
fn a_cause_orders_its_consequences_without_ordering_unrelated_events() {
    let mut timeline = Timeline::default();
    let contact = timeline.create_event("Erstkontakt", "Signal empfangen", None);
    let deadline = timeline.create_event("Fristablauf", "", None);
    let departure = timeline.create_event("Aufbruch", "", None);
    let isolated = timeline.create_event("Isoliert", "", None);
    timeline
        .relate(contact, deadline, Relation::Before, String::new())
        .unwrap();
    timeline
        .relate(deadline, departure, Relation::Cause, "Vertrag".into())
        .unwrap();

    assert_eq!(timeline.compare(contact, departure), TemporalOrder::Before);
    assert_eq!(timeline.compare(departure, deadline), TemporalOrder::After);
    assert_eq!(timeline.compare(isolated, contact), TemporalOrder::Unknown);
    assert_eq!(timeline.compare(isolated, isolated), TemporalOrder::Same);
    assert_eq!(
        timeline.compare(Id::nil(), Id::nil()),
        TemporalOrder::Unknown
    );
    let shown: Vec<_> = timeline
        .ordered_events()
        .iter()
        .map(|event| event.id)
        .collect();
    assert_eq!(shown.len(), 4);
    assert_eq!(
        shown
            .iter()
            .copied()
            .filter(|id| *id != isolated)
            .collect::<Vec<_>>(),
        [contact, deadline, departure]
    );
}

#[test]
fn a_new_cycle_is_rejected_with_the_full_path_and_keeps_existing_order() {
    let mut timeline = Timeline::default();
    let contact = timeline.create_event("Erstkontakt", "", None);
    let deadline = timeline.create_event("Fristablauf", "", None);
    let departure = timeline.create_event("Aufbruch", "", None);
    timeline
        .relate(contact, deadline, Relation::Before, "".into())
        .unwrap();
    timeline
        .relate(deadline, departure, Relation::Before, "".into())
        .unwrap();

    let error = timeline
        .relate(departure, contact, Relation::Cause, "".into())
        .unwrap_err();
    assert_eq!(
        error,
        TimelineError::Cycle {
            path: vec![departure, contact, deadline, departure]
        }
    );
    assert_eq!(timeline.compare(contact, departure), TemporalOrder::Before);
    assert_eq!(timeline.relations().len(), 2);
    assert_eq!(
        timeline.relate(contact, contact, Relation::Before, "".into()),
        Err(TimelineError::Cycle {
            path: vec![contact, contact]
        })
    );
}

#[test]
fn external_cycles_are_diagnosed_while_independent_order_remains_available() {
    let directory = tempfile::tempdir().unwrap();
    Project::create(directory.path(), "Zeitnetz").unwrap();
    let [a, b, c, d] = [1, 2, 3, 4].map(Id::from_u128);
    for (id, target) in [(a, Some(b)), (b, Some(a)), (c, Some(d)), (d, None)] {
        let relations = target
            .map(|target| format!("relations:\n  - to: {target}\n    kind: before\n"))
            .unwrap_or_default();
        std::fs::write(
            directory.path().join("Planung").join(format!("{id}.md")),
            format!("---\nid: {id}\nkind: event\n{relations}---\n"),
        )
        .unwrap();
    }
    let project = Project::open(directory.path()).unwrap();
    let mut timeline = Timeline::from_project(&project);

    assert_eq!(
        timeline.issues(),
        [TimelineIssue::Cycle {
            path: vec![a, b, a]
        }]
    );
    assert_eq!(timeline.compare(a, b), TemporalOrder::Unknown);
    assert_eq!(timeline.compare(c, d), TemporalOrder::Before);
    assert_eq!(timeline.ordered_events().len(), 4);

    assert!(timeline.remove_relation(b, a, Relation::Before));
    assert!(timeline.issues().is_empty());
    assert_eq!(timeline.compare(a, b), TemporalOrder::Before);
}

#[test]
fn missing_targets_never_create_phantom_order() {
    let directory = tempfile::tempdir().unwrap();
    Project::create(directory.path(), "Zeitnetz").unwrap();
    let [event, missing] = [1, 2].map(Id::from_u128);
    std::fs::write(
        directory.path().join("Planung/Ereignis.md"),
        format!(
            "---\nid: {event}\nkind: event\nrelations:\n  - to: {missing}\n    kind: cause\n---\n"
        ),
    )
    .unwrap();
    let project = Project::open(directory.path()).unwrap();
    let timeline = Timeline::from_project(&project);

    assert_eq!(
        timeline.issues(),
        [TimelineIssue::MissingEvent {
            from: event,
            to: missing
        }]
    );
    assert_eq!(timeline.compare(event, missing), TemporalOrder::Unknown);
    assert_eq!(timeline.ordered_events().len(), 1);
}

#[test]
fn editing_a_relation_roundtrips_its_time_and_preserves_external_fields() {
    use writer::storage::Store;

    let directory = tempfile::tempdir().unwrap();
    Project::create(directory.path(), "Planung").unwrap();
    let [a, b] = [1, 2].map(Id::from_u128);
    let source = directory.path().join("Planung/Erstkontakt.md");
    let target = directory.path().join("Planung/Fristablauf.md");
    std::fs::write(
        &source,
        format!(
            "---\nid: {a}\nkind: event\ncustom: reserviert\nrelations:\n  - to: {b}\n    kind: cause\n    external: überliefert\n---\n- Das Signal\n",
        ),
    ).unwrap();
    let target_before = format!("---\nid: {b}\nkind: event\n---\n- Die Frist\n");
    std::fs::write(&target, &target_before).unwrap();

    let mut store = Store::new(directory.path()).unwrap();
    let project = Project::open(directory.path()).unwrap();
    let mut timeline = Timeline::from_project(&project);
    timeline
        .update_event(
            a,
            "Erster Kontakt",
            "- Das grüne Signal\n",
            Some("Sonnenaufgang".into()),
        )
        .unwrap();
    timeline
        .relate_at(
            a,
            b,
            Relation::Cause,
            "Eine Vereinbarung".into(),
            Some("100 Jahre später".into()),
        )
        .unwrap();
    timeline.stage_event(a, &project, &mut store).unwrap();
    store.flush().unwrap();

    assert_eq!(std::fs::read_to_string(target).unwrap(), target_before);
    let project = Project::open(directory.path()).unwrap();
    let timeline = Timeline::from_project(&project);
    assert_eq!(
        timeline.event(a).unwrap().time.as_deref(),
        Some("Sonnenaufgang")
    );
    assert_eq!(timeline.event(a).unwrap().body, "- Das grüne Signal\n");
    let [relation] = timeline.relations() else {
        panic!("Die bearbeitete Beziehung muss genau einmal gespeichert werden");
    };
    assert_eq!(relation.time.as_deref(), Some("100 Jahre später"));
    assert_eq!(relation.note, "Eine Vereinbarung");
    let data = std::fs::read_to_string(source).unwrap();
    assert!(data.contains("reserviert"));
    assert!(data.contains("überliefert"));
}

#[test]
fn network_columns_show_parallel_branches_before_their_shared_successor() {
    let mut timeline = Timeline::default();
    let start = timeline.create_event("Start", "", None);
    let branch_a = timeline.create_event("Linker Zweig", "", None);
    let branch_b = timeline.create_event("Rechter Zweig", "", None);
    let goal = timeline.create_event("Treffen", "", None);
    timeline.create_event("Ohne Verbindung", "", None);
    for (from, to) in [
        (start, branch_a),
        (start, branch_b),
        (branch_a, goal),
        (branch_b, goal),
    ] {
        timeline
            .relate(from, to, Relation::Before, "".into())
            .unwrap();
    }

    let columns: Vec<Vec<_>> = timeline
        .event_columns()
        .into_iter()
        .map(|events| events.into_iter().map(|event| event.id).collect())
        .collect();
    let mut parallel = vec![branch_a, branch_b];
    parallel.sort();
    assert_eq!(columns, [vec![start], parallel, vec![goal]]);
}
