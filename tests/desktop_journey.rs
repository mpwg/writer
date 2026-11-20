use std::{fs, process::Command};

use tempfile::tempdir;
use writer::{
    Project,
    export::{ExportFormat, export},
    manuscript::Manuscript,
    outline::Outline,
    storage::Store,
    timeline::{Relation, Timeline},
};

#[test]
fn offline_book_journey_preserves_story_time_and_exports_in_chapter_order() {
    let folder = tempdir().unwrap();
    let book = folder.path().join("Das verlorene Imperium");
    let project = Project::create(&book, "Das verlorene Imperium").unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&book)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    git(&["config", "user.name", "Abnahme"]);
    git(&["config", "user.email", "abnahme@example.invalid"]);
    let mut store = Store::new(&book).unwrap();
    let mut timeline = Timeline::from_project(&project);
    let contact = timeline.create_event("Erstkontakt", "Die Station fängt ein Signal auf.", None);
    let departure = timeline.create_event("Aufbruch", "Die Sonde startet.", None);
    timeline
        .relate(
            contact,
            departure,
            Relation::Before,
            "Das Signal löst den Start aus.".into(),
        )
        .unwrap();
    for event in [contact, departure] {
        timeline.stage_event(event, &project, &mut store).unwrap();
    }
    store.flush().unwrap();

    let project = Project::open(&book).unwrap();
    let mut outline = Outline::from_project(&project);
    let first = outline.create_chapter("Aufbruch");
    let second = outline.create_chapter("Rückblende");
    let flashback = outline.derive_scene(contact, "Die Antenne bewegt sich.");
    let launch = outline.derive_scene(departure, "Die Sonde startet.");
    outline.place(flashback, second, 0).unwrap();
    outline.place(launch, first, 0).unwrap();
    outline.stage_changes(&project, &mut store).unwrap();
    store.flush().unwrap();

    let project = Project::open(&book).unwrap();
    let mut manuscript = Manuscript::from_project(&project);
    let text = &manuscript.scene(flashback).unwrap().text;
    let concept = text.anchor(0..text.text().len()).unwrap();
    manuscript.set_concept(&concept, true).unwrap();
    let doc = project.scene(flashback).unwrap();
    store
        .stage_body(
            &store.relative_path(doc).unwrap(),
            doc,
            &manuscript.scene(flashback).unwrap().text.markdown(),
        )
        .unwrap();
    store.flush().unwrap();
    assert!(git(&["status", "--porcelain"]).contains("Manuskript/"));
    assert!(store.checkpoint("Erste Fassung").unwrap().created);
    assert!(!store.checkpoint("Doppelklick").unwrap().created);
    assert_eq!(git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    assert!(!git(&["ls-files"]).contains(".writer.lock"));

    let reopened = Project::open(&book).unwrap();
    assert_eq!(
        Outline::from_project(&reopened)
            .scene(flashback)
            .unwrap()
            .event,
        Some(contact)
    );
    let manuscript = Manuscript::from_project(&reopened);
    assert_eq!(
        manuscript
            .chapters()
            .iter()
            .flat_map(|chapter| chapter.scenes.iter().copied())
            .collect::<Vec<_>>(),
        [launch, flashback]
    );
    let markdown = folder.path().join("Imperium.md");
    let pdf = folder.path().join("Imperium.pdf");
    export(&manuscript, ExportFormat::Markdown, &markdown).unwrap();
    export(&manuscript, ExportFormat::Pdf, &pdf).unwrap();
    for output in [
        fs::read_to_string(markdown).unwrap(),
        pdf_extract::extract_text(pdf).unwrap(),
    ] {
        assert!(
            output.find("Die Sonde startet.").unwrap()
                < output.find("Die Antenne bewegt sich.").unwrap()
        );
        assert!(output.contains("Konzept"));
    }
}
