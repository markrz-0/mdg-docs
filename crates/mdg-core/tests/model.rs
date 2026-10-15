mod common;

use std::sync::Arc;

use common::*;
use mdg_core::*;

fn opts_module(id: &str, parent: &str, name: &str) -> AddModuleOptions<'static> {
    AddModuleOptions { id: Some(id.into()), parent_id: parent.into(), name: name.into(), ..Default::default() }
}

fn err_code<T: std::fmt::Debug>(r: Result<T>) -> ErrorCode {
    r.expect_err("expected an error").code
}

fn err_message<T: std::fmt::Debug>(r: Result<T>) -> String {
    r.expect_err("expected an error").message
}

// ------------------------------------------------------------ documents and root

#[test]
fn creates_a_valid_document_with_a_single_root() {
    let doc = create_document(CreateDocumentOptions {
        title: Some("App".into()),
        root_id: Some("app".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(doc.root_id, "app");
    let root = module(&doc, "app");
    assert_eq!(root.parent_id, None);
    assert_eq!(root.name, "App");
    assert!(root.inputs.is_empty() && root.outputs.is_empty());
    let result = validate_document(&doc, ValidationOptions::default());
    assert!(result.valid && result.errors.is_empty() && result.warnings.is_empty());
}

#[test]
fn generates_opaque_ids_when_none_are_given() {
    let doc = create_document(CreateDocumentOptions::default()).unwrap();
    assert!(doc.root_id.starts_with("mod-") && doc.root_id.len() == 40, "{}", doc.root_id);
    assert_eq!(doc.metadata.title, "Untitled");
}

#[test]
fn rejects_ports_on_the_root() {
    let doc = Builder::new().doc;
    let r = add_port(&doc, "root", PortDirection::Input, AddPortOptions { name: "x".into(), ..Default::default() });
    assert_eq!(err_code(r), ErrorCode::InvalidOperation);
}

#[test]
fn reports_root_constraint_violations() {
    let doc = Builder::new().doc;
    let bad = edit_module(&doc, "root", |m| {
        m.inputs.push(Port { id: "i".into(), name: "i".into(), description: String::new(), ..Default::default() })
    });
    assert!(error_codes(&bad).contains(&"ROOT_HAS_PORTS"));
    let mut missing = doc.clone();
    missing.root_id = "nope".into();
    assert!(error_codes(&missing).contains(&"ROOT_MISSING"));
}

// ------------------------------------------------------------ modules

#[test]
fn supports_arbitrary_nesting() {
    let mut doc = Builder::new().doc;
    let mut parent = "root".to_string();
    for i in 0..2000 {
        let id = format!("m{i}");
        doc = add_module(&doc, opts_module(&id, &parent, &format!("M{i}"))).unwrap().document;
        parent = id;
    }
    assert!(validate_document(&doc, ValidationOptions::default()).valid);
    assert_eq!(get_ancestors(&doc, "m1999").unwrap().len(), 2000);
    assert_eq!(get_descendants(&doc, "root", None).unwrap().len(), 2000);
    assert!(render_tree(&doc, &TreeOptions::default()).unwrap().lines().count() == 2001);
}

#[test]
fn inserts_renames_and_describes_modules_without_mutating_the_input() {
    let doc = Builder::new().doc;
    let CreateResult { document, id } = add_module(&doc, opts_module("api", "root", "API")).unwrap();
    assert!(doc.modules.get("api").is_none());
    let children: Vec<&str> = get_children(&document, "root").unwrap().iter().map(|m| m.id.as_str()).collect();
    assert_eq!(children, vec![id.as_str()]);
    assert_eq!(get_parent(&document, "api").unwrap().unwrap().id, "root");
    let renamed =
        update_module_description(&rename_module(&document, "api", "API Service").unwrap(), "api", "# Hi").unwrap();
    assert_eq!(module(&renamed, "api").name, "API Service");
    assert_eq!(module(&renamed, "api").description, "# Hi");
}

#[test]
fn rejects_bad_ids_duplicates_and_empty_names() {
    let doc = Builder::new().module("a").doc;
    assert_eq!(err_code(add_module(&doc, opts_module("a", "root", "x"))), ErrorCode::InvalidArgument);
    assert_eq!(err_code(add_module(&doc, opts_module("bad:id", "root", "x"))), ErrorCode::InvalidArgument);
    let empty = AddModuleOptions { parent_id: "root".into(), name: "  ".into(), ..Default::default() };
    assert_eq!(err_message(add_module(&doc, empty)), "Module name must not be empty.");
    let missing = AddModuleOptions { parent_id: "missing".into(), name: "x".into(), ..Default::default() };
    assert_eq!(err_code(add_module(&doc, missing)), ErrorCode::NotFound);
}

#[test]
fn inserts_at_an_index() {
    let doc = Builder::new().module("a").module("b").doc;
    let opts = AddModuleOptions { index: Some(1), ..opts_module("c", "root", "C") };
    let doc = add_module(&doc, opts).unwrap().document;
    assert_eq!(module(&doc, "root").child_ids, vec!["a", "c", "b"]);
    let opts = AddModuleOptions { index: Some(99), ..opts_module("d", "root", "D") };
    let doc = add_module(&doc, opts).unwrap().document;
    assert_eq!(module(&doc, "root").child_ids, vec!["a", "c", "b", "d"]);
}

#[test]
fn removes_leaf_modules_and_refuses_unsafe_removals_by_default() {
    let b = Builder::new().module("a").module("b").module_in("a1", "a").output("a", "out").input("b", "in");
    let doc = b.connect("c1", "a:out", "b:in").doc;
    assert_eq!(err_code(remove_module(&doc, "root", false)), ErrorCode::InvalidOperation);
    assert!(err_message(remove_module(&doc, "a", false)).contains("child module"));
    let without_child = remove_module(&doc, "a1", false).unwrap();
    assert!(err_message(remove_module(&without_child, "a", false)).contains("connected"));
    let cascaded = remove_module(&doc, "a", true).unwrap();
    assert!(cascaded.modules.get("a").is_none());
    assert!(cascaded.modules.get("a1").is_none());
    assert!(module(&cascaded, "root").connections.is_empty());
    assert!(validate_document(&cascaded, ValidationOptions::default()).valid);
}

#[test]
fn moves_modules_preserving_ids_and_rejects_cycles_and_broken_connections() {
    let b = Builder::new().module("a").module("b").module_in("a1", "a").module_in("a2", "a1");
    let moved = move_module(&b.doc, "a1", "b", None).unwrap();
    assert_eq!(module(&moved, "a1").parent_id.as_deref(), Some("b"));
    assert_eq!(module(&moved, "a2").parent_id.as_deref(), Some("a1"));
    assert_eq!(module(&moved, "b").child_ids, vec!["a1"]);
    assert!(module(&moved, "a").child_ids.is_empty());
    assert!(validate_document(&moved, ValidationOptions::default()).valid);

    assert!(err_message(move_module(&b.doc, "root", "a", None)).contains("root"));
    assert!(err_message(move_module(&b.doc, "a", "a2", None)).contains("cycle"));
    assert!(err_message(move_module(&b.doc, "a", "a", None)).contains("cycle"));

    let doc = b.output("a1", "o").output("a", "o").connect("c", "a1:o", "a:o").doc;
    assert!(err_message(move_module(&doc, "a1", "b", None)).contains("would become invalid"));
}

#[test]
fn reorders_within_the_same_parent() {
    let doc = Builder::new().module("a").module("b").module("c").doc;
    let doc = move_module(&doc, "c", "root", Some(0)).unwrap();
    assert_eq!(module(&doc, "root").child_ids, vec!["c", "a", "b"]);
}

// ------------------------------------------------------------ ports

#[test]
fn creates_renames_describes_and_deletes_ports() {
    let b = Builder::new().module("a").named_input("a", "in", "Request").named_output("a", "out", "Response");
    let doc = rename_port(&b.doc, "a", "in", "Login Request").unwrap();
    let doc = update_port_description(&doc, "a", "out", "Emits a JWT.").unwrap();
    assert_eq!(
        module(&doc, "a").inputs[0],
        Port { id: "in".into(), name: "Login Request".into(), description: String::new(), ..Default::default() }
    );
    assert_eq!(module(&doc, "a").outputs[0].description, "Emits a JWT.");
    let doc = remove_port(&doc, "a", "in", false).unwrap();
    assert!(module(&doc, "a").inputs.is_empty());
}

#[test]
fn shares_one_id_namespace_for_inputs_and_outputs() {
    let doc = Builder::new().module("a").input("a", "x").doc;
    let r = add_port(
        &doc,
        "a",
        PortDirection::Output,
        AddPortOptions { id: Some("x".into()), name: "x".into(), ..Default::default() },
    );
    assert!(err_message(r).contains("already has a port"));
}

#[test]
fn refuses_to_delete_connected_ports_unless_cascading() {
    let doc = Builder::new().module("a").module("b").output("a", "o").input("b", "i").connect("c", "a:o", "b:i").doc;
    assert!(err_message(remove_port(&doc, "a", "o", false)).contains("Disconnect"));
    let doc = remove_port(&doc, "a", "o", true).unwrap();
    assert!(module(&doc, "root").connections.is_empty());
    assert!(validate_document(&doc, ValidationOptions::default()).valid);
}

// ------------------------------------------------------------ connections

fn base() -> Builder {
    Builder::new()
        .module("p")
        .module_in("a", "p")
        .module_in("b", "p")
        .input("p", "pin")
        .output("p", "pout")
        .input("a", "ain")
        .output("a", "aout")
        .input("b", "bin")
        .output("b", "bout")
}

fn connect_in(doc: &Document, from: &str, to: &str, container: Option<&str>) -> Result<AddConnectionResult> {
    add_connection(
        doc,
        AddConnectionOptions {
            from: ep(from),
            to: ep(to),
            container_id: container.map(Into::into),
            ..Default::default()
        },
    )
}

#[test]
fn allows_all_four_boundary_patterns() {
    let doc = base()
        .connect("c1", "p:pin", "a:ain") // parent input -> child input
        .connect("c2", "a:aout", "b:bin") // child output -> child input
        .connect("c3", "b:bout", "p:pout") // child output -> parent output
        .connect("c4", "p:pin", "p:pout") // parent input -> parent output
        .doc;
    let ids: Vec<&str> = module(&doc, "p").connections.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["c1", "c2", "c3", "c4"]);
    assert!(validate_document(&doc, ValidationOptions::default()).errors.is_empty());
}

#[test]
fn rejects_invalid_directions() {
    let doc = base().doc;
    assert!(err_message(connect_in(&doc, "a:ain", "b:bin", Some("p"))).contains("source"));
    assert!(err_message(connect_in(&doc, "a:aout", "b:bout", Some("p"))).contains("destination"));
    assert!(err_message(connect_in(&doc, "p:pout", "a:ain", Some("p"))).contains("source"));
    assert!(err_message(connect_in(&doc, "a:aout", "p:pin", Some("p"))).contains("destination"));
}

#[test]
fn rejects_cross_level_connections() {
    let doc = base().module_in("deep", "a").input("deep", "din").doc;
    assert!(err_message(connect_in(&doc, "b:bout", "deep:din", None)).contains("routing"));
    assert!(err_message(connect_in(&doc, "b:bout", "deep:din", Some("p"))).contains("direct children"));
}

#[test]
fn allows_fan_out_multiple_sources_and_message_cycles() {
    let doc = base()
        .module_in("c", "p")
        .input("c", "cin")
        .connect("f1", "a:aout", "b:bin")
        .connect("f2", "a:aout", "c:cin") // fan-out
        .connect("m1", "p:pin", "b:bin") // multiple sources into b:bin
        .connect("cy", "b:bout", "a:ain") // a -> b -> a cycle
        .doc;
    assert!(validate_document(&doc, ValidationOptions::default()).valid);
}

#[test]
fn rejects_duplicates_and_removes_connections() {
    let doc = base().connect("c", "a:aout", "b:bin").doc;
    assert!(err_message(connect_in(&doc, "a:aout", "b:bin", None)).contains("already exists"));
    let dup_id = add_connection(
        &doc,
        AddConnectionOptions { id: Some("c".into()), from: ep("b:bout"), to: ep("a:ain"), ..Default::default() },
    );
    assert!(err_message(dup_id).contains("already exists"));
    let doc = remove_connection(&doc, "c").unwrap();
    assert!(module(&doc, "p").connections.is_empty());
    assert_eq!(err_code(remove_connection(&doc, "c")), ErrorCode::NotFound);
}

// ------------------------------------------------------------ validation

#[test]
fn rejects_parent_cycles() {
    let doc = Builder::new().module("a").module_in("b", "a").doc;
    let doc = edit_module(&doc, "a", |m| m.parent_id = Some("b".into()));
    let doc = edit_module(&doc, "root", |m| m.child_ids.clear());
    assert!(error_codes(&doc).contains(&"PARENT_CYCLE"));
    assert!(codes(&validate_module(&doc, "a", ValidationOptions::default()).errors).contains(&"PARENT_CYCLE"));
}

#[test]
fn detects_inconsistent_parent_child_links_and_missing_references() {
    let doc = Builder::new().module("a").module("b").output("a", "o").input("b", "i").connect("c", "a:o", "b:i").doc;
    let mut doc = edit_module(&doc, "root", |m| {
        m.child_ids = vec!["a".into(), "b".into(), "ghost".into()];
        m.connections = vec![
            Connection { id: "c".into(), from: ep("a:o"), to: ep("b:missing") },
            Connection { id: "c".into(), from: ep("a:o"), to: ep("b:i") },
        ];
    });
    doc.modules.insert(
        "orphan",
        Arc::new(Module {
            id: "orphan".into(),
            parent_id: Some("nowhere".into()),
            name: "o".into(),
            description: String::new(),
            inputs: vec![],
            outputs: vec![],
            child_ids: vec![],
            connections: vec![],
            tags: vec![],
        }),
    );
    let found = error_codes(&doc);
    for code in ["MISSING_CHILD", "CONNECTION_PORT_MISSING", "DUPLICATE_CONNECTION_ID", "MISSING_PARENT"] {
        assert!(found.contains(&code), "{code} missing from {found:?}");
    }
}

#[test]
fn reports_unconnected_ports_as_warnings_only() {
    let doc = Builder::new().module("a").input("a", "in").doc;
    let result = validate_document(&doc, ValidationOptions::default());
    assert!(result.valid);
    assert_eq!(codes(&result.warnings), vec!["UNCONNECTED_PORT"]);
    assert!(validate_document(&doc, ValidationOptions { warnings: false }).warnings.is_empty());
}

#[test]
fn issues_serialize_with_the_original_key_order() {
    let doc = Builder::new().module("a").input("a", "in").doc;
    let result = validate_document(&doc, ValidationOptions::default());
    assert_eq!(
        js::stringify(&js::to_value(&result.warnings[0])),
        r#"{"severity":"warning","code":"UNCONNECTED_PORT","moduleId":"a","portId":"in","message":"The input \"in\" (a:in) is not connected."}"#
    );
    let issues = check_connection(&doc, "root", &ep("a:in"), &ep("a:nope"), Some("c1"));
    assert_eq!(
        js::stringify(&js::to_value(&issues[1])),
        r#"{"severity":"error","moduleId":"root","connectionId":"c1","portId":"nope","code":"CONNECTION_PORT_MISSING","message":"Connection \"c1\" in \"root\" references missing port \"a:nope\"."}"#
    );
}

// ------------------------------------------------------------ transactions

#[test]
fn commits_multi_step_edits_only_when_all_steps_succeed() {
    let doc = Builder::new().module("a").doc;
    let failed = transaction(&doc, |d| {
        let next = add_module(d, opts_module("x", "a", "X"))?.document;
        Ok(add_module(&next, opts_module("x", "a", "dup"))?.document)
    });
    assert_eq!(err_code(failed), ErrorCode::InvalidArgument);
    assert!(doc.modules.get("x").is_none());
    let ok = transaction(&doc, |d| Ok(add_module(d, opts_module("x", "a", "X"))?.document)).unwrap();
    assert!(ok.modules.get("x").is_some());
}

#[test]
fn rejects_results_that_fail_validation() {
    let doc = Builder::new().module("a").doc;
    let r = transaction(&doc, |d| {
        let mut next = d.clone();
        next.root_id = "missing".into();
        Ok(next)
    });
    assert!(err_message(r).contains("structural"));
}

// ------------------------------------------------------------ layout metadata

#[test]
fn drops_boundary_positions_of_removed_modules_but_keeps_them_on_move() {
    let mut doc = Builder::new().module("a").module("b").module_in("a1", "a").doc;
    let mut positions = JsMap::new();
    positions.insert("a1", Position::new(1.0, 1.0));
    let mut boundaries = JsMap::new();
    boundaries.insert("a", BoundaryLayout { input: Some(Position::new(0.0, 0.0)), output: None });
    boundaries.insert("a1", BoundaryLayout { input: None, output: Some(Position::new(5.0, 5.0)) });
    doc.layout = Some(Layout { positions, boundaries: Some(boundaries) });

    let moved = move_module(&doc, "a1", "b", None).unwrap();
    let layout = moved.layout.as_ref().unwrap();
    assert_eq!(
        layout.boundaries.as_ref().unwrap().get("a1"),
        Some(&BoundaryLayout { input: None, output: Some(Position::new(5.0, 5.0)) })
    );
    assert!(layout.positions.get("a1").is_none());
    let removed = remove_module(&doc, "a", true).unwrap();
    assert!(removed.layout.unwrap().boundaries.is_none());
}
