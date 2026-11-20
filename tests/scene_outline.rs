use writer::{
    Id, Project,
    outline::{Outline, OutlineError, SceneTiming},
};

#[test]
fn placing_a_flashback_changes_its_chapter_without_changing_its_event_or_time() {
    let mut outline = Outline::default();
    let [event, after, before, character] = [1, 2, 3, 4].map(Id::from_u128);
    let first_chapter = outline.create_chapter("Erstes Kapitel");
    let second_chapter = outline.create_chapter("Zweites Kapitel");
    let scene = outline.derive_scene(event, "- Das Signal erreicht die Küste");
    let companion = outline.derive_scene(event, "- Eine Antwort bleibt aus");
    let timing = SceneTiming {
        after: Some(after),
        before: Some(before),
        time: Some("Eine Stunde später".into()),
    };
    outline
        .update_scene(
            scene,
            "Das Signal",
            "- Das Signal erreicht die Küste",
            vec![character],
            timing.clone(),
        )
        .unwrap();

    assert_eq!(outline.unplaced_scenes().count(), 2);
    outline.place(scene, first_chapter, 0).unwrap();
    outline.place(scene, second_chapter, 0).unwrap();

    assert_eq!(outline.chapter_of(scene), Some(second_chapter));
    assert!(outline.chapters()[0].scenes.is_empty());
    let flashback = outline.scene(scene).unwrap();
    assert_eq!(flashback.event, Some(event));
    assert_eq!(flashback.timing, timing);
    assert_eq!(flashback.characters, [character]);
    assert_eq!(
        outline
            .unplaced_scenes()
            .map(|scene| scene.id)
            .collect::<Vec<_>>(),
        [companion]
    );
}

#[test]
fn an_invalid_target_keeps_the_existing_chapter_position() {
    let mut outline = Outline::default();
    let chapter = outline.create_chapter("Kapitel");
    let scene = outline.derive_scene(Id::from_u128(1), "");
    outline.place(scene, chapter, 0).unwrap();

    assert_eq!(
        outline.place(scene, chapter, 1),
        Err(OutlineError::InvalidPosition { index: 1, len: 0 })
    );
    assert_eq!(outline.chapters()[0].scenes, [scene]);
}

#[test]
fn chapter_and_scene_reordering_use_final_positions() {
    let mut outline = Outline::default();
    let first = outline.create_chapter("Kapitel 1");
    let second = outline.create_chapter("Kapitel 2");
    let [a, b, c] = [1, 2, 3].map(|id| outline.derive_scene(Id::from_u128(id), ""));
    outline.place(a, first, 0).unwrap();
    outline.place(b, first, 1).unwrap();
    outline.place(c, first, 2).unwrap();

    outline.place(a, first, 2).unwrap();
    outline.move_chapter(second, 0).unwrap();
    outline.rename_chapter(first, "Rückblende").unwrap();

    assert_eq!(outline.chapters()[0].id, second);
    assert_eq!(outline.chapters()[1].scenes, [b, c, a]);
    assert_eq!(outline.chapters()[1].title, "Rückblende");
    assert!(outline.remove_from_chapter(c).unwrap());
    assert_eq!(outline.chapter_of(c), None);
    assert_eq!(outline.chapters()[1].scenes, [b, a]);
}

#[test]
fn external_scene_metadata_is_independent_of_the_chapter_list() {
    let directory = tempfile::tempdir().unwrap();
    Project::create(directory.path(), "Signal").unwrap();
    let [scene, event, after, before, chapter, first] = [1, 2, 3, 4, 5, 6].map(Id::from_u128);
    std::fs::write(
        directory.path().join("Manuskript/Signal.md"),
        format!("---\nid: {scene}\nkind: scene\nscene_event: {event}\nscene_timing:\n  after: {after}\n  before: {before}\n  time: Nachts\n---\nEin grünes Licht.\n"),
    ).unwrap();
    for (id, order, scenes) in [
        (chapter, 1, format!("scenes: [{scene}]\n")),
        (first, 0, String::new()),
    ] {
        std::fs::write(
            directory.path().join("Manuskript").join(format!("{id}.md")),
            format!("---\nid: {id}\nkind: chapter\nchapter_order: {order}\n{scenes}---\n"),
        )
        .unwrap();
    }
    let project = Project::open(directory.path()).unwrap();
    let outline = Outline::from_project(&project);

    assert_eq!(outline.chapters()[0].id, first);
    assert_eq!(outline.chapter_of(scene), Some(chapter));
    let scene = outline.scene(scene).unwrap();
    assert_eq!(scene.bullets, "Ein grünes Licht.\n");
    assert_eq!(scene.event, Some(event));
    assert_eq!(
        scene.timing,
        SceneTiming {
            after: Some(after),
            before: Some(before),
            time: Some("Nachts".into())
        }
    );
}

#[test]
fn a_saved_chapter_move_preserves_the_flashback_file_byte_for_byte() {
    use writer::storage::Store;

    let directory = tempfile::tempdir().unwrap();
    Project::create(directory.path(), "Signal").unwrap();
    let [event, after, before, legacy] = [1, 2, 3, 4].map(Id::from_u128);
    let chapter_path = directory.path().join("Manuskript/01-Kapitel.md");
    std::fs::write(
        &chapter_path,
        format!("---\nid: {legacy}\nkind: chapter\nvermerk: erhalten\n---\nAnmerkungen.\n"),
    )
    .unwrap();
    let mut project = Project::open(directory.path()).unwrap();
    let mut store = Store::new(directory.path()).unwrap();
    let mut outline = Outline::from_project(&project);
    let scene = outline.derive_scene(event, "- Das Signal");
    let second = outline.create_chapter("Kapitel 2");
    let timing = SceneTiming {
        after: Some(after),
        before: Some(before),
        time: Some("Mitternacht".into()),
    };
    outline
        .update_scene(
            scene,
            "Das Signal",
            "- Die Rückblende",
            vec![],
            timing.clone(),
        )
        .unwrap();
    outline.stage_changes(&project, &mut store).unwrap();
    store.flush().unwrap();

    project = Project::open(directory.path()).unwrap();
    let mut outline = Outline::from_project(&project);
    assert_eq!(
        outline
            .chapters()
            .iter()
            .map(|chapter| chapter.id)
            .collect::<Vec<_>>(),
        [legacy, second]
    );
    assert_eq!(outline.chapter_of(scene), None);
    let scene_path = project.scene(scene).unwrap().path().to_owned();
    let before_placement = std::fs::read(&scene_path).unwrap();

    outline.place(scene, legacy, 0).unwrap();
    outline.place(scene, second, 0).unwrap();
    outline.move_chapter(second, 0).unwrap();
    outline.stage_changes(&project, &mut store).unwrap();
    store.flush().unwrap();

    let project = Project::open(directory.path()).unwrap();
    let outline = Outline::from_project(&project);
    assert_eq!(outline.chapters()[0].id, second);
    assert_eq!(outline.chapter_of(scene), Some(second));
    assert_eq!(outline.scene(scene).unwrap().event, Some(event));
    assert_eq!(outline.scene(scene).unwrap().timing, timing);
    assert_eq!(std::fs::read(scene_path).unwrap(), before_placement);
    let preserved_chapter = std::fs::read_to_string(chapter_path).unwrap();
    assert!(preserved_chapter.contains("vermerk: erhalten"));
    assert!(preserved_chapter.ends_with("Anmerkungen.\n"));
}

#[test]
fn scene_and_chapter_drop_targets_resolve_against_current_positions() {
    let mut outline = Outline::default();
    let first = outline.create_chapter("Kapitel 1");
    let second = outline.create_chapter("Kapitel 2");
    let third = outline.create_chapter("Kapitel 3");
    let [a, b, c] = [1, 2, 3].map(|id| outline.derive_scene(Id::from_u128(id), ""));
    outline.append_scene(a, first).unwrap();
    outline.append_scene(b, first).unwrap();
    outline.append_scene(c, second).unwrap();

    outline.place_at_scene(a, first, b).unwrap();
    assert_eq!(outline.chapters()[0].scenes, [b, a]);
    outline.append_scene(b, first).unwrap();
    assert_eq!(outline.chapters()[0].scenes, [a, b]);
    outline.place_at_scene(c, first, b).unwrap();
    assert_eq!(outline.chapters()[0].scenes, [a, c, b]);
    outline.move_chapter_to(first, third).unwrap();
    assert_eq!(
        outline
            .chapters()
            .iter()
            .map(|chapter| chapter.id)
            .collect::<Vec<_>>(),
        [second, third, first]
    );
}
