use writer::{
    Id,
    ai::details::{DetailQueue, Trigger},
    passage::SceneText,
};

#[test]
fn only_leave_with_a_new_revision_or_an_explicit_request_triggers() {
    let mut queue = DetailQueue::default();
    let id = Id::new_v4();
    assert!(!queue.should_request(Trigger::Leave, id, 1));
    queue.set_enabled(true);
    queue.open(id, 1);
    assert!(!queue.should_request(Trigger::Pause, id, 2));
    assert!(queue.should_request(Trigger::Leave, id, 2));
    assert!(!queue.should_request(Trigger::Leave, id, 2));
    assert!(queue.should_request(Trigger::Manual, id, 2));
    queue.set_enabled(false);
    assert!(!queue.should_request(Trigger::Manual, id, 3));
    let text = SceneText::new(id, "Der Antennenmast fällt.");
    assert!(queue.suggestions(id).is_empty());
    assert_eq!(text.text(), "Der Antennenmast fällt.");
}
