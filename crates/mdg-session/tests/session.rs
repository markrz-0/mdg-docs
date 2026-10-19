use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdg_core::*;
use mdg_session::*;

fn loaded() -> Session {
    let mut s = Session::new();
    s.load(create_authentication_example_v1(), "authentication.mdg", None);
    s
}

/// A session whose clock only moves when the test says so.
fn with_fake_clock() -> (Session, Arc<AtomicU64>) {
    let start = Instant::now();
    let millis = Arc::new(AtomicU64::new(0));
    let m = millis.clone();
    let mut s = Session::with_clock(Arc::new(move || start + Duration::from_millis(m.load(Ordering::SeqCst))));
    s.load(create_authentication_example_v1(), "authentication.mdg", None);
    (s, millis)
}

#[test]
fn offers_only_ports_of_the_same_kind() {
    let mut s = Session::new();
    s.load(create_authentication_example(), "authentication.mdg", None);
    // Protected Endpoint's validation request can only reach request handlers.
    let choices = s.route_choices(&ep("protected-endpoint:validate-token"), None).unwrap();
    let labels: Vec<&str> = choices.candidates.iter().map(|c| c.label.as_str()).collect();
    assert!(labels.contains(&"JWT Validator › Validate Token"), "{labels:?}");
    assert!(!labels.iter().any(|l| l.contains("Security Event")), "{labels:?}");

    s.navigate("application");
    // Once disconnected, the event output can reach only the one event input.
    assert!(s.delete_connection("root-security-event"));
    let partners = s.connection_partners(&ep("api-service:security-event"), ConnectRole::Source).unwrap();
    assert_eq!(partners, [ep("audit-log:security-event")]);

    let id = s.add_module_port("audit-log", PortDirection::Input, PortKind::Request).unwrap();
    let port = &s.document().modules.get("audit-log").unwrap().inputs[1];
    assert_eq!((port.id.as_str(), port.kind), (id.as_str(), PortKind::Request));
}

fn ep(spec: &str) -> Endpoint {
    let (m, p) = spec.split_once(':').unwrap();
    Endpoint::new(m, p)
}

fn rename(s: &mut Session, id: &str, name: &str) -> bool {
    s.apply(None, |d| rename_module(d, id, name))
}

fn module<'a>(s: &'a Session, id: &str) -> &'a Module {
    s.document().modules.get(id).unwrap()
}

// ------------------------------------------------------------ editor store

#[test]
fn routes_edits_through_core_operations_and_tracks_dirtiness() {
    let mut s = loaded();
    assert!(!s.dirty());
    let id = s.add_child_module(Some("Audit"), None).unwrap();
    assert!(s.dirty());
    assert_eq!(s.selection(), Some(&Selection::Module { module_id: id.clone() }));
    assert!(s.positions().get(&id).is_some());
    let port = s.add_module_port(&id, PortDirection::Input, PortKind::Event).unwrap();
    assert_eq!(module(&s, &id).inputs[0].id, port);
    assert!(s.connect(ep("frontend:login-submitted"), Endpoint::new(id.clone(), port)));
    assert!(validate_document(s.document(), ValidationOptions::default()).valid);
}

#[test]
fn records_rejected_operations_without_changing_the_document() {
    let mut s = loaded();
    let before = s.document().clone();
    assert!(!s.delete_module("api-service", false));
    assert!(s.error().unwrap().contains("child module"));
    assert!(Arc::ptr_eq(s.document(), &before));
    assert!(s.connect(ep("frontend:login-submitted"), ep("frontend:session-established")));
    assert!(!s.connect(ep("frontend:session-established"), ep("api-service:login-request")));
}

#[test]
fn resets_navigation_and_selection_when_their_targets_disappear() {
    let mut s = loaded();
    s.navigate("authentication");
    s.select(Some(Selection::Module { module_id: "token-issuer".into() }));
    s.navigate("api-service");
    assert!(s.delete_module("authentication", true));
    s.navigate("authentication");
    assert_eq!(s.current_module_id(), "api-service");
}

#[test]
fn keeps_layout_separate_from_the_canonical_document_and_persists_it_on_save() {
    let mut s = loaded();
    assert!(s.document().layout.is_none());
    s.set_positions(&[("frontend".into(), Position::new(11.0, 22.0))]);
    let saved = decode_mdg(&encode_mdg(&s.document_for_save(), EncodeOptions::default()).unwrap()).unwrap();
    assert_eq!(saved.layout.as_ref().unwrap().positions.get("frontend"), Some(&Position::new(11.0, 22.0)));
    s.load(saved, "x.mdg", None);
    assert_eq!(s.positions().get("frontend"), Some(&Position::new(11.0, 22.0)));
}

#[test]
fn reports_details_and_validation_in_the_view() {
    let mut s = loaded();
    let view = s.view();
    assert_eq!(view.validation, ValidationSummary { valid: true, errors: 0, warnings: 0 });
    assert_eq!(view.path.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["application"]);
    s.select(Some(Selection::Port { module_id: "api-service".into(), port_id: "login-response".into() }));
    assert_eq!(s.view().details.port_connections, Some(3));
    s.select(Some(Selection::Connection { connection_id: "root-login".into() }));
    let info = s.view().details.connection.unwrap();
    assert_eq!(info.from, "Frontend › Login Submitted (frontend:login-submitted)");
    assert_eq!(info.to, "API Service › Login Request (api-service:login-request)");
    let json = serde_json::to_value(s.view()).unwrap();
    assert_eq!(json["selection"], serde_json::json!({ "kind": "connection", "connectionId": "root-login" }));
    assert_eq!(json["document"]["modules"]["frontend"]["childIds"], serde_json::json!([]));
}

#[test]
fn offers_exactly_the_connections_core_accepts() {
    let mut s = loaded();
    s.navigate("api-service");
    let targets = s.connection_partners(&ep("authentication:token-issued"), ConnectRole::Source).unwrap();
    // Already connected to login-response; everything else that is a destination here.
    let ids: Vec<String> = targets.iter().map(|e| e.to_string()).collect();
    assert_eq!(
        ids,
        vec![
            "api-service:profile-response",
            "api-service:token-validation-request",
            "authentication:login-request",
            "user-routes:profile-request",
            "user-routes:token-validation-result",
        ]
    );
    let sources = s.connection_partners(&ep("user-routes:profile-request"), ConnectRole::Target).unwrap();
    assert!(sources.contains(&ep("api-service:login-request")));
    assert!(!sources.contains(&ep("api-service:profile-request"))); // duplicate
    for source in &sources {
        assert!(s.clone_for_test().connect(source.clone(), ep("user-routes:profile-request")));
    }
}

trait CloneForTest {
    fn clone_for_test(&self) -> Session;
}

impl CloneForTest for Session {
    fn clone_for_test(&self) -> Session {
        let mut s = Session::new();
        s.load(self.document_for_save(), "copy.mdg", None);
        s.navigate(self.current_module_id());
        s
    }
}

// ------------------------------------------------------------ undo / redo

#[test]
fn undoes_and_redoes_document_edits() {
    let mut s = loaded();
    let original = s.document().clone();
    rename(&mut s, "frontend", "Web App");
    let renamed = s.document().clone();
    assert_eq!(s.past_len(), 1);
    s.undo();
    assert!(Arc::ptr_eq(s.document(), &original));
    assert_eq!(s.future_len(), 1);
    s.redo();
    assert!(Arc::ptr_eq(s.document(), &renamed));
    assert_eq!(s.future_len(), 0);
}

#[test]
fn clears_redo_history_on_a_new_edit() {
    let mut s = loaded();
    rename(&mut s, "frontend", "A");
    s.undo();
    rename(&mut s, "frontend", "B");
    assert_eq!(s.future_len(), 0);
    s.redo();
    assert_eq!(module(&s, "frontend").name, "B");
}

#[test]
fn undoes_node_moves_and_layout() {
    let mut s = loaded();
    let before = s.positions().clone();
    s.set_positions(&[("frontend".into(), Position::new(1.0, 2.0)), ("jwt-validator".into(), Position::new(3.0, 4.0))]);
    assert_eq!(s.past_len(), 1); // a multi-node drag is one step
    s.set_positions(&[("frontend".into(), Position::new(1.0, 2.0))]);
    assert_eq!(s.past_len(), 1); // no change, no step
    s.apply_layout(&[("api-service".into(), Position::new(0.0, 0.0))]);
    assert_eq!(s.past_len(), 2);
    assert_eq!(s.layout_version(), 1);
    s.undo();
    s.undo();
    assert!(Arc::ptr_eq(s.positions(), &before));
}

#[test]
fn does_not_record_failed_operations() {
    let mut s = loaded();
    assert!(!s.delete_module("api-service", false));
    assert_eq!(s.past_len(), 0);
}

#[test]
fn coalesces_rapid_edits_that_share_a_key() {
    let (mut s, clock) = with_fake_clock();
    let original = s.document().clone();
    for text in ["H", "He", "Hel", "Hello"] {
        s.apply(Some("desc:frontend"), |d| update_module_description(d, "frontend", text));
        clock.fetch_add(100, Ordering::SeqCst);
    }
    assert_eq!(s.past_len(), 1);
    clock.fetch_add(COALESCE.as_millis() as u64 + 1, Ordering::SeqCst);
    s.apply(Some("desc:frontend"), |d| update_module_description(d, "frontend", "Hello!"));
    assert_eq!(s.past_len(), 2);
    s.undo();
    s.undo();
    assert!(Arc::ptr_eq(s.document(), &original));
}

#[test]
fn tracks_dirtiness_against_the_saved_state_across_undo() {
    let mut s = loaded();
    assert!(!s.dirty());
    rename(&mut s, "frontend", "X");
    assert!(s.dirty());
    s.undo();
    assert!(!s.dirty());
    s.redo();
    s.mark_saved("a.mdg", None);
    assert!(!s.dirty());
    s.undo();
    assert!(s.dirty());
}

#[test]
fn restores_navigation_safely_when_undo_removes_the_open_module() {
    let mut s = loaded();
    let id = s.add_child_module(Some("Temp"), None).unwrap();
    s.navigate(&id);
    s.undo();
    assert_eq!(s.current_module_id(), "application");
    assert_eq!(s.selection(), None);
}

#[test]
fn bounds_the_history_and_resets_it_on_load() {
    let mut s = loaded();
    for i in 0..HISTORY_LIMIT + 20 {
        rename(&mut s, "frontend", &format!("n{i}"));
    }
    assert_eq!(s.past_len(), HISTORY_LIMIT);
    s.load(create_authentication_example_v1(), "x.mdg", None);
    assert_eq!((s.past_len(), s.future_len()), (0, 0));
}

// ------------------------------------------------------------ boundary panels

#[test]
fn moves_boundary_panels_persists_them_in_the_file_and_undoes_the_move() {
    let mut s = loaded();
    let key = boundary_key("api-service", BoundarySide::Input);
    s.set_positions(&[(key.clone(), Position::new(-500.0, 40.0))]);
    let saved = decode_mdg(&encode_mdg(&s.document_for_save(), EncodeOptions::default()).unwrap()).unwrap();
    let boundaries = saved.layout.as_ref().unwrap().boundaries.as_ref().unwrap();
    assert_eq!(boundaries.keys(), vec!["api-service"]);
    assert_eq!(
        boundaries.get("api-service"),
        Some(&BoundaryLayout { input: Some(Position::new(-500.0, 40.0)), output: None })
    );
    s.load(saved, "x.mdg", None);
    assert_eq!(s.positions().get(&key), Some(&Position::new(-500.0, 40.0)));
    s.set_positions(&[(key.clone(), Position::new(0.0, 0.0))]);
    s.undo();
    assert_eq!(s.positions().get(&key), Some(&Position::new(-500.0, 40.0)));
}

#[test]
fn drops_positions_with_their_module() {
    let mut s = loaded();
    s.set_positions(&[(boundary_key("api-service", BoundarySide::Output), Position::new(-9999.0, 0.0))]);
    assert!(s.delete_module("api-service", true));
    assert!(!s.positions().keys().iter().any(|k| k.starts_with("api-service")));
}

// ------------------------------------------------------------ routing

#[test]
fn commits_a_previewed_route_as_one_undo_step_and_shows_where_it_joins() {
    let mut s = loaded();
    s.apply(None, |d| {
        let options = AddPortOptions { id: Some("audit".into()), name: "Audit Event".into(), ..Default::default() };
        Ok(add_port(d, "protected-endpoint", PortDirection::Output, options)?.document)
    });
    let base = s.document().clone();
    let choices = s.route_choices(&ep("protected-endpoint:audit"), None).unwrap();
    assert_eq!(choices.source_label, "API Service / User Routes / Protected Endpoint › Audit Event");
    assert!(choices.candidates.iter().any(|c| c.label == "JWT Validator › Validate Token"));
    assert!(choices.candidates.iter().all(|c| c.endpoint.module_id != "protected-endpoint"));

    let request = RouteRequest {
        source: ep("protected-endpoint:audit"),
        target: ep("jwt-validator:validate-token"),
        mode: Some(RouteMode::Send),
        reuse: true,
        ..Default::default()
    };
    let RoutePreview::Ok { token, join_name, steps, already_routed } = s.preview_route(&request) else {
        panic!("expected a plan")
    };
    assert_eq!(join_name, "Application");
    assert!(!already_routed);
    assert_eq!(steps[0], "Create output “Audit Event” on User Routes");
    assert_eq!(steps[1], "Connect Protected Endpoint › Audit Event → User Routes › Audit Event in User Routes");
    assert!(s.commit_route(token));
    assert_eq!(s.current_module_id(), "application");
    assert!(matches!(s.selection(), Some(Selection::Connection { .. })));
    assert!(validate_document(s.document(), ValidationOptions::default()).errors.is_empty());
    s.undo();
    assert!(Arc::ptr_eq(s.document(), &base));

    // The same route again reuses everything: nothing to commit.
    s.redo();
    let RoutePreview::Ok { token, already_routed, .. } = s.preview_route(&request) else { panic!("expected a plan") };
    assert!(already_routed);
    assert!(!s.commit_route(token));
}

#[test]
fn refuses_a_preview_computed_against_an_older_document() {
    let mut s = loaded();
    let request = RouteRequest {
        source: ep("frontend:login-submitted"),
        target: ep("jwt-validator:validate-token"),
        mode: Some(RouteMode::Send),
        reuse: true,
        ..Default::default()
    };
    let RoutePreview::Ok { token, .. } = s.preview_route(&request) else { panic!("expected a plan") };
    rename(&mut s, "frontend", "Changed");
    assert!(!s.commit_route(token));
    assert!(s.error().unwrap().contains("changed since the route was previewed"));
    let missing = RouteRequest { target: ep("jwt-validator:nope"), ..request };
    assert_eq!(
        s.preview_route(&missing),
        RoutePreview::Error { message: "Port \"nope\" does not exist on module \"jwt-validator\".".into() }
    );
}

// ------------------------------------------------------------ files

#[test]
fn opens_saves_and_refuses_invalid_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("arch.mdg");
    write_mdg(&file, &create_authentication_example_v1()).unwrap();
    let mut s = Session::new();
    assert!(s.open_file(&file));
    assert_eq!(s.file_name(), "arch.mdg");
    assert_eq!(s.positions().get("api-service"), Some(&Position::new(380.0, 40.0)));
    rename(&mut s, "frontend", "Web");
    assert!(s.dirty());
    assert!(s.save_to(&file));
    assert!(!s.dirty());
    // Saving an unchanged session reproduces the original bytes layout-wise.
    assert_eq!(read_mdg(&file).unwrap().modules.get("frontend").unwrap().name, "Web");
    assert_eq!(s.markdown_file_name(), "arch.md");
    assert!(s.export_markdown(&dir.path().join("arch.md")));
    assert!(std::fs::read_to_string(dir.path().join("arch.md")).unwrap().contains("# Web\n"));

    let mut bad = create_authentication_example_v1();
    let mut app = (**bad.modules.get("application").unwrap()).clone();
    app.child_ids.pop();
    bad.modules.insert("application", Arc::new(app));
    let bad_file = dir.path().join("bad.mdg");
    std::fs::write(&bad_file, encode_mdg(&bad, EncodeOptions { validate: false }).unwrap()).unwrap();
    assert!(!s.open_file(&bad_file));
    assert!(s.error().unwrap().starts_with("bad.mdg has 1 structural error(s) and cannot be edited safely."));
    assert_eq!(s.file_name(), "arch.mdg");
    assert!(!s.open_file(&dir.path().join("missing.mdg")));
    assert!(s.error().unwrap().starts_with("Cannot open file:"));
}

#[test]
fn round_trips_the_reference_file_unchanged() {
    let bytes = encode_mdg(&create_authentication_example_v1(), EncodeOptions::default()).unwrap();
    let mut s = Session::new();
    s.load(decode_mdg(&bytes).unwrap(), "a.mdg", None);
    assert_eq!(encode_mdg(&s.document_for_save(), EncodeOptions::default()).unwrap(), bytes);
}
