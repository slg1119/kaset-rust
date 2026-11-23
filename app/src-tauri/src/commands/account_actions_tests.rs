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
