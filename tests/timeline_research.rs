use writer::{
    Id,
    ai::research::{Evidence, Scenario, TemporalScope},
    timeline::{Relation, Timeline},
};

#[test]
fn maps_evidence_by_chronology_and_exposes_unordered_ideas() {
    let mut timeline = Timeline::default();
    let earlier = timeline.create_event("Erstkontakt", "", None);
    let later = timeline.create_event("Rückkehr", "", None);
    let question = timeline.create_event("Aufbruch", "", None);
    let maybe = timeline.create_event("Begegnung", "", None);
    timeline
        .relate(earlier, later, Relation::Before, String::new())
        .unwrap();
    timeline
        .relate(later, question, Relation::Before, String::new())
        .unwrap();
    let evidence = |event, scenario| Evidence {
        document: Id::new_v4(),
        event: Some(event),
        scenario,
    };
    let mut evidence = vec![
        evidence(earlier, Scenario::Prose),
        evidence(later, Scenario::Planning),
        evidence(maybe, Scenario::OpenIdea),
    ];
    writer::ai::research::sort_evidence(&mut evidence, &timeline, Some(question));
    assert_eq!(
        evidence.iter().map(|entry| entry.event).collect::<Vec<_>>(),
        [Some(later), Some(earlier), Some(maybe)]
    );
    assert_eq!(
        evidence[2].scope(&timeline, Some(question)),
        TemporalScope::Unordered
    );
    assert_eq!(evidence[2].scenario, Scenario::OpenIdea);
}
