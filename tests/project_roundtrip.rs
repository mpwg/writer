use std::{fs, path::Path};

use tempfile::tempdir;
use writer::{Id, Project, ProjectIssue};

const SCENE: &str = "00000000-0000-4000-8000-000000000001";
const LOST: &str = "00000000-0000-4000-8000-000000000002";
const CHAPTER: &str = "00000000-0000-4000-8000-000000000003";
const BODY: &str = "Eine Tür öffnet sich.";

fn write_document(root: &Path, name: &str, metadata: &str, body: &str) {
    fs::write(
        root.join("Manuskript").join(name),
        format!("---\n{metadata}\n---\n{body}"),
    )
    .unwrap();
}

#[test]
fn opens_remaining_scene_after_external_rename_and_removal() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("Buch");
    Project::create(&root, "Über den Fluss").unwrap();
    write_document(
        &root,
        "Tür.md",
        &format!("id: {SCENE}\nkind: scene\ncustom: erhalten"),
        BODY,
    );
    write_document(
        &root,
        "Ende.md",
        &format!("id: {LOST}\nkind: scene"),
        "Ende",
    );
    write_document(
        &root,
        "Kapitel.md",
        &format!("id: {CHAPTER}\nkind: chapter\nscenes: [{SCENE}, {LOST}]"),
        "",
    );
    let original = fs::read(root.join("Manuskript/Tür.md")).unwrap();
    let moved = temp.path().join("Buch verschoben");
    fs::rename(root, &moved).unwrap();
    fs::remove_file(moved.join("Manuskript/Ende.md")).unwrap();

    let reopened = Project::open(&moved).unwrap();
    let lost: Id = LOST.parse().unwrap();
    assert_eq!(reopened.title(), "Über den Fluss");
    assert_eq!(reopened.scene(SCENE.parse().unwrap()).unwrap().body(), BODY);
    assert!(reopened.has_missing_ref(lost));
    assert_eq!(
        reopened
            .document(CHAPTER.parse().unwrap())
            .unwrap()
            .scenes(),
        &[SCENE.parse().unwrap(), lost]
    );
    assert_eq!(fs::read(moved.join("Manuskript/Tür.md")).unwrap(), original);
}

#[test]
fn reports_ambiguous_ids_and_broken_files_without_hiding_intact_scenes() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    Project::create(root, "Testbuch").unwrap();
    for file in ["Ende.md", "Kopie.md"] {
        write_document(root, file, &format!("id: {LOST}\nkind: scene"), "");
    }
    write_document(root, "Kaputt.md", "id: [", "");
    write_document(root, "Tür.md", &format!("id: {SCENE}\nkind: scene"), BODY);
    write_document(
        root,
        "Kapitel.md",
        &format!("id: {CHAPTER}\nkind: chapter\nscenes: [{LOST}]"),
        "",
    );
    let project = Project::open(root).unwrap();
    let lost = LOST.parse().unwrap();
    assert!(project.scene(lost).is_none());
    assert!(!project.has_missing_ref(lost));
    assert_eq!(project.scene(SCENE.parse().unwrap()).unwrap().body(), BODY);
    assert!(project.issues().iter().any(|issue| matches!(issue, ProjectIssue::InvalidDocument { path, .. } if path.ends_with("Kaputt.md"))));
    assert!(project.issues().iter().any(|issue| matches!(issue, ProjectIssue::AmbiguousReference { path, target } if path.ends_with("Kapitel.md") && *target == lost)));
}

#[test]
fn rejects_nonempty_destination_and_keeps_existing_bytes() {
    let temp = tempdir().unwrap();
    let existing = temp.path().join("unverändert.txt");
    fs::write(&existing, b"Original").unwrap();
    assert!(Project::create(temp.path(), "Testbuch").is_err());
    assert_eq!(fs::read(existing).unwrap(), b"Original");
    assert!(!temp.path().join("project.yaml").exists());
}
