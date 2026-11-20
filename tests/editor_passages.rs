use writer::{
    Id,
    passage::{SceneText, TextError},
};

#[test]
fn replacing_inside_a_concept_preserves_its_id_through_reopen_and_undo() {
    let id = Id::from_u128(1);
    let original = "Eine Tür öffnet sich.\n\n- Gewöhnliche Liste\n";
    let mut scene = SceneText::new(id, original);
    let sentence = scene.anchor(0.."Eine Tür öffnet sich.".len()).unwrap();
    scene.set_concept(&sentence, true).unwrap();
    let marked = scene.markdown();
    let concept_id = scene.concepts()[0].id;
    let word = scene.anchor(5..9).unwrap();
    scene.replace(&word, "Pforte").unwrap();
    assert_eq!(
        scene.text(),
        "Eine Pforte öffnet sich.\n\n- Gewöhnliche Liste\n"
    );
    assert_eq!(scene.concepts()[0].id, concept_id);
    assert_eq!(
        &scene.text()[scene.concepts()[0].range.clone()],
        "Eine Pforte öffnet sich."
    );
    let reopened = SceneText::from_markdown(id, &scene.markdown());
    assert_eq!(reopened.text(), scene.text());
    assert_eq!(reopened.concepts(), scene.concepts());
    assert!(scene.undo());
    assert_eq!(scene.markdown(), marked);
    assert!(scene.undo());
    assert_eq!(scene.markdown(), original);
}

#[test]
fn rejects_selection_inside_a_combining_grapheme_and_formats_whole_graphemes() {
    let mut scene = SceneText::new(Id::from_u128(1), "„Cafe\u{301}“\nNächste Zeile");
    assert_eq!(scene.anchor(3..7).unwrap_err(), TextError::InvalidRange);
    let word = scene.anchor(3..9).unwrap();
    scene
        .format(&word, writer::passage::TextFormat::Bold)
        .unwrap();
    assert_eq!(scene.text(), "„**Cafe\u{301}**“\nNächste Zeile");
}

#[test]
fn stale_anchors_follow_a_unique_quotation_but_reject_ambiguous_copies() {
    let id = Id::from_u128(1);
    let mut scene = SceneText::new(id, "Vorher „Äpfel“ nachher");
    let word = scene.anchor(10..16).unwrap();
    scene
        .edit("Ein neuer Absatz\n\nVorher „Äpfel“ nachher")
        .unwrap();
    scene.replace(&word, "Birnen").unwrap();
    assert_eq!(scene.text(), "Ein neuer Absatz\n\nVorher „Birnen“ nachher");

    let mut copied = SceneText::new(id, "Vorher „Äpfel“ nachher Vorher „Äpfel“ nachher");
    assert_eq!(
        copied.replace(&word, "Birnen"),
        Err(TextError::AmbiguousAnchor)
    );
    assert_eq!(
        scene.replace(&word, "Birnen"),
        Err(TextError::MissingAnchor)
    );
}

#[test]
fn removing_the_middle_of_a_concept_preserves_the_pieces() {
    let mut scene = SceneText::new(Id::from_u128(1), "AAA BBB CCC");
    let all = scene.anchor(0..11).unwrap();
    scene.set_concept(&all, true).unwrap();
    let original = scene.concepts()[0].id;
    let middle = scene.anchor(4..7).unwrap();
    scene.set_concept(&middle, false).unwrap();
    assert_eq!(scene.concepts()[0].id, original);
    assert_eq!(scene.concepts()[0].range, 0..4);
    assert_eq!(scene.concepts()[1].range, 7..11);
    assert_ne!(scene.concepts()[0].id, scene.concepts()[1].id);
    assert_eq!(scene.text(), "AAA BBB CCC");
}

#[test]
fn formatting_before_an_empty_concept_preserves_both_passage_boundaries() {
    let id = Id::from_u128(1);
    let mut scene = SceneText::new(id, "ABCDEF");
    let left = scene.anchor(0..3).unwrap();
    let right = scene.anchor(3..6).unwrap();
    scene.set_concept(&left, true).unwrap();
    scene.set_concept(&right, true).unwrap();
    let right_id = scene.concepts()[1].id;
    scene.replace(&right, "").unwrap();
    scene
        .format(&left, writer::passage::TextFormat::Bold)
        .unwrap();
    let reopened = SceneText::from_markdown(id, &scene.markdown());
    assert_eq!(reopened.text(), "**ABC**");
    assert_eq!(reopened.concepts()[0].range, 0..7);
    assert_eq!(reopened.concepts()[1].range, 7..7);
    assert_eq!(reopened.concepts()[1].id, right_id);
}
