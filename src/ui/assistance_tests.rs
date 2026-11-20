use super::{
    AiPreferences, AiSession, AiStatus, Brainstorm, ConsistencyRun, Draft, Finding, FindingSource,
    Page, Pending, Project, Relation, Review, Role, SceneText, Timeline, citation_batches,
    test_support::{self, TestWorkspace},
};
use gpui_kit::{App, AppContext, Focusable, TestAppContext, Window, px, size, test::TestWindowExt};
use writer::{
    knowledge::{Knowledge, KnowledgeKind},
    storage::Store,
};

fn codex_window<R>(
    cx: &mut TestAppContext,
    workspace: &TestWorkspace,
    run: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    cx.update_window(workspace.window, |_, window, cx| run(window, cx))
        .expect("Codex-Testfenster")
}

#[gpui_kit::test]
fn timeline_brainstorm_switches_to_the_selected_event_pair(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    cx.simulate_window_resize(workspace.window, size(px(1440.), px(1600.)));
    let project = Project::open(&workspace.root).unwrap();
    let timeline = Timeline::from_project(&project);
    let event = |title: &str| {
        timeline
            .ordered_events()
            .into_iter()
            .find(|event| event.title == title)
            .unwrap()
            .id
    };
    let first = event("Erstkontakt");
    let second = event("Aufbruch");
    let other = event("Fristablauf");
    codex_window(cx, &workspace, |window, cx| {
        window.click(format!("event-{first}"), cx);
        window.click("brainstorm-from-event", cx);
        window.click(format!("brainstorm-second-{second}"), cx);
    });
    let message = workspace.update(cx, |view, window, cx| {
        let pair = view.ai.brainstorm.as_ref().unwrap().pair();
        view.complete_ai(
            Pending::Brainstorm {
                pair,
                prompt: "Was kommt danach?".into(),
                retried: false,
            },
            "Die Wache öffnet den Seiteneingang.".into(),
            None,
            window,
            cx,
        );
        view.ai.brainstorm.as_ref().unwrap().messages()[0].id
    });
    let message_button = format!("message-{message}");

    codex_window(cx, &workspace, |window, cx| {
        window.click(("page", 0_usize), cx);
        window.click(format!("event-{first}"), cx);
        window.click("brainstorm-from-event", cx);
        assert!(window.find(message_button.clone()).visible());

        window.click(("page", 0_usize), cx);
        window.click(format!("event-{other}"), cx);
        window.click("brainstorm-from-event", cx);
        assert!(window.try_find("send-chat").is_none());
        assert!(window.try_find(message_button.clone()).is_none());
        window.click(format!("brainstorm-second-{second}"), cx);
        assert!(window.find("send-chat").visible());
        assert!(window.try_find(message_button.clone()).is_none());

        window.click(format!("brainstorm-first-{second}"), cx);
        window.click(format!("brainstorm-second-{first}"), cx);
        assert!(window.try_find(message_button.clone()).is_some());
        window.click(message_button, cx);
        window.click(format!("brainstorm-second-{first}"), cx);
        assert!(window.find("mark-idea").visible());
    });
}

#[gpui_kit::test]
fn brainstorm_promotes_only_the_selected_passage_with_the_chosen_link(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    cx.simulate_window_resize(workspace.window, size(px(1440.), px(1600.)));
    let project = Project::open(&workspace.root).unwrap();
    let timeline = Timeline::from_project(&project);
    let event_named = |title: &str| {
        timeline
            .ordered_events()
            .into_iter()
            .find(|event| event.title == title)
            .unwrap()
            .id
    };
    let first = event_named("Erstkontakt");
    let second = event_named("Aufbruch");
    let quote = "Die Wache öffnet den Seiteneingang.";
    let reply = format!("Eine Idee: {quote} Danach kommt der Suchtrupp.");
    let mut chat = Brainstorm::open(&project, [first, second]).unwrap();
    let message = chat.append(Role::Codex, &reply).unwrap();
    workspace.update(cx, |view, _, _| view.refresh_project().unwrap());

    codex_window(cx, &workspace, |window, cx| {
        window.click(format!("event-{first}"), cx);
        window.click("brainstorm-from-event", cx);
        assert!(
            window
                .try_find(format!("brainstorm-second-{first}"))
                .is_none()
        );
        window.click(format!("brainstorm-second-{second}"), cx);
        assert_eq!(window.find("send-chat").disabled(), Some(true));
        window.click(format!("message-{message}"), cx);
    });
    workspace.update(cx, |view, _, cx| {
        assert_eq!(view.ai.answer.read(cx).value(), reply);
        let start = reply.find(quote).unwrap();
        view.ai.answer.update(cx, |answer, cx| {
            answer.set_selected_range(start..start + quote.len(), cx);
        });
    });
    codex_window(cx, &workspace, |window, cx| {
        window.click("mark-idea", cx);
        window.click(format!("promote-{first}-2"), cx);
    });

    let project = Project::open(&workspace.root).unwrap();
    let chat = Brainstorm::open(&project, [first, second]).unwrap();
    assert_eq!(chat.ideas().len(), 1);
    assert_eq!(chat.ideas()[0].quote, quote);
    assert_eq!(Timeline::from_project(&project).ordered_events().len(), 3);

    let title = "Ablenkung";
    workspace.update(cx, |view, window, cx| {
        view.ai.note.update(cx, |note, cx| note.focus(window, cx));
    });
    codex_window(cx, &workspace, |window, cx| {
        window.input(title, cx);
        window.click("promote", cx);
    });

    let project = Project::open(&workspace.root).unwrap();
    let updated = Timeline::from_project(&project);
    let promoted = updated
        .ordered_events()
        .into_iter()
        .find(|event| event.title == title)
        .expect("Die Bestätigung legt das Ereignis an.");
    assert_eq!(updated.ordered_events().len(), 4);
    assert!(promoted.body.contains(quote));
    assert!(promoted.body.contains(&format!("#nachricht-{message}")));
    assert!(!promoted.body.contains("Danach kommt der Suchtrupp."));
    assert!(updated.relations().iter().any(|relation| {
        relation.from == first && relation.to == promoted.id && relation.kind == Relation::Cause
    }));
    workspace.update(cx, |view, _, _| {
        assert!(matches!(view.page, Page::Timeline));
        assert_eq!(view.event, Some(promoted.id));
    });
}

#[gpui_kit::test]
fn scene_applies_only_the_marked_part_of_a_canned_draft(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    cx.simulate_window_resize(workspace.window, size(px(1440.), px(1600.)));
    let path = workspace.root.join("Manuskript/Signal.md");
    let original = "- Tür öffnen\n- Leise eintreten";
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(path, source.replace("Eine Tür öffnet sich.", original)).unwrap();
    let project = Project::open(&workspace.root).unwrap();
    let scene = project
        .scenes()
        .find(|scene| scene.title() == "Das Signal")
        .unwrap();
    let id = scene.id();
    let selected_reply = "Das Schott öffnete sich lautlos.";
    let full_reply = format!("{selected_reply} Der Suchtrupp kehrte zurück.");
    let text = SceneText::from_markdown(id, scene.body());
    let anchor = text.anchor(0..original.len()).unwrap();
    assert_eq!(anchor.quote(), original);
    workspace.update(cx, |view, window, cx| {
        view.refresh_project().unwrap();
        view.select_scene(id, window, cx);
        view.complete_ai(
            Pending::Draft {
                anchor,
                prompt: "Eine knappe Fassung".into(),
            },
            full_reply.clone(),
            None,
            window,
            cx,
        );
    });

    let project = Project::open(&workspace.root).unwrap();
    assert!(project.scene(id).unwrap().body().contains(original));
    assert_eq!(Draft::history(&project, id)[0].draft.text(), full_reply);

    codex_window(cx, &workspace, |window, cx| {
        window.render_frame(cx);
        for action in ["start-draft", "revise-draft", "detail-manual"] {
            assert_eq!(window.find(action).disabled(), Some(true));
        }
    });
    workspace.update(cx, |view, _, cx| {
        view.ai.answer.update(cx, |answer, cx| {
            answer.set_selected_range(0..selected_reply.len(), cx);
        });
    });
    codex_window(cx, &workspace, |window, cx| {
        window.click("accept-draft", cx);
        window.click(("page", 0_usize), cx);
    });

    let project = Project::open(&workspace.root).unwrap();
    let saved = SceneText::from_markdown(id, project.scene(id).unwrap().body());
    assert_eq!(
        saved.text().trim_end(),
        "Das Schott öffnete sich lautlos.\nDanach verstummt die Antenne."
    );
    assert_eq!(saved.concepts().len(), 1);
    assert_eq!(
        &saved.text()[saved.concepts()[0].range.clone()],
        selected_reply
    );
    assert_eq!(Knowledge::from_project(&project).entries().count(), 0);
}

#[gpui_kit::test]
fn late_scene_draft_requires_a_new_selection_for_a_duplicated_target(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    let scene = writer::Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let home = tempfile::tempdir().unwrap();
    enable_fixture(cx, &workspace, home.path());
    let quote = "Eine Tür öffnet sich.";
    let inserted = "Das Schott gab lautlos nach.";
    let original = workspace.update(cx, |view, window, cx| {
        view.select_scene(scene, window, cx);
        view.editor.update(cx, |editor, cx| {
            editor.set_selected_range(0..quote.len(), cx);
            editor.value().to_string()
        })
    });
    codex_window(cx, &workspace, |window, cx| window.click("start-draft", cx));
    let request = home.path().join("codex/draft-request");
    wait_for(cx, |_| request.exists());

    let duplicated = format!("Ein neuer Absatz.\n\n{original}\n{original}");
    workspace.update(cx, |view, window, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_selected_range(0..original.len(), cx);
            editor.focus(window, cx);
        });
    });
    codex_window(cx, &workspace, |window, cx| window.input(&duplicated, cx));
    let response = home.path().join("codex/draft-response");
    let temporary = response.with_extension("tmp");
    std::fs::write(&temporary, format!("{inserted} Der übrige Entwurf.")).unwrap();
    std::fs::rename(temporary, response).unwrap();
    wait_for(cx, |cx| {
        workspace.update(cx, |view, _, _| view.ai.draft.is_some())
    });
    workspace.update(cx, |view, _, cx| {
        view.ai.answer.update(cx, |answer, cx| {
            answer.set_selected_range(0..inserted.len(), cx)
        });
    });

    codex_window(cx, &workspace, |window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("accept-draft").disabled(), Some(true));
        window.click("choose-draft-target", cx);
        window.click("accept-draft-at-selection", cx);
    });
    workspace.update(cx, |view, _, cx| {
        assert_eq!(view.editor.read(cx).value(), duplicated)
    });

    let second = duplicated.rfind(quote).unwrap();
    workspace.update(cx, |view, _, cx| {
        view.editor.update(cx, |editor, cx| {
            editor.set_selected_range(second..second + quote.len(), cx)
        });
    });
    codex_window(cx, &workspace, |window, cx| {
        window.click("accept-draft-at-selection", cx)
    });
    workspace.update(cx, |view, _, cx| assert!(view.flush(cx)));
    let project = Project::open(&workspace.root).unwrap();
    let saved = SceneText::from_markdown(scene, project.scene(scene).unwrap().body());
    assert_eq!(
        saved.text(),
        format!(
            "{}{inserted}{}",
            &duplicated[..second],
            &duplicated[second + quote.len()..]
        )
    );
}

#[gpui_kit::test]
fn research_only_shows_verified_citations_and_opens_the_chosen_passage(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    let project = Project::open(&workspace.root).unwrap();
    let scene = project
        .scenes()
        .find(|scene| scene.title() == "Das Signal")
        .unwrap();
    let id = scene.id();
    let quote = "Eine Tür öffnet sich.";
    let citations: Vec<_> = citation_batches(&project).into_iter().flatten().collect();
    let source = citations
        .into_iter()
        .find(|source| source.anchor.document_id() == id && source.anchor.quote() == quote)
        .expect("Beleg mit Konzeptstatus");
    let event = source.evidence.event.expect("Zeitbezug des Belegs");
    workspace.update(cx, |view, window, cx| {
        view.complete_ai(
            Pending::ResearchSummary(vec![source]),
            r#"[
                {"statement":"Die Tür öffnet sich.","sources":[0]},
                {"statement":"Eine zweite Tür öffnet sich.","sources":[1]}
            ]"#
            .into(),
            None,
            window,
            cx,
        );
        assert_eq!(view.ai.answers.len(), 1);
    });
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    codex_window(cx, &workspace, |window, cx| {
        window.click("ask-from-scene", cx);
        assert_eq!(
            window.find(("source", 0_usize)).label(),
            Some("Das Signal · Konzept · bis zum Bezugspunkt · Eine Tür öffnet sich.")
        );
        window.click(format!("research-at-{event}"), cx);
        assert_eq!(
            window.find(("source", 0_usize)).label(),
            Some("Das Signal · Konzept · zeitlich offen · Eine Tür öffnet sich.")
        );
        assert!(window.try_find(("source", 1_000_usize)).is_none());
        window.click(("source", 0_usize), cx);
    });
    workspace.update(cx, |view, _, cx| {
        assert!(matches!(view.page, Page::Scene));
        assert_eq!(view.selected_scene, Some(id));
        assert_eq!(view.editor.read(cx).selected_text(), quote);
    });
}

#[gpui_kit::test]
fn research_from_the_scene_preserves_chronology_and_links_each_passage(cx: &mut TestAppContext) {
    use serde_json::{Value, json};
    use writer::{Id, timeline::TemporalOrder};

    let workspace = test_support::open(cx);
    let home = tempfile::tempdir().unwrap();
    let project = Project::open(&workspace.root).unwrap();
    let mut timeline = Timeline::from_project(&project);
    let mut store = Store::new(&workspace.root).unwrap();
    let scene_id = |title| {
        project
            .scenes()
            .find(|scene| scene.title() == title)
            .unwrap()
            .id()
    };
    let start = scene_id("Der Start");
    let flashback = scene_id("Das Signal");
    let reference = timeline
        .ordered_events()
        .into_iter()
        .find(|event| event.title == "Aufbruch")
        .unwrap()
        .id;
    let sources = [
        (
            flashback,
            "Das Signal",
            "Prosa",
            "bis zum Bezugspunkt",
            "Ada traf Ben an der Antenne.",
        ),
        (
            start,
            "Der Start",
            "Planung",
            "bis zum Bezugspunkt",
            "- Ada soll Ben an der Rampe treffen.",
        ),
        (
            Id::new_v4(),
            "An der Schleuse",
            "Prosa",
            "zeitlich offen",
            "Ada traf Ben an der Schleuse.",
        ),
        (
            Id::new_v4(),
            "Im Funkraum",
            "Prosa",
            "zeitlich offen",
            "Ada traf Ben im Funkraum.",
        ),
    ];
    let mut open_events = Vec::new();
    for (id, title, _, _, quote) in sources {
        if let Some(scene) = project.scene(id) {
            store
                .stage_body(&store.relative_path(scene).unwrap(), scene, quote)
                .unwrap();
        } else {
            let event = timeline.create_event(title, "", None);
            timeline.stage_event(event, &project, &mut store).unwrap();
            open_events.push(event);
            std::fs::write(
                workspace.root.join("Manuskript").join(format!("{id}.md")),
                format!("---\nid: {id}\nkind: scene\ntitle: {title}\nscene_event: {event}\n---\n{quote}\n"),
            )
            .unwrap();
        }
    }
    store.flush().unwrap();
    assert_eq!(
        timeline.compare(open_events[0], open_events[1]),
        TemporalOrder::Unknown
    );
    workspace.update(cx, |view, _, _| view.refresh_project().unwrap());
    enable_fixture(cx, &workspace, home.path());
    let question = "Wann haben sich Ada und Ben zuletzt getroffen?";
    codex_window(cx, &workspace, |window, cx| {
        window.click("close-settings", cx);
        window.click(("page", 1_usize), cx);
        window.click(format!("scene-{start}"), cx);
        window.click("outline-edit-scene", cx);
        window.click("ask-from-scene", cx);
        let input = workspace.view.read(cx).ai.research_input.entity_id();
        window.click(("input", input), cx);
        window.input(question, cx);
        window.click("ask", cx);
    });

    let reply = |cx: &mut TestAppContext, kind: &str, statements: &[(&str, &[usize])]| {
        let directory = home.path().join("codex");
        let request_path = directory.join(format!("{kind}-request"));
        wait_for(cx, |_| request_path.exists());
        let request: Value = serde_json::from_slice(&std::fs::read(request_path).unwrap()).unwrap();
        let prompt = request["params"]["input"][0]["text"].as_str().unwrap();
        assert!(prompt.contains(question));
        let numbers: Vec<_> = sources
            .iter()
            .map(|(_, title, scenario, scope, quote)| {
                let (before_quote, _) = prompt.split_once(&format!("\n{quote}\n")).unwrap();
                let header = before_quote.lines().last().unwrap();
                let (number, context) = header.split_once(": ").unwrap();
                assert_eq!(context, format!("{title} | {scenario} | {scope} | "));
                number.parse::<usize>().unwrap()
            })
            .collect();
        let precedes = |earlier: usize, later: usize| {
            let edge = format!("{} liegt vor {}", numbers[earlier], numbers[later]);
            prompt.lines().any(|line| line == edge)
        };
        assert!(precedes(0, 1));
        assert!(!precedes(2, 3) && !precedes(3, 2));
        let result: Vec<_> = statements
            .iter()
            .map(|(statement, indices)| {
                json!({
                    "statement": statement,
                    "sources": indices.iter().map(|&index| numbers[index]).collect::<Vec<_>>(),
                })
            })
            .collect();
        let response = directory.join(format!("{kind}-response"));
        let temporary = response.with_extension("tmp");
        std::fs::write(&temporary, serde_json::to_vec(&result).unwrap()).unwrap();
        std::fs::rename(temporary, response).unwrap();
    };
    reply(
        cx,
        "research",
        &[
            ("In der Rückblende treffen sie sich an der Antenne.", &[0]),
            ("Das Treffen an der Rampe ist geplant.", &[1]),
            (
                "Für Schleuse und Funkraum bleibt die Reihenfolge offen.",
                &[2, 3],
            ),
        ],
    );
    let summary = "Die Rückblende schildert das Treffen an der Antenne. Die Rampe ist Planung. Die Reihenfolge der Treffen an der Schleuse und im Funkraum ist offen.";
    reply(cx, "research-summary", &[(summary, &[0, 1, 2, 3])]);
    wait_for(cx, |cx| {
        workspace.update(cx, |view, _, _| {
            view.ai.pending.is_none()
                && view
                    .ai
                    .answers
                    .iter()
                    .any(|answer| answer.statement == summary)
        })
    });

    for (index, (id, title, scenario, scope, quote)) in sources.into_iter().enumerate() {
        codex_window(cx, &workspace, |window, cx| {
            let source = ("source", index);
            assert_eq!(
                window.find(source).label(),
                Some(format!("{title} · {scenario} · {scope} · {quote}").as_str())
            );
            window.click(source, cx);
        });
        workspace.update(cx, |view, _, cx| {
            assert!(matches!(view.page, Page::Scene));
            assert_eq!(view.selected_scene, Some(id));
            assert_eq!(view.editor.read(cx).selected_text(), quote);
        });
        if index + 1 < sources.len() {
            codex_window(cx, &workspace, |window, cx| {
                window.click("ask-from-scene", cx);
                if id != start {
                    window.click(format!("research-at-{reference}"), cx);
                }
            });
        }
    }
}

#[gpui_kit::test]
fn codex_consent_can_be_revoked_and_model_actions_require_a_connection(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    let settings_dir = tempfile::tempdir().unwrap();
    let preferences = AiPreferences::at(settings_dir.path().to_path_buf());
    let project_id = Project::open(&workspace.root).unwrap().id();
    assert!(project_id.is_some());
    workspace.update(cx, |view, _, _| {
        view.ai.session = AiSession::with_preferences(&preferences);
        view.ai.preferences = Some(AiPreferences::at(settings_dir.path().to_path_buf()));
        view.ai.cli = Some(settings_dir.path().join("codex-fehlt"));
    });

    codex_window(cx, &workspace, |window, cx| {
        window.click(("page", 6_usize), cx);
        assert_eq!(window.find("check-consistency").disabled(), Some(true));
        assert!(window.try_find("enable-codex").is_none());
        window.click("ai-settings", cx);
        assert_eq!(window.find("enable-codex").checked(), Some(false));
        assert_eq!(
            window.find("enable-codex").label(),
            Some("KI für dieses Projekt")
        );
        window.click("enable-codex", cx);
        assert_eq!(window.find("enable-codex").checked(), Some(true));
        assert!(window.find("retry-connection").visible());
        assert!(window.try_find("login-browser").is_none());
    });
    assert!(preferences.is_enabled(&workspace.root, project_id).unwrap());

    codex_window(cx, &workspace, |window, cx| {
        window.click("enable-codex", cx);
        assert_eq!(window.find("enable-codex").checked(), Some(false));
        assert!(window.try_find("retry-connection").is_none());
        window.click("close-settings", cx);
        assert_eq!(window.find("check-consistency").disabled(), Some(true));
        window.click(("page", 4_usize), cx);
        assert_eq!(window.find("ask").disabled(), Some(true));
    });
    assert!(!preferences.is_enabled(&workspace.root, project_id).unwrap());
    workspace.update(cx, |view, _, _| {
        assert_eq!(view.ai.session.status(), &AiStatus::Disabled);
        assert!(view.ai.pending.is_none());
    });
}

#[gpui_kit::test]
fn detail_suggestions_trigger_on_leave_and_stay_with_the_edited_scene(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    cx.simulate_window_resize(workspace.window, size(px(1440.), px(1600.)));
    let project = Project::open(&workspace.root).unwrap();
    let scene = project
        .scenes()
        .find(|scene| scene.title() == "Das Signal")
        .unwrap();
    let id = scene.id();
    let mut world = Knowledge::from_project(&project);
    let target = world.create(
        KnowledgeKind::Place,
        "Station",
        "Die Station ist alt.",
        None,
    );
    let mut store = Store::new(&workspace.root).unwrap();
    world.stage_changes(&project, &mut store).unwrap();
    store.flush().unwrap();

    let home = tempfile::tempdir().unwrap();
    enable_fixture(cx, &workspace, home.path());
    let request = home.path().join("codex/detail-request");
    let other_scene = project
        .scenes()
        .find(|scene| scene.title() == "Der Start")
        .unwrap()
        .id();
    workspace.update(cx, |view, window, cx| {
        view.refresh_project().unwrap();
        view.select_scene(id, window, cx);
        view.editor.update(cx, |editor, cx| {
            let end = editor.value().len();
            editor.set_selected_range(end..end, cx);
            editor.focus(window, cx);
        });
    });
    codex_window(cx, &workspace, |window, cx| {
        window.input("Das Licht erlischt.", cx)
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(!request.exists());
    workspace.update(cx, |view, _, _| assert!(view.ai.pending.is_none()));

    let open_scene = |cx: &mut TestAppContext, scene| {
        codex_window(cx, &workspace, |window, cx| {
            window.click(("page", 1_usize), cx);
            window.click(format!("scene-{scene}"), cx);
            window.click("outline-edit-scene", cx);
        });
    };
    open_scene(cx, other_scene);
    wait_for(cx, |_| request.exists());
    let payload = std::fs::read_to_string(&request).unwrap();
    assert!(payload.contains("Das Licht erlischt."));
    assert!(!payload.contains("Die Sonde startet."));
    let response = serde_json::json!([
        {"quote": "Eine Tür öffnet sich.", "target": target, "text": "Eine Tür geht auf."},
        {"quote": "Danach verstummt die Antenne.", "target": target, "text": "Die Antenne schweigt."}
    ]).to_string();
    let response_path = home.path().join("codex/detail-response");
    let pending_response = response_path.with_extension("tmp");
    std::fs::write(&pending_response, response).unwrap();
    std::fs::rename(pending_response, response_path).unwrap();
    wait_for(cx, |cx| {
        workspace.update(cx, |view, _, _| view.ai.pending.is_none())
    });
    let [accept, discard] = workspace.update(cx, |view, _, _| {
        assert_eq!(view.selected_scene, Some(other_scene));
        assert!(view.ai.details.suggestions(other_scene).is_empty());
        let [accept, discard] = view.ai.details.suggestions(id) else {
            panic!("Beide belegten Vorschläge bleiben bei ihrer Ursprungsszene.");
        };
        [accept.id, discard.id]
    });
    assert_eq!(
        Project::open(&workspace.root)
            .unwrap()
            .document(target)
            .unwrap()
            .body()
            .trim_end(),
        "Die Station ist alt."
    );
    open_scene(cx, id);
    codex_window(cx, &workspace, |window, cx| {
        window.click(format!("drop-detail-{discard}"), cx);
        assert!(window.try_find(format!("open-detail-{discard}")).is_none());
        window.click(format!("open-detail-{accept}"), cx);
    });
    workspace.update(cx, |view, window, cx| {
        assert_eq!(view.ai.excerpt.read(cx).value(), "Eine Tür geht auf.");
        view.ai.excerpt.update(cx, |excerpt, cx| {
            excerpt.set_value("Die Zugangstür öffnet sich.", window, cx);
        });
    });
    codex_window(cx, &workspace, |window, cx| {
        window.click(format!("accept-detail-{accept}"), cx);
        assert!(window.try_find(format!("open-detail-{accept}")).is_none());
    });

    let project = Project::open(&workspace.root).unwrap();
    assert_eq!(
        project.document(target).unwrap().body().trim_end(),
        "Die Station ist alt.\n\nDie Zugangstür öffnet sich."
    );
}

#[gpui_kit::test]
fn consistency_review_saves_the_reason_and_hides_the_dismissed_finding(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    let project = Project::open(&workspace.root).unwrap();
    let citations: Vec<_> = citation_batches(&project)
        .into_iter()
        .flatten()
        .filter(|citation| matches!(citation.title.as_str(), "Das Signal" | "Der Start"))
        .collect();
    let source_index = |quote: &str| {
        citations
            .iter()
            .position(|source| source.anchor.quote() == quote)
            .expect("Textbeleg der Testszene")
    };
    let first = source_index("Eine Tür öffnet sich.");
    let second = source_index("Die Sonde startet.");
    workspace.update(cx, |view, window, cx| {
        view.complete_ai(
            Pending::Consistency(ConsistencyRun {
                batches: vec![citations.clone()],
                left: 0,
                right: 0,
                current: citations,
            }),
            serde_json::json!([{"title": "Widerspruch", "sources": [first, second]}]).to_string(),
            None,
            window,
            cx,
        );
    });
    let reason = "Aus Absicht";
    codex_window(cx, &workspace, |window, cx| {
        window.click(("page", 6_usize), cx);
        assert!(window.find(("source", 10_000_usize)).visible());
        assert!(window.find(("source", 10_001_usize)).visible());
        assert!(window.try_find(("dismiss-finding", 0_usize)).is_none());
        window.click("open-reason", cx);
        window.input(reason, cx);
        window.click(("dismiss-finding", 0_usize), cx);
        assert!(window.try_find(("dismiss-finding", 0_usize)).is_none());
    });

    let project = Project::open(&workspace.root).unwrap();
    let review = Review::from_project(&project);
    assert_eq!(review.decisions().len(), 1);
    let decision = &review.decisions()[0];
    assert_eq!(decision.reason.as_deref(), Some(reason));
    codex_window(cx, &workspace, |window, cx| {
        window.click("marked-findings", cx);
        let row = format!("decision-{}", decision.id);
        assert!(window.find(row.clone()).visible());
        window.click(row, cx);
        assert!(window.find(("source", 10_000_usize)).visible());
        assert!(window.try_find("open-reason").is_none());
    });
}

#[gpui_kit::test]
fn consistency_source_opens_the_timeline_relationship(cx: &mut TestAppContext) {
    let workspace = test_support::open(cx);
    let project = Project::open(&workspace.root).unwrap();
    let review = Review::from_project(&project);
    let timeline_source = review
        .metadata_sources()
        .iter()
        .find(|source| source.source.quote == "100 Jahre später")
        .unwrap()
        .source
        .clone();
    let event = timeline_source.document;
    let target = Timeline::from_project(&project).relations()[0].to;
    let scene = project
        .scenes()
        .find(|scene| scene.title() == "Das Signal")
        .unwrap();
    workspace.update(cx, |view, _, _| {
        view.ai.findings.push(Finding {
            title: "Dauer der Reise".into(),
            sources: vec![
                timeline_source,
                FindingSource {
                    document: scene.id(),
                    quote: "Eine Tür öffnet sich.".into(),
                },
            ],
        });
    });
    codex_window(cx, &workspace, |window, cx| {
        window.click(("page", 6_usize), cx);
        assert_eq!(
            window.find(("source", 10_000_usize)).label(),
            Some("Erstkontakt → Aufbruch")
        );
        window.click(("source", 10_000_usize), cx);
    });
    workspace.update(cx, |view, window, cx| {
        assert!(view.page == Page::Timeline);
        assert_eq!(view.event, Some(event));
        assert_eq!(view.event_target, Some(target));
        assert_eq!(view.relation_time.read(cx).value(), "100 Jahre später");
        assert!(
            view.relation_time
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
}

#[path = "../../tests/support/app_server.rs"]
mod app_server;

fn wait_for(cx: &mut TestAppContext, mut predicate: impl FnMut(&mut TestAppContext) -> bool) {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate(cx) {
        assert!(
            Instant::now() < deadline,
            "Zeitüberschreitung beim lokalen Protokolltest"
        );
        cx.executor().advance_clock(Duration::from_millis(180));
        cx.run_until_parked();
        thread::sleep(Duration::from_millis(5));
    }
}

fn enable_fixture(cx: &mut TestAppContext, workspace: &TestWorkspace, home: &std::path::Path) {
    let preferences = AiPreferences::at(home.into());
    workspace.update(cx, |view, _, _| {
        view.ai.session = AiSession::with_preferences(&preferences);
        view.ai.preferences = Some(preferences);
        view.ai.cli = Some(app_server::mock_executable().into());
    });

    codex_window(cx, workspace, |window, cx| {
        window.click(("page", 7_usize), cx);
        window.click("enable-codex", cx);
    });
    wait_for(cx, |cx| {
        workspace.update(cx, |view, _, _| {
            view.ai.session.status() == &AiStatus::SignInRequired
        })
    });
    codex_window(cx, workspace, |window, cx| window.click("login", cx));
    wait_for(cx, |cx| {
        workspace.update(cx, |view, _, _| {
            view.ai.session.status() == &AiStatus::Ready
        })
    });
}

#[gpui_kit::test]
fn brainstorm_reopens_and_promotes_a_marked_reply_without_duplicate_events(
    cx: &mut TestAppContext,
) {
    let workspace = test_support::open(cx);
    let home = tempfile::tempdir().unwrap();
    let project = Project::open(&workspace.root).unwrap();
    let timeline = Timeline::from_project(&project);
    let event = |title: &str| {
        timeline
            .ordered_events()
            .into_iter()
            .find(|event| event.title == title)
            .unwrap()
            .id
    };
    let pair = [event("Erstkontakt"), event("Aufbruch")];
    let other = event("Fristablauf");

    enable_fixture(cx, &workspace, home.path());

    let prompt = "Was sieht die Figur? Der Leuchtturm ist dunkel.";
    codex_window(cx, &workspace, |window, cx| {
        window.click("close-settings", cx);
        window.click(format!("event-{}", pair[0]), cx);
        window.click("brainstorm-from-event", cx);
        window.click(format!("brainstorm-second-{}", pair[1]), cx);
        let id = workspace.view.read(cx).ai.brainstorm_input.entity_id();
        window.click(("input", id), cx);
        window.input(prompt, cx);
        assert_ne!(window.find("send-chat").disabled(), Some(true));
        window.click("send-chat", cx);

        window.click(("page", 0_usize), cx);
        window.click(format!("event-{other}"), cx);
        window.click("brainstorm-from-event", cx);
        assert!(window.find("interrupt").visible());
        assert!(
            window
                .try_find(format!("brainstorm-second-{}", pair[0]))
                .is_none()
        );
        assert!(window.find(format!("brainstorm-second-{other}")).visible());
    });
    wait_for(cx, |cx| {
        workspace.update(cx, |view, _, _| {
            view.ai
                .brainstorm
                .as_ref()
                .unwrap()
                .messages()
                .iter()
                .any(|message| message.role == Role::Codex)
        })
    });

    let reopened = Project::open(&workspace.root).unwrap();
    let chat = Brainstorm::open(&reopened, pair).unwrap();
    assert_eq!(
        chat.messages()
            .iter()
            .map(|message| (message.role, message.text.as_str()))
            .collect::<Vec<_>>(),
        [(Role::Author, prompt), (Role::Codex, "Die Küste.")]
    );
    assert_eq!(Timeline::from_project(&reopened).ordered_events().len(), 3);
    let message = chat.messages()[1].id;

    codex_window(cx, &workspace, |window, cx| {
        window.click(format!("message-{message}"), cx);
    });
    workspace.update(cx, |view, _, cx| {
        view.ai.answer.update(cx, |answer, cx| {
            answer.set_selected_range(0.."Die Küste.".len(), cx);
        });
    });
    codex_window(cx, &workspace, |window, cx| window.click("mark-idea", cx));
    let reopened = Project::open(&workspace.root).unwrap();
    let chat = Brainstorm::open(&reopened, pair).unwrap();
    let idea = chat.ideas().first().unwrap().id;

    codex_window(cx, &workspace, |window, cx| {
        window.click(("page", 7_usize), cx);
        window.click("enable-codex", cx);
        window.click("close-settings", cx);
        assert_eq!(window.find("send-chat").disabled(), Some(true));
    });
    workspace.update(cx, |view, _, _| {
        assert_eq!(view.ai.session.status(), &AiStatus::Disabled)
    });

    codex_window(cx, &workspace, |window, cx| {
        window.click("choose-project", cx);
        window.click("open", cx);
    });
    let root = workspace.root.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![root]));
    cx.run_until_parked();

    let open_reversed_pair = |cx: &mut TestAppContext| {
        codex_window(cx, &workspace, |window, cx| {
            window.click(format!("event-{}", pair[1]), cx);
            window.click("brainstorm-from-event", cx);
            window.click(format!("brainstorm-second-{}", pair[0]), cx);
            assert!(window.find(format!("message-{message}")).visible());
            window.click(format!("idea-{idea}"), cx);
            window.click(format!("promote-{}-2", pair[0]), cx);
        });
    };
    open_reversed_pair(cx);
    workspace.update(cx, |view, window, cx| {
        view.ai.note.update(cx, |input, cx| {
            input.set_value("Nicht übernehmen", window, cx);
        });
    });
    codex_window(cx, &workspace, |window, cx| {
        assert!(window.try_find("cancel-idea-promotion").is_some());
        window.click("cancel-idea-promotion", cx);
        assert!(window.try_find("promote").is_none());
        assert!(window.find(format!("idea-{idea}")).visible());
    });
    let after_cancel = Project::open(&workspace.root).unwrap();
    let unchanged = Timeline::from_project(&after_cancel);
    assert_eq!(unchanged.ordered_events(), timeline.ordered_events());
    assert_eq!(unchanged.relations(), timeline.relations());
    assert_eq!(
        Brainstorm::open(&after_cancel, [pair[1], pair[0]])
            .unwrap()
            .ideas()[0]
            .quote,
        "Die Küste."
    );

    codex_window(cx, &workspace, |window, cx| {
        window.click(format!("idea-{idea}"), cx);
        window.click(format!("promote-{}-2", pair[0]), cx);
        window.click("promote", cx);
    });
    open_reversed_pair(cx);
    codex_window(cx, &workspace, |window, cx| window.click("promote", cx));
    let after_promote = Project::open(&workspace.root).unwrap();
    let timeline = Timeline::from_project(&after_promote);
    assert_eq!(timeline.ordered_events().len(), 4);
    assert_eq!(timeline.event(idea).unwrap().title, "Die Küste.");
    assert!(
        timeline
            .event(idea)
            .unwrap()
            .body
            .contains(&format!("#nachricht-{message}"))
    );
    assert_eq!(
        timeline
            .relations()
            .iter()
            .filter(|link| {
                link.from == pair[0] && link.to == idea && link.kind == Relation::Cause
            })
            .count(),
        1
    );
}
