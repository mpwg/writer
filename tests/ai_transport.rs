#[path = "support/app_server.rs"]
mod app_server;

use app_server::mock_executable;

use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use tempfile::TempDir;
use writer::{
    Id,
    ai::{AiError, AiEvent, AiPreferences, AiRequest, AiSession, AiStatus},
};

#[test]
fn disabled_session_never_starts_the_cli() {
    let directory = TempDir::new().unwrap();
    let preferences = AiPreferences::at(directory.path().join("prefs"));
    let mut session = AiSession::with_preferences(&preferences);

    assert_eq!(session.status(), &AiStatus::Disabled);
    assert_eq!(session.login(), Err(AiError::Disabled));
    assert_eq!(
        session.request(AiRequest::new("Test")),
        Err(AiError::Disabled)
    );
    assert!(session.poll().is_empty());
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
}

#[test]
fn consent_stays_with_the_local_user_and_the_original_book() {
    let directory = TempDir::new().unwrap();
    let book = directory.path().join("book");
    let copy = directory.path().join("copy");
    fs::create_dir(&book).unwrap();
    fs::create_dir(&copy).unwrap();
    let config = directory.path().join("user-one");
    let other = AiPreferences::at(directory.path().join("user-two"));
    let id = Some(Id::from_u128(1));
    let another_book = Some(Id::from_u128(2));

    let preferences = AiPreferences::at(config.clone());
    preferences.set_enabled(&book, id, true).unwrap();

    let preferences = AiPreferences::at(config);
    assert!(preferences.is_enabled(&book, id).unwrap());
    assert!(!preferences.is_enabled(&copy, id).unwrap());
    assert!(!preferences.is_enabled(&book, another_book).unwrap());
    assert!(!preferences.is_enabled(&book, None).unwrap());
    assert!(!other.is_enabled(&book, id).unwrap());
    assert!(fs::read_dir(&book).unwrap().next().is_none());

    preferences.set_enabled(&copy, id, true).unwrap();
    preferences.set_enabled(&book, id, false).unwrap();
    assert!(!preferences.is_enabled(&book, id).unwrap());
    assert!(preferences.is_enabled(&copy, id).unwrap());
}

#[test]
fn missing_cli_leaves_the_session_unavailable() {
    let directory = TempDir::new().unwrap();
    let preferences = AiPreferences::at(directory.path().join("prefs"));
    let mut session = AiSession::with_preferences(&preferences);
    assert_eq!(
        session.enable(Some(&directory.path().join("nonexistent"))),
        Err(AiError::MissingCli)
    );
    assert_eq!(
        session.status(),
        &AiStatus::Unavailable(AiError::MissingCli)
    );
}

#[test]
fn correlates_fragmented_login_responses_and_turns() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let id = session
        .request(
            AiRequest::new("Was sieht die Figur?")
                .with_context("Der Leuchtturm ist dunkel.")
                .with_thread_history(),
        )
        .unwrap();
    let mut received = Vec::new();
    observe(&mut session, |_, events| {
        received.extend_from_slice(events);
        events
            .iter()
            .any(|event| matches!(event, AiEvent::Completed { .. }))
    });
    assert_eq!(
        received,
        vec![
            AiEvent::Delta {
                request_id: id,
                text: "Die Küste.".into()
            },
            AiEvent::Completed {
                request_id: id,
                text: "Die Küste.".into(),
                thread_id: Some("thread-valid".into())
            },
        ]
    );

    let second = session
        .request(AiRequest::new("Weiter").resume("thread-valid"))
        .unwrap();
    observe(&mut session, |_, events| {
        events.iter().any(|event| {
            *event
                == AiEvent::Failed {
                    request_id: second,
                    error: AiError::QuotaExceeded,
                }
        })
    });
}

#[test]
fn identifies_a_lost_thread_before_starting_a_turn() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let id = session
        .request(AiRequest::new("Weiter").resume("missing"))
        .unwrap();
    observe(&mut session, |_, events| {
        events.contains(&AiEvent::Failed {
            request_id: id,
            error: AiError::ThreadUnavailable,
        })
    });
    assert_eq!(session.status(), &AiStatus::Ready);
}

#[test]
fn opt_out_interrupts_the_turn_and_discards_its_answer() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let id = session.request(AiRequest::new("Späte Antwort")).unwrap();
    observe(&mut session, |_, events| {
        events.iter().any(|event| {
            *event
                == AiEvent::Delta {
                    request_id: id,
                    text: "Beginn".into(),
                }
        })
    });
    session.disable();
    assert_eq!(session.status(), &AiStatus::Disabled);
    assert_eq!(
        session.request(AiRequest::new("Test")),
        Err(AiError::Disabled)
    );
    let interrupt = fixture.preferences_dir().join("codex").join("interrupted");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !interrupt.exists() && Instant::now() < deadline {
        assert!(session.poll().is_empty());
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        interrupt.exists(),
        "the server did not receive turn/interrupt"
    );
    assert!(session.poll().is_empty());
}

#[test]
fn interrupts_a_turn_without_disabling_the_project() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let id = session.request(AiRequest::new("Späte Antwort")).unwrap();
    observe(&mut session, |_, events| {
        events.iter().any(|event| {
            *event
                == AiEvent::Delta {
                    request_id: id,
                    text: "Beginn".into(),
                }
        })
    });
    session.interrupt().unwrap();
    observe(&mut session, |status, events| {
        *status == AiStatus::Ready
            && events.contains(&AiEvent::Failed {
                request_id: id,
                error: AiError::Interrupted,
            })
    });
    assert!(
        fixture
            .preferences_dir()
            .join("codex")
            .join("interrupted")
            .exists()
    );
    assert!(session.request(AiRequest::new("Weitere Frage")).is_ok());
}

#[test]
fn a_closed_process_fails_an_active_request() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let id = session.request(AiRequest::new("Prozess beenden")).unwrap();
    observe(&mut session, |status, events| {
        *status == AiStatus::Unavailable(AiError::ProcessEnded)
            && events.contains(&AiEvent::Failed {
                request_id: id,
                error: AiError::ProcessEnded,
            })
    });
}

#[test]
fn cancels_browser_login_and_ignores_its_completion() {
    let fixture = Fixture::new();
    let mut session = fixture.connect();
    fs::write(fixture.preferences_dir().join("codex").join("cancel"), "").unwrap();
    session.login().unwrap();
    observe(&mut session, |_, events| {
        events
            .iter()
            .any(|event| matches!(event, AiEvent::LoginUrl(_)))
    });
    session.cancel_login().unwrap();
    observe(&mut session, |status, events| {
        *status == AiStatus::SignInRequired
            && events.contains(&AiEvent::ConnectionError(AiError::LoginCancelled))
    });
    assert_eq!(
        session.request(AiRequest::new("Test")),
        Err(AiError::NotReady)
    );
}

fn observe(session: &mut AiSession, mut expected: impl FnMut(&AiStatus, &[AiEvent]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let events = session.poll();
        if expected(session.status(), &events) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "unexpected session status: {:?}; recent events: {:?}",
            session.status(),
            events
        );
        thread::sleep(Duration::from_millis(5));
    }
}

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Self {
        Self(TempDir::new().unwrap())
    }

    fn preferences_dir(&self) -> PathBuf {
        self.0.path().join("prefs")
    }

    fn connect(&self) -> AiSession {
        let mut session = AiSession::with_preferences(&AiPreferences::at(self.preferences_dir()));
        session.enable(Some(mock_executable())).unwrap();
        observe(&mut session, |status, _| {
            *status == AiStatus::SignInRequired
        });
        session
    }

    fn session(&self) -> AiSession {
        let mut session = self.connect();
        session.login().unwrap();
        let mut saw_url = false;
        observe(&mut session, |status, events| {
            saw_url |= events.iter().any(|event| {
                matches!(event, AiEvent::LoginUrl(url) if url == "https://auth.openai.com/oauth/authorize?test=yes")
            });
            *status == AiStatus::Ready && saw_url
        });
        session
    }
}
