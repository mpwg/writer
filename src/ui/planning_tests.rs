use crate::ui::project::test_support::{TestWorkspace, open};
use gpui_kit::{App, ElementId, EntityId, TestAppContext, Window, test::TestWindowExt as _};
use writer::{
    Id, Project,
    outline::Outline,
    timeline::{Relation, Timeline},
};

const FIRST_EVENT: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000001);
const SECOND_EVENT: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000002);
const FIRST_CHAPTER: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000004);
const SECOND_CHAPTER: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000005);
const START_SCENE: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000006);
const SIGNAL_SCENE: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);

fn interact<R>(
    workspace: &TestWorkspace,
    cx: &mut TestAppContext,
    interaction: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    workspace
        .window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            interaction(window, cx)
        })
        .unwrap()
}

fn enter_text(window: &mut Window, entity: EntityId, text: &str, cx: &mut App) {
    window.click(("input", entity), cx);
    assert_eq!(window.find(("input", entity)).focused(), Some(true));
    window.press(
        if cfg!(target_os = "macos") {
            "cmd-a"
        } else {
            "ctrl-a"
        },
        cx,
    );
    window.input(text, cx);
}

fn reopen(workspace: &TestWorkspace) -> Project {
    Project::open(&workspace.root).unwrap()
}

#[gpui_kit::test]
fn timeline_switches_between_network_and_list(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let first = ElementId::from(format!("event-{FIRST_EVENT}"));
    let second = ElementId::from(format!("event-{SECOND_EVENT}"));
    let relation = ElementId::from(format!("network-link-{FIRST_EVENT}-{SECOND_EVENT}-Before"));
    interact(&workspace, cx, |window, cx| {
        assert!(window.find(relation.clone()).visible());
        assert!(
            window.find(first.clone()).bounds().left()
                < window.find(second.clone()).bounds().left()
        );

        window.click("Liste", cx);
        assert!(window.try_find(relation.clone()).is_none());
        assert!(window.find(first).bounds().top() < window.find(second).bounds().top());

        window.click("Netz", cx);
        assert!(window.find(relation).visible());
    });
}

#[gpui_kit::test]
fn timeline_persists_new_event_from_keyboard_input(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let (title, body, time) = workspace.update(cx, |view, _, _| {
        (
            view.event_title.entity_id(),
            view.form_body.entity_id(),
            view.event_time.entity_id(),
        )
    });
    interact(&workspace, cx, |window, cx| {
        window.click("new-event", cx);
        enter_text(window, title, "Brücke geöffnet", cx);
        enter_text(window, body, "Die Fähre legt ab.", cx);
        enter_text(window, time, " Bei Dämmerung ", cx);
        window.press("tab", cx);
        assert_eq!(window.find("save-event").focused(), Some(true));
        window.press("enter", cx);
    });

    let project = reopen(&workspace);
    let timeline = Timeline::from_project(&project);
    let event = timeline
        .ordered_events()
        .into_iter()
        .find(|event| event.title == "Brücke geöffnet")
        .expect("das Ereignis muss auf der Festplatte gespeichert sein");
    assert_eq!(event.body, "Die Fähre legt ab.");
    assert_eq!(event.time.as_deref(), Some("Bei Dämmerung"));
}

#[gpui_kit::test]
fn timeline_preserves_the_event_title_draft_when_selecting_a_chapter(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let title = workspace.update(cx, |view, _, _| view.event_title.entity_id());
    interact(&workspace, cx, |window, cx| {
        window.click(format!("event-{FIRST_EVENT}"), cx);
        enter_text(window, title, "Funkkontakt", cx);
        window.click(("page", 1_usize), cx);
        window.click(format!("chapter-{FIRST_CHAPTER}"), cx);
        window.click(("page", 0_usize), cx);
        window.click("save-event", cx);
    });

    let timeline = Timeline::from_project(&reopen(&workspace));
    assert_eq!(timeline.event(FIRST_EVENT).unwrap().title, "Funkkontakt");
}

#[gpui_kit::test]
fn timeline_preserves_the_event_time_draft_when_saving_a_scene_time(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let (event_time, scene_time) = workspace.update(cx, |view, _, _| {
        (view.event_time.entity_id(), view.scene_time.entity_id())
    });
    interact(&workspace, cx, |window, cx| {
        window.click(format!("event-{FIRST_EVENT}"), cx);
        enter_text(window, event_time, "Bei Sonnenaufgang", cx);
        window.click(("page", 1_usize), cx);
        window.click(format!("scene-{SIGNAL_SCENE}"), cx);
        window.click("outline-edit-scene", cx);
        window.click("scene-details", cx);
        enter_text(window, scene_time, "Nach Mitternacht", cx);
        window.click("set-scene-time", cx);
        window.click(("page", 0_usize), cx);
        window.click("save-event", cx);
    });

    let project = reopen(&workspace);
    let outline = Outline::from_project(&project);
    let scene = outline.scene(SIGNAL_SCENE).unwrap();
    assert_eq!(scene.timing.time.as_deref(), Some("Nach Mitternacht"));
    let timeline = Timeline::from_project(&project);
    assert_eq!(
        timeline.event(FIRST_EVENT).unwrap().time.as_deref(),
        Some("Bei Sonnenaufgang")
    );
}

#[gpui_kit::test]
fn timeline_edits_and_removes_a_relation_from_the_network(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let (note, time) = workspace.update(cx, |view, _, _| {
        (
            view.relation_note.entity_id(),
            view.relation_time.entity_id(),
        )
    });
    let relation = ElementId::from(format!("network-link-{FIRST_EVENT}-{SECOND_EVENT}-Before"));
    interact(&workspace, cx, |window, cx| {
        assert_eq!(
            window.find(relation.clone()).label(),
            Some("100 Jahre später")
        );
        window.click(relation.clone(), cx);
        assert_eq!(
            window.find(("input", note)).value(),
            Some("Das Signal löst den Start aus.")
        );
        enter_text(window, note, "Die Sonde wartet auf grünes Licht.", cx);
        enter_text(window, time, "Zwei Jahre später", cx);
        window.click("Liegt vor", cx);
        assert_eq!(
            window.find(relation.clone()).label(),
            Some("Zwei Jahre später")
        );
    });
    let project = reopen(&workspace);
    let timeline = Timeline::from_project(&project);
    let [link] = timeline.relations() else {
        panic!("die bearbeitete Verbindung muss eindeutig sein");
    };
    assert_eq!(
        (link.from, link.to, link.kind),
        (FIRST_EVENT, SECOND_EVENT, Relation::Before)
    );
    assert_eq!(link.note, "Die Sonde wartet auf grünes Licht.");
    assert_eq!(link.time.as_deref(), Some("Zwei Jahre später"));

    interact(&workspace, cx, |window, cx| {
        window.click(format!("unlink-{FIRST_EVENT}-{SECOND_EVENT}-Before"), cx);
        assert!(window.try_find(relation).is_none());
    });
    assert!(
        Timeline::from_project(&reopen(&workspace))
            .relations()
            .is_empty()
    );
}

#[gpui_kit::test]
fn timeline_reports_cycle_without_changing_the_saved_relations(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let before = Timeline::from_project(&reopen(&workspace))
        .relations()
        .to_vec();
    interact(&workspace, cx, |window, cx| {
        window.click(format!("event-{SECOND_EVENT}"), cx);
        window.click("event-link-picker", cx);
        window.click(format!("target-{FIRST_EVENT}"), cx);
        window.click("Liegt vor", cx);
    });

    let error = workspace
        .update(cx, |view, _, _| view.error.clone())
        .unwrap();
    assert!(error.starts_with("Zeitkreis"), "{error}");
    assert!(
        error.contains("Aufbruch") && error.contains("Erstkontakt"),
        "{error}"
    );
    assert_eq!(
        Timeline::from_project(&reopen(&workspace)).relations(),
        before
    );
}

#[gpui_kit::test]
fn timeline_opens_the_scene_derived_from_an_event(cx: &mut TestAppContext) {
    let workspace = open(cx);
    interact(&workspace, cx, |window, cx| {
        window.click(format!("event-{FIRST_EVENT}"), cx);
        window.click("derive-scene", cx);
        assert!(window.find("confirm-derived-scene").visible());
        window.click("confirm-derived-scene", cx);
        window.click("open-derived-scene", cx);
        assert!(window.find("scene-details").visible());
    });

    let outline = Outline::from_project(&reopen(&workspace));
    let scene = outline
        .unplaced_scenes()
        .find(|scene| scene.event == Some(FIRST_EVENT))
        .expect("die abgeleitete Szene muss auf der Festplatte gespeichert sein");
    assert_eq!(scene.title, "Die Station fängt ein Signal auf.");
    assert_eq!(
        scene.bullets.trim_end(),
        "Die Station fängt ein Signal auf."
    );
}

#[gpui_kit::test]
fn outline_creates_a_chapter_and_places_a_scene_with_the_buttons(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let title = workspace.update(cx, |view, _, _| view.chapter_title.entity_id());
    interact(&workspace, cx, |window, cx| {
        window.click(("page", 1_usize), cx);
        window.click("new-chapter", cx);
        enter_text(window, title, " Die Überfahrt ", cx);
        window.click("chapter-add", cx);
    });
    let created = Outline::from_project(&reopen(&workspace))
        .chapters()
        .iter()
        .find(|chapter| chapter.title == "Die Überfahrt")
        .expect("das neue Kapitel muss auf der Festplatte gespeichert sein")
        .id;

    interact(&workspace, cx, |window, cx| {
        window.click(format!("chapter-{created}"), cx);
        enter_text(window, title, "Späte Überfahrt", cx);
        window.click("chapter-rename", cx);
        window.click(format!("scene-{SIGNAL_SCENE}"), cx);
        window.click("outline-scene-actions", cx);
        assert!(window.find("popup-menu").visible());
        // Removing the current placement is first, then the other existing
        // chapter, then the chapter just added and renamed.
        for _ in 0..3 {
            window.press("down", cx);
        }
        window.press("enter", cx);
    });
    let outline = Outline::from_project(&reopen(&workspace));
    assert_eq!(outline.chapters().last().unwrap().title, "Späte Überfahrt");
    assert_eq!(outline.chapter_of(SIGNAL_SCENE), Some(created));
    assert_eq!(
        outline.scene(SIGNAL_SCENE).unwrap().event,
        Some(FIRST_EVENT)
    );
}

#[gpui_kit::test]
fn outline_dragging_reorders_chapters_and_moves_scenes_without_changing_their_event(
    cx: &mut TestAppContext,
) {
    let workspace = open(cx);
    interact(&workspace, cx, |window, cx| {
        window.click(("page", 1_usize), cx);
        window.drag_to(
            format!("drag-chapter-{SECOND_CHAPTER}"),
            format!("chapter-row-{FIRST_CHAPTER}"),
            cx,
        );
    });
    let chapter_order: Vec<_> = Outline::from_project(&reopen(&workspace))
        .chapters()
        .iter()
        .map(|chapter| chapter.id)
        .collect();
    assert_eq!(chapter_order, [SECOND_CHAPTER, FIRST_CHAPTER]);

    interact(&workspace, cx, |window, cx| {
        window.drag_to(
            format!("drag-scene-{SIGNAL_SCENE}"),
            format!("chapter-row-{FIRST_CHAPTER}"),
            cx,
        );
    });
    let outline = Outline::from_project(&reopen(&workspace));
    assert_eq!(outline.chapters()[1].scenes, [START_SCENE, SIGNAL_SCENE]);

    interact(&workspace, cx, |window, cx| {
        window.drag_to(
            format!("drag-scene-{SIGNAL_SCENE}"),
            format!("scene-row-{START_SCENE}"),
            cx,
        );
    });
    let outline = Outline::from_project(&reopen(&workspace));
    assert_eq!(outline.chapters()[1].scenes, [SIGNAL_SCENE, START_SCENE]);

    interact(&workspace, cx, |window, cx| {
        window.drag_to(format!("drag-scene-{SIGNAL_SCENE}"), "unplaced-scenes", cx);
    });
    let outline = Outline::from_project(&reopen(&workspace));
    assert_eq!(outline.chapter_of(SIGNAL_SCENE), None);
    assert_eq!(
        outline.scene(SIGNAL_SCENE).unwrap().event,
        Some(FIRST_EVENT)
    );
}

#[gpui_kit::test]
fn scene_details_save_time_and_toggle_a_relative_bound(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let time = workspace.update(cx, |view, _, _| view.scene_time.entity_id());
    let before = format!("scene-time-true-{SECOND_EVENT}");
    interact(&workspace, cx, |window, cx| {
        window.click(("page", 1_usize), cx);
        window.click(format!("scene-{SIGNAL_SCENE}"), cx);
        window.click("outline-edit-scene", cx);
        window.click("scene-details", cx);
        enter_text(window, time, "Nach Mitternacht", cx);
        window.click("set-scene-time", cx);
        window.click(before.clone(), cx);
    });
    let outline = Outline::from_project(&reopen(&workspace));
    let scene = outline.scene(SIGNAL_SCENE).unwrap();
    assert_eq!(scene.timing.time.as_deref(), Some("Nach Mitternacht"));
    assert_eq!(scene.timing.before, Some(SECOND_EVENT));
    assert_eq!(scene.event, Some(FIRST_EVENT));

    interact(&workspace, cx, |window, cx| {
        window.click(("page", 1_usize), cx);
        window.click(format!("scene-{START_SCENE}"), cx);
        window.click("outline-edit-scene", cx);
        window.click("scene-details", cx);
        assert_eq!(window.find(("input", time)).value(), Some(""));

        window.click(("page", 1_usize), cx);
        window.click(format!("scene-{SIGNAL_SCENE}"), cx);
        window.click("outline-edit-scene", cx);
        window.click("scene-details", cx);
        assert_eq!(
            window.find(("input", time)).value(),
            Some("Nach Mitternacht")
        );
    });

    interact(&workspace, cx, |window, cx| window.click(before, cx));
    let outline = Outline::from_project(&reopen(&workspace));
    assert_eq!(outline.scene(SIGNAL_SCENE).unwrap().timing.before, None);
}

#[gpui_kit::test]
fn timeline_keeps_event_fields_in_a_separate_inspector(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let title = workspace.update(cx, |view, _, _| view.event_title.entity_id());
    interact(&workspace, cx, |window, cx| {
        let first_event = format!("event-{FIRST_EVENT}");
        assert!(window.try_find(("input", title)).is_none());
        window.click(first_event.clone(), cx);
        let card = window.find(first_event).bounds();
        let field = window.find(("input", title)).bounds();
        assert!(card.right() < field.left());
    });
}

#[gpui_kit::test]
fn outline_discloses_chapter_children_without_hiding_other_chapters(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let disclosure = format!("chapter-disclosure-{FIRST_CHAPTER}");
    let scene = format!("scene-{START_SCENE}");
    interact(&workspace, cx, |window, cx| {
        window.click(("page", 1_usize), cx);
        assert!(window.find(scene.clone()).visible());
        window.click(disclosure.clone(), cx);
        assert!(window.try_find(scene.clone()).is_none());
        assert!(window.find(format!("scene-{SIGNAL_SCENE}")).visible());
        window.click(disclosure, cx);
        assert!(window.find(scene).visible());
    });
}

#[gpui_kit::test]
fn outline_selection_keeps_chapters_visible_until_explicit_edit(cx: &mut TestAppContext) {
    let workspace = open(cx);
    interact(&workspace, cx, |window, cx| {
        window.click(("page", 1_usize), cx);
        window.click(format!("scene-{SIGNAL_SCENE}"), cx);
        assert!(window.find(format!("chapter-{SECOND_CHAPTER}")).visible());
        assert!(window.find("outline-scene-position").visible());
        assert!(
            window
                .find(format!("outline-timeline-event-{FIRST_EVENT}"))
                .visible()
        );
        window.click("outline-edit-scene", cx);
        assert!(window.find("scene-details").visible());
    });
}

#[gpui_kit::test]
fn narrow_timeline_restores_focus_when_returning_from_details(cx: &mut TestAppContext) {
    let workspace = open(cx);
    cx.simulate_window_resize(
        workspace.window,
        gpui_kit::size(gpui_kit::px(740.), gpui_kit::px(720.)),
    );
    interact(&workspace, cx, |window, cx| {
        let event = format!("event-{FIRST_EVENT}");
        window.click(event.clone(), cx);
        assert!(window.try_find(event.clone()).is_none());
        window.click("close-event-inspector", cx);
        assert_eq!(window.find(event).focused(), Some(true));
    });
}

#[gpui_kit::test]
fn outline_context_menu_moves_a_scene_without_changing_its_time(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let before = Outline::from_project(&reopen(&workspace));
    let scene = before.scene(SIGNAL_SCENE).unwrap();
    interact(&workspace, cx, |window, cx| {
        window.click(("page", 1_usize), cx);
        window.right_click(format!("scene-{SIGNAL_SCENE}"), cx);
    });
    interact(&workspace, cx, |window, cx| {
        assert!(window.find("popup-menu").visible());
        // The first command removes the scene from its chapter. The next
        // command places it in the other chapter.
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    });
    let outline = Outline::from_project(&reopen(&workspace));
    assert_eq!(outline.chapter_of(SIGNAL_SCENE), Some(FIRST_CHAPTER));
    assert_eq!(outline.scene(SIGNAL_SCENE).unwrap().timing, scene.timing);
}

#[gpui_kit::test]
fn timeline_separates_unordered_events_from_temporal_paths(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let third_event = Id::from_u128(0x00000000_0000_4000_8000_000000000003);
    let unordered = format!("event-{third_event}");
    interact(&workspace, cx, |window, _| {
        let network = window.find("event-network").bounds();
        let card = window.find(unordered).bounds();
        let section = window.find("timeline-unordered").bounds();
        assert!(card.top() >= network.bottom());
        assert!(section.contains(&card.origin));
    });
}

#[gpui_kit::test]
fn changing_a_relationship_type_replaces_the_selected_edge(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let event_path = workspace.root.join("Planung/Erstkontakt.md");
    let document = std::fs::read_to_string(&event_path).unwrap();
    let document = document.replace(
        "    time: 100 Jahre später",
        "    time: 100 Jahre später\n    redaktion: Erstfassung",
    );
    std::fs::write(&event_path, document).unwrap();
    workspace.update(cx, |view, _, _| view.refresh_project().unwrap());
    interact(&workspace, cx, |window, cx| {
        window.click(
            format!("network-link-{FIRST_EVENT}-{SECOND_EVENT}-Before"),
            cx,
        );
        window.click("Verursacht", cx);
    });
    let timeline = Timeline::from_project(&reopen(&workspace));
    let [edge] = timeline.relations() else {
        panic!("Nach dem Typwechsel muss genau eine Verbindung bestehen");
    };
    assert_eq!(edge.kind, Relation::Cause);
    assert_eq!((edge.from, edge.to), (FIRST_EVENT, SECOND_EVENT));
    assert_eq!(edge.time.as_deref(), Some("100 Jahre später"));
    assert!(
        std::fs::read_to_string(event_path)
            .unwrap()
            .contains("redaktion: Erstfassung")
    );
}

#[gpui_kit::test]
fn narrow_outline_restores_focus_to_the_selected_scene(cx: &mut TestAppContext) {
    let workspace = open(cx);
    cx.simulate_window_resize(
        workspace.window,
        gpui_kit::size(gpui_kit::px(740.), gpui_kit::px(720.)),
    );
    interact(&workspace, cx, |window, cx| {
        let scene = format!("scene-{SIGNAL_SCENE}");
        window.click(("page", 1_usize), cx);
        window.click(scene.clone(), cx);
        assert!(window.try_find(scene.clone()).is_none());
        window.click("close-outline-inspector", cx);
        assert_eq!(window.find(scene).focused(), Some(true));
    });
}

#[gpui_kit::test]
fn timeline_inspector_resizes_from_its_splitter(cx: &mut TestAppContext) {
    let workspace = open(cx);
    let title = workspace.update(cx, |view, _, _| view.event_title.entity_id());
    interact(&workspace, cx, |window, cx| {
        window.click(format!("event-{FIRST_EVENT}"), cx);
        let left = window.find(("input", title)).bounds().left();
        let split = window
            .within("timeline-split")
            .find(("resizable-handle", 0_usize));
        let start = split.bounds().center();
        let end = start - gpui_kit::point(gpui_kit::px(60.), gpui_kit::px(0.));
        window.drag(start, end, cx);
        assert!(window.find(("input", title)).bounds().left() < left);
    });
}
