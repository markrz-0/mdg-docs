mod common;

use common::*;
use mdg_core::*;

fn hop_ids(hops: &[TraceHop]) -> Vec<String> {
    hops.iter().map(|h| format!("{}:{}", h.module_id, h.port_id)).collect()
}

fn backward() -> TraceOptions {
    TraceOptions { direction: TraceDirection::Backward, ..Default::default() }
}

#[test]
fn example_is_structurally_valid_without_warnings() {
    let result = validate_document(&create_authentication_example_v1(), ValidationOptions::default());
    assert_eq!(result.errors, vec![]);
    assert_eq!(result.warnings, vec![]);
}

// ------------------------------------------------------------ trace

#[test]
fn traces_forward_across_parent_boundaries() {
    let doc = create_authentication_example_v1();
    let result = trace(&doc, "protected-endpoint", "token-validation-request", TraceOptions::default()).unwrap();
    assert_eq!(result.routes.len(), 1);
    assert_eq!(
        hop_ids(&result.routes[0].hops),
        vec![
            "protected-endpoint:token-validation-request",
            "user-routes:token-validation-request",
            "api-service:token-validation-request",
            "jwt-validator:validate-token",
        ]
    );
    assert_eq!(result.routes[0].end, TraceRouteEnd::Terminal);
    assert_eq!(result.routes[0].hops[1].connection_id.as_deref(), Some("routes-validate-token"));
    assert_eq!(result.routes[0].hops[1].container_id.as_deref(), Some("user-routes"));
}

#[test]
fn traces_the_response_backwards() {
    let doc = create_authentication_example_v1();
    let result = trace(&doc, "protected-endpoint", "token-validation-result", backward()).unwrap();
    assert_eq!(
        hop_ids(&result.routes[0].hops),
        vec![
            "protected-endpoint:token-validation-result",
            "user-routes:token-validation-result",
            "api-service:token-validation-result",
            "jwt-validator:validation-result",
        ]
    );
}

#[test]
fn reports_multiple_routes() {
    let doc = create_authentication_example_v1();
    let result = trace(&doc, "api-service", "login-response", backward()).unwrap();
    let ends: Vec<String> = result.routes.iter().map(|r| hop_ids(&r.hops).pop().unwrap()).collect();
    assert_eq!(ends, vec!["token-issuer:token-issued", "credential-validator:credentials-invalid"]);
}

#[test]
fn terminates_on_cycles_and_respects_max_depth() {
    let doc = Builder::new()
        .module("a")
        .module("b")
        .input("a", "in")
        .output("a", "out")
        .input("b", "in")
        .output("b", "out")
        .connect("a-pass", "a:in", "a:out")
        .connect("b-pass", "b:in", "b:out")
        .connect("ab", "a:out", "b:in")
        .connect("ba", "b:out", "a:in")
        .doc;
    let result = trace(&doc, "a", "in", TraceOptions::default()).unwrap();
    assert!(result.cyclic);
    assert_eq!(result.routes.len(), 1);
    assert_eq!(result.routes[0].end, TraceRouteEnd::Cycle);
    assert_eq!(result.routes[0].cycle_index, Some(0));
    assert_eq!(hop_ids(&result.routes[0].hops), vec!["a:in", "a:out", "b:in", "b:out", "a:in"]);
    let limited = trace(&doc, "a", "in", TraceOptions { max_depth: 2, ..Default::default() }).unwrap();
    assert_eq!(limited.routes[0].end, TraceRouteEnd::MaxDepth);
    assert_eq!(limited.routes[0].hops.len(), 3);
}

#[test]
fn caps_routes_and_flags_truncation() {
    let mut b = Builder::new().module("src").output("src", "o");
    for i in 0..5 {
        let id = format!("d{i}");
        b = b.module(&id).input(&id, "i").connect(&format!("c{i}"), "src:o", &format!("{id}:i"));
    }
    let result = trace(&b.doc, "src", "o", TraceOptions { max_routes: 3, ..Default::default() }).unwrap();
    assert_eq!(result.routes.len(), 3);
    assert!(result.truncated);
}

// ------------------------------------------------------------ documentation

#[test]
fn describes_public_contracts_only_at_depth_0() {
    let doc = create_authentication_example_v1();
    let md = describe_module_markdown(&doc, "authentication", Some(0)).unwrap();
    assert!(
        md.contains("# Authentication\n\n## Identity\n- ID: `authentication`\n- Parent: API Service (`api-service`)")
    );
    assert!(md.contains("## Inputs\n\n### Login Request\n- ID: `login-request`"));
    assert!(!md.contains("Submodules"));
    assert!(!md.contains("Credential Validator"));
}

#[test]
fn includes_children_and_internal_connections_at_depth_1() {
    let doc = create_authentication_example_v1();
    let result = describe_module(&doc, "authentication", Some(1)).unwrap();
    let children: Vec<&str> = result.module.children.as_ref().unwrap().iter().map(|c| c.id.as_str()).collect();
    assert_eq!(children, vec!["credential-validator", "token-issuer"]);
    assert_eq!(result.module.connections.as_ref().unwrap().len(), 4);
    let ids: Vec<&str> = result.descendants.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, vec!["credential-validator", "token-issuer"]);
    assert!(result.descendants[0].children.is_none());
    let md = describe_module_markdown(&doc, "authentication", Some(1)).unwrap();
    assert!(md.contains("## Internal Connections"));
    assert!(md.contains(
        "Login Request (boundary input, `authentication:login-request`) -> Credential Validator › Check Credentials"
    ));
    assert!(!md.contains("JWT Validator")); // no unrelated modules
}

#[test]
fn describes_recursively_to_depth_n() {
    let doc = create_authentication_example_v1();
    let result = describe_module(&doc, "api-service", Some(2)).unwrap();
    let got: Vec<(&str, usize)> = result.descendants.iter().map(|d| (d.id.as_str(), d.depth)).collect();
    assert_eq!(
        got,
        vec![
            ("authentication", 1),
            ("credential-validator", 2),
            ("token-issuer", 2),
            ("user-routes", 1),
            ("protected-endpoint", 2),
        ]
    );
}

#[test]
fn exports_the_whole_hierarchy_and_per_module_files() {
    let doc = create_authentication_example_v1();
    let md = export_document_markdown(&doc).unwrap();
    for m in doc.modules.values() {
        assert!(md.contains(&format!("# {}\n", m.name)));
    }
    let files = export_module_files(&doc).unwrap();
    assert!(files.iter().any(|f| f.path == "index.md"));
    assert_eq!(files.len(), 10);
    let api = files.iter().find(|f| f.path == "api-service.md").unwrap();
    assert!(api.content.contains("- [Authentication](authentication.md) (`authentication`)"));
    assert_eq!(module_file_name("con"), "con_.md");
    assert_eq!(module_file_name("COM1"), "COM1_.md");
    assert_eq!(module_file_name("com10"), "com10.md");
}

#[test]
fn renders_trees() {
    let doc = create_authentication_example_v1();
    assert_eq!(
        render_tree(&doc, &TreeOptions::default()).unwrap(),
        [
            "Application",
            "├── Frontend",
            "├── API Service",
            "│   ├── Authentication",
            "│   │   ├── Credential Validator",
            "│   │   └── Token Issuer",
            "│   └── User Routes",
            "│       └── Protected Endpoint",
            "└── JWT Validator",
        ]
        .join("\n")
    );
    let opts = TreeOptions { depth: Some(1), ids: true, ..Default::default() };
    assert!(render_tree(&doc, &opts).unwrap().contains("├── API Service [api-service] (+2)"));
    let tree = build_tree(&doc, &TreeOptions { depth: Some(1), ..Default::default() }).unwrap();
    let ids: Vec<&str> = tree.children.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["frontend", "api-service", "jwt-validator"]);
    assert_eq!(tree.children[1].truncated, Some(true));
}
