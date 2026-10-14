//! The editor's frontend tests use the session view of the reference example
//! (editor/tests/fixtures/authentication.view.json). This keeps that fixture
//! identical to what the session actually sends. Regenerate with
//! `UPDATE_FIXTURES=1 cargo test -p mdg-session --test fixture`.

use std::path::Path;

use mdg_core::create_authentication_example;
use mdg_session::Session;

#[test]
fn editor_fixture_matches_the_session_view() {
    let mut session = Session::new();
    session.load(create_authentication_example(), "authentication.mdg", None);
    let json = format!("{}\n", serde_json::to_string_pretty(&session.view()).unwrap());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../editor/tests/fixtures/authentication.view.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(current == json, "stale fixture; rerun with UPDATE_FIXTURES=1");
}
