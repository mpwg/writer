use std::{fs, path::Path};

use writer::{
    Id, Project,
    export::{ExportError, ExportFormat, export},
    manuscript::Manuscript,
    passage::TextError,
};

const FIRST: u128 = 20;
const SECOND: u128 = 10;

fn document(root: &Path, file: &str, id: u128, kind: &str, metadata: &str, body: &str) {
    let id = Id::from_u128(id);
    fs::write(
        root.join("Manuskript").join(file),
        format!("---\nid: {id}\nkind: {kind}\n{metadata}---\n{body}"),
    )
    .unwrap();
}

fn book(root: &Path) -> Manuscript {
    Project::create(root, "Über den Fluss").unwrap();
    let first = Id::from_u128(FIRST);
    let second = Id::from_u128(SECOND);
    let earlier = Id::from_u128(100);
    let later = Id::from_u128(101);
    document(
        root,
        "A.md",
        30,
        "chapter",
        &format!("scenes: [{first}]\n"),
        "",
    );
    document(
        root,
        "B.md",
        31,
        "chapter",
        &format!("scenes: [{second}]\n"),
        "",
    );
    document(
        root,
        "Der Aufbruch.md",
        FIRST,
        "scene",
        &format!("scene_timing: {{after: {later}}}\n"),
        "Ein Schiff fährt hinaus.\n",
    );
    document(
        root,
        "Die Rückblende.md",
        SECOND,
        "scene",
        &format!("scene_timing: {{after: {earlier}}}\n"),
        "Die Tür öffnet sich.\n",
    );
    Manuscript::from_project(&Project::open(root).unwrap())
}

#[test]
fn an_edit_in_the_second_scene_has_one_changed_document() {
    let temp = tempfile::tempdir().unwrap();
    let mut book = book(temp.path());
    let second = Id::from_u128(SECOND);
    let target = book.scene(second).unwrap().text.anchor(4..8).unwrap();
    book.replace(&target, &target, "Pforte").unwrap();
    assert_eq!(book.changed_scene_ids().collect::<Vec<_>>(), [second]);
    assert_eq!(
        book.scene(second).unwrap().text.text(),
        "Die Pforte öffnet sich.\n"
    );
}

#[test]
fn a_scene_repeated_in_the_chapter_plan_has_one_editable_span() {
    let temp = tempfile::tempdir().unwrap();
    let _ = book(temp.path());
    let first = Id::from_u128(FIRST);
    let second = Id::from_u128(SECOND);
    document(
        temp.path(),
        "B.md",
        31,
        "chapter",
        &format!("scenes: [{first}, {second}]\n"),
        "",
    );
    let mut manuscript = Manuscript::from_project(&Project::open(temp.path()).unwrap());
    assert_eq!(manuscript.chapters()[0].scenes, [first]);
    assert_eq!(manuscript.chapters()[1].scenes, [second]);
    manuscript
        .edit_scene(first, "Ein anderer Aufbruch.")
        .unwrap();
    assert_eq!(manuscript.changed_scene_ids().collect::<Vec<_>>(), [first]);
}

#[test]
fn one_undo_restores_both_scenes_after_a_cross_scene_replace() {
    let temp = tempfile::tempdir().unwrap();
    let mut book = book(temp.path());
    let first = Id::from_u128(FIRST);
    let second = Id::from_u128(SECOND);
    let from = book.scene(first).unwrap().text.anchor(4..10).unwrap();
    let to = book.scene(second).unwrap().text.anchor(4..8).unwrap();
    book.replace(&from, &to, "Boot").unwrap();
    assert_eq!(
        book.changed_scene_ids().collect::<Vec<_>>(),
        [first, second]
    );
    assert_eq!(book.scene(first).unwrap().text.text(), "Ein Boot");
    assert_eq!(book.scene(second).unwrap().text.text(), " öffnet sich.\n");
    assert!(book.undo());
    assert_eq!(
        book.scene(first).unwrap().text.text(),
        "Ein Schiff fährt hinaus.\n"
    );
    assert_eq!(
        book.scene(second).unwrap().text.text(),
        "Die Tür öffnet sich.\n"
    );
    assert_eq!(book.changed_scene_ids().count(), 0);

    assert_eq!(
        book.replace(&to, &from, "Boot"),
        Err(TextError::InvalidRange)
    );
    assert_eq!(book.changed_scene_ids().count(), 0);
}

#[test]
fn both_exports_keep_chapter_order_and_show_concepts_with_umlauts() {
    let temp = tempfile::tempdir().unwrap();
    let mut book = book(temp.path());
    let second = Id::from_u128(SECOND);
    let source = "\n\n#panic(\"Wortwörtlich\")\n\n".to_owned()
        + &"Die Wellen glitzern. Eine Möwe überquert den Fluss.\n\n".repeat(100)
        + "Schlusswort";
    let second_text = &book.scene(second).unwrap().text;
    let start = second_text.anchor(0.."Die Tür öffnet sich.".len()).unwrap();
    let end = second_text
        .anchor(second_text.text().len()..second_text.text().len())
        .unwrap();
    book.set_concept(&start, true).unwrap();
    book.replace(&end, &end, &source).unwrap();

    let md = temp.path().join("Buch.md");
    export(&book, ExportFormat::Markdown, &md).unwrap();
    let md = fs::read_to_string(md).unwrap();
    assert!(md.contains("# A"));
    assert!(md.contains("# B"));
    assert!(!md.contains("writer:concept"));
    assert!(md.contains("Konzept"));
    assert!(md.find("Schiff").unwrap() < md.find("Tür").unwrap());

    let pdf = temp.path().join("Buch.pdf");
    export(&book, ExportFormat::Pdf, &pdf).unwrap();
    let pages = pdf_extract::extract_text_by_pages(&pdf).unwrap();
    assert!(pages.len() > 1, "Das Kapitel soll über Seiten umbrechen");
    let text = pages.join("\n");
    assert!(text.contains("Über den Fluss"), "{text}");
    assert!(text.contains("Konzept"));
    assert!(text.contains("Wortwörtlich"));
    assert!(text.contains("Schlusswort"));
    assert!(text.find("Schiff").unwrap() < text.find("Tür").unwrap());
}

#[test]
fn failed_pdf_export_leaves_the_previous_target_byte_for_byte() {
    let temp = tempfile::tempdir().unwrap();
    let mut book = book(temp.path());
    let deeply_nested = "> ".repeat(64) + "Tiefe Verschachtelung";
    book.edit_scene(Id::from_u128(SECOND), &deeply_nested)
        .unwrap();
    let pdf = temp.path().join("Buch.pdf");
    let previous = "Unveränderte bisherige Ausgabe".as_bytes();
    fs::write(&pdf, previous).unwrap();
    assert!(matches!(
        export(&book, ExportFormat::Pdf, &pdf),
        Err(ExportError::MarkdownDepth)
    ));
    assert_eq!(fs::read(pdf).unwrap(), previous);
}
