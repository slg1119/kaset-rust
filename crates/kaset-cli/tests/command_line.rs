use std::process::Command;

#[cfg(debug_assertions)]
#[test]
fn missing_session_file_exits_with_sign_in_instructions() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("missing-session.txt");
    let result = Command::new(env!("CARGO_BIN_EXE_kaset"))
        .arg("whoami")
        .arg("--session-file")
        .arg(&path)
        .output()
        .unwrap();

    assert_eq!(result.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(result.stderr).unwrap().trim_end(),
        "Not signed in. Sign in with the Kaset app first."
    );
    assert!(result.stdout.is_empty());
    assert!(!path.exists());
}

#[cfg(debug_assertions)]
#[test]
fn expired_debug_session_is_read_without_modification() {
    use kaset_session::{FileStore, SESSION_FILE_ENV, STORE_ENV, SessionStore};
    use ytm_core::secret::Secret;

    let temp = tempfile::tempdir().unwrap();
    let path = kaset_session::dev_session_path(temp.path());
    let store = FileStore::new(path.clone());
    let session = Secret::new("SID=test-cookie");
    store.save(&session).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_kaset"))
        .arg("whoami")
        .env(STORE_ENV, "file")
        .env(SESSION_FILE_ENV, &path)
        .output()
        .unwrap();

    assert_eq!(result.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(result.stderr).unwrap().trim_end(),
        "Session expired. Open the Kaset app to sign in again."
    );
    assert!(result.stdout.is_empty());
    assert_eq!(store.load().unwrap(), Some(session));
}

#[cfg(not(debug_assertions))]
#[test]
fn task12_release_rejects_session_file_flag() {
    let result = Command::new(env!("CARGO_BIN_EXE_kaset"))
        .args(["whoami", "--session-file", "test-session.txt"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("unexpected argument '--session-file'")
    );
    assert!(result.stdout.is_empty());
}
