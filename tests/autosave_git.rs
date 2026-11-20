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
    fs::write(root.join(file), "Ausgang").unwrap();
    let mut first = Store::new(root).unwrap();
    let mut second = Store::new(root).unwrap();
    first
        .stage(file, Some(b"Ausgang".to_vec()), b"Erste Fassung".to_vec())
        .unwrap();
    second
        .stage(file, Some(b"Ausgang".to_vec()), b"Zweite Fassung".to_vec())
        .unwrap();
    first.flush().unwrap();
    assert!(second.flush().is_err());
    let conflict = second.conflict().unwrap();
    assert_eq!(
        conflict.on_disk.as_deref(),
        Some(b"Erste Fassung".as_slice())
    );
    assert_eq!(conflict.in_app, b"Zweite Fassung");
    second
        .resolve(ConflictResolution::Merged(b"Verbunden".to_vec()))
        .unwrap();
    assert_eq!(fs::read(root.join(file)).unwrap(), b"Verbunden");
}
