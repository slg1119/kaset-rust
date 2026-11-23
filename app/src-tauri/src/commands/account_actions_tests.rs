use super::{FeedbackCache, LibraryAction, LibraryEdit, validate_playlist_input};
use ytm_core::models::SongMetadata;
use ytm_core::secret::Secret;

#[test]
fn feedback_is_scoped_to_video_action_and_session() {
    let mut cache = FeedbackCache::default();
    let metadata = SongMetadata {
        like_status: None,
        in_library: false,
        add_token: Some(Secret::new("test-add_-%3D=+/.")),
        remove_token: Some(Secret::new("test?invalid")),
    };
    assert!(cache.token(1, "song-id", LibraryAction::Add).is_err());
    let visible = cache.insert(1, "song-id", metadata);
    assert!(visible.can_add);
    assert!(!visible.can_remove);
    assert_eq!(
        serde_json::to_value(&visible).unwrap(),
        serde_json::json!({"like_status":null,"in_library":false,"can_add":true,"can_remove":false})
    );
    assert!(cache.token(1, "other-id", LibraryAction::Add).is_err());
    assert!(cache.token(1, "song-id", LibraryAction::Remove).is_err());
    assert_eq!(
        cache
            .token(1, "song-id", LibraryAction::Add)
            .unwrap()
            .expose(),
        "test-add_-%3D=+/."
    );
    assert!(cache.token(2, "song-id", LibraryAction::Add).is_err());
    assert!(cache.token(1, "song-id", LibraryAction::Add).is_err());
}

#[test]
fn playlist_input_checks_characters_and_ids() {
    let title = "가".repeat(150);
    let description = "나".repeat(5000);
    assert!(validate_playlist_input(&title, &description, &["song-id".into()]).is_ok());
    assert!(validate_playlist_input(&format!("{title}a"), "", &[]).is_err());
    assert!(validate_playlist_input("Title", &format!("{description}a"), &[]).is_err());
    assert!(validate_playlist_input(" ", "", &[]).is_err());
    assert!(validate_playlist_input("Title", "line\nline", &[]).is_ok());
    assert!(validate_playlist_input("Title", "", &["bad/id".into()]).is_err());
}

#[test]
fn library_edit_does_not_accept_a_token_from_ipc() {
    assert!(
        serde_json::from_value::<LibraryEdit>(
            serde_json::json!({"video_id":"song-id","action":"remove","token":"test-token"})
        )
        .is_err()
    );
}

#[test]
fn successful_edit_consumes_the_cached_tokens_for_only_that_video() {
    let mut cache = FeedbackCache::default();
    for id in ["one", "two"] {
        cache.insert(
            5,
            id,
            SongMetadata {
                like_status: None,
                in_library: true,
                add_token: Some(Secret::new("test-add")),
                remove_token: Some(Secret::new("test-remove")),
            },
        );
    }
    assert!(cache.token(5, "one", LibraryAction::Add).is_err());
    assert!(cache.token(5, "one", LibraryAction::Remove).is_ok());
    cache.remove("one");
    assert!(cache.token(5, "one", LibraryAction::Remove).is_err());
    assert!(cache.token(5, "two", LibraryAction::Remove).is_ok());
}

fn signed_in_state() -> crate::state::AppState {
    use crate::session::{Session, store::MemoryStore};
    use ytm_core::auth::CookieJar;

    let session = Session::new(Box::new(MemoryStore::default()));
    session
        .sign_in(CookieJar::from_header("SAPISID=test-cookie"))
        .unwrap();
    crate::state::AppState::new(session)
}

fn feedback_metadata() -> SongMetadata {
    SongMetadata {
        like_status: None,
        in_library: false,
        add_token: Some(Secret::new("test-add")),
        remove_token: None,
    }
}

#[tokio::test]
async fn task12_next_request_does_not_hold_feedback_lock() {
    let state = signed_in_state();
    let epoch = state.session.epoch();
    let result = super::load_song_metadata(&state, epoch, "song-id", async {
        assert!(state.feedback.try_lock().is_ok());
        Ok(feedback_metadata())
    })
    .await
    .unwrap();
    assert!(result.can_add);
}

#[tokio::test]
async fn task12_sign_out_immediately_clears_feedback_tokens() {
    let state = signed_in_state();
    let epoch = state.session.epoch();
    state
        .feedback
        .lock()
        .await
        .insert(epoch, "song-id", feedback_metadata());
    crate::commands::finish_sign_out(&state, async { state.session.sign_out() })
        .await
        .unwrap();
    // Inspect entries before a lookup on the new epoch can clear them lazily.
    assert!(state.feedback.lock().await.entries.is_empty());
}

#[tokio::test]
async fn stale_next_response_cannot_restore_tokens_after_sign_out_or_an_edit() {
    for sign_out in [false, true] {
        let state = signed_in_state();
        let epoch = state.session.epoch();
        let result = super::load_song_metadata(&state, epoch, "song-id", async {
            if sign_out {
                crate::commands::finish_sign_out(&state, async { state.session.sign_out() })
                    .await
                    .unwrap();
            } else {
                let mut cache = state.feedback.lock().await;
                cache.select_epoch(epoch);
                cache.edited("song-id");
            }
            Ok(feedback_metadata())
        })
        .await;
        assert!(result.is_err());
        assert!(state.feedback.lock().await.entries.is_empty());
    }
}

#[test]
fn task13_playlist_creation_caps_the_video_count() {
    let mut videos = vec!["test-vid001".to_owned(); 500];
    assert!(validate_playlist_input("Title", "", &videos).is_ok());
    videos.push("test-vid002".into());
    let error = validate_playlist_input("Title", "", &videos).unwrap_err();
    assert_eq!(error.kind, crate::error::ErrorKind::InvalidArgument);
}

#[tokio::test]
async fn task13_sign_out_finishes_while_the_library_edit_request_is_pending() {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::Notify;
    use tokio::time::timeout;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};
    use ytm_core::auth::CookieJar;
    use ytm_core::client::{ClientConfig, InnerTubeClient};

    let state = Arc::new(signed_in_state());
    let epoch = state.session.epoch();
    let video_id = "test-vid001";
    state
        .feedback
        .lock()
        .await
        .insert(epoch, video_id, feedback_metadata());

    let server = MockServer::start().await;
    let received = Arc::new(Notify::new());
    Mock::given(method("POST"))
        .and(path("/feedback"))
        .and(body_partial_json(
            serde_json::json!({"feedbackTokens": ["test-add"]}),
        ))
        .respond_with({
            let received = Arc::clone(&received);
            move |_: &Request| {
                received.notify_one();
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({}))
                    .set_delay(Duration::from_secs(1))
            }
        })
        .expect(1)
        .mount(&server)
        .await;

    let mut config = ClientConfig::music("test-key");
    config.base_url = server.uri();
    let client = InnerTubeClient::new(config, state.session.cookies_for_epoch(epoch)).unwrap();
    let pending = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            let edit = LibraryEdit {
                video_id: video_id.into(),
                action: LibraryAction::Add,
            };
            super::send_library_edit(&state, &client, epoch, &edit).await
        }
    });

    timeout(Duration::from_secs(5), received.notified())
        .await
        .unwrap();
    timeout(
        Duration::from_millis(500),
        crate::commands::finish_sign_out(&state, async { state.session.sign_out() }),
    )
    .await
    .expect("sign-out waited on the pending library edit")
    .unwrap();
    assert!(!pending.is_finished());
    assert!(state.feedback.lock().await.entries.is_empty());

    // A late response must leave the same song's new-session token alone.
    state
        .session
        .sign_in(CookieJar::from_header("SAPISID=test-new-cookie"))
        .unwrap();
    let new_epoch = state.session.epoch();
    state
        .feedback
        .lock()
        .await
        .insert(new_epoch, video_id, feedback_metadata());
    assert_eq!(
        pending.await.unwrap().unwrap_err().kind,
        crate::error::ErrorKind::NotSignedIn
    );
    assert!(
        state
            .feedback
            .lock()
            .await
            .token(new_epoch, video_id, LibraryAction::Add)
            .is_ok()
    );
}
