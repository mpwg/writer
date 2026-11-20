use std::{fs, path::Path, process::Command};

use tempfile::tempdir;
use writer::{
    Project,
    storage::{ConflictResolution, Store},
};

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn saves_files_before_git_and_preserves_unrelated_staging() {
    let folder = tempdir().unwrap();
    let root = folder.path();
    Project::create(root, "Buch").unwrap();
    git(root, &["config", "user.name", "Schreibtest"]);
    git(root, &["config", "user.email", "test@example.invalid"]);
    let mut store = Store::new(root).unwrap();
    let first = store.checkpoint("Buch angelegt").unwrap();
    let file = Path::new("Manuskript/Szene.md");
    store
        .stage(file, None, "Eine Tür öffnet sich.".as_bytes().to_vec())
        .unwrap();
    store.flush().unwrap();
    assert_eq!(
        fs::read_to_string(root.join(file)).unwrap(),
        "Eine Tür öffnet sich."
    );
    assert_eq!(git(root, &["rev-parse", "HEAD"]), first.id);

    fs::write(root.join("fremd.txt"), "Fremd").unwrap();
    git(root, &["add", "fremd.txt"]);
    fs::write(root.join(".git/index.lock"), "").unwrap();
    assert!(store.checkpoint("Szene geschrieben").is_err());
    assert_eq!(
        fs::read_to_string(root.join(file)).unwrap(),
        "Eine Tür öffnet sich."
    );
    fs::remove_file(root.join(".git/index.lock")).unwrap();
    let second = store.checkpoint("Szene geschrieben").unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(store.checkpoint("Doppelklick").unwrap().id, second.id);
    assert_eq!(git(root, &["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "fremd.txt");
}

#[test]
fn external_changes_require_an_explicit_resolution() {
    let folder = tempdir().unwrap();
    let root = folder.path();
    Project::create(root, "Buch").unwrap();
    let file = Path::new("Manuskript/Szene.md");
    let document = |body| {
        format!("---\nid: 00000000-0000-4000-8000-000000000007\nkind: scene\n---\n{body}")
            .into_bytes()
    };
    fs::write(root.join(file), document("Ausgang")).unwrap();
    let mut first = Store::new(root).unwrap();
    let mut second = Store::new(root).unwrap();
    first
        .stage(file, Some(document("Ausgang")), document("Erste Fassung"))
        .unwrap();
    second
        .stage(file, Some(document("Ausgang")), document("Zweite Fassung"))
        .unwrap();
    first.flush().unwrap();
    assert!(second.flush().is_err());
    let conflict = second.conflict().unwrap();
    assert_eq!(conflict.on_disk, Some(document("Erste Fassung")));
    assert_eq!(conflict.in_app, document("Zweite Fassung"));
    second
        .resolve(ConflictResolution::Merged(document("Verbunden")))
        .unwrap();
    assert_eq!(fs::read(root.join(file)).unwrap(), document("Verbunden"));
}

#[test]
fn merged_document_must_keep_its_identity() {
    let folder = tempdir().unwrap();
    let root = folder.path();
    Project::create(root, "Buch").unwrap();
    let file = Path::new("Manuskript/Szene.md");
    let path = root.join(file);
    let document = |id, body| format!("---\nid: {id}\nkind: scene\n---\n{body}").into_bytes();
    let id = "00000000-0000-4000-8000-000000000007";
    let original = document(id, "Ausgang");
    let internal = document(id, "Interne Fassung");
    let external = document(id, "Externe Fassung");
    fs::write(&path, &external).unwrap();
    let mut store = Store::new(root).unwrap();
    store.stage(file, Some(original), internal.clone()).unwrap();
    assert!(store.flush().is_err());

    let other_scene = document("00000000-0000-4000-8000-000000000077", "Falsche Szene");
    assert!(
        store
            .resolve(ConflictResolution::Merged(other_scene))
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), external);
    assert_eq!(store.conflict().unwrap().in_app, internal);
}
