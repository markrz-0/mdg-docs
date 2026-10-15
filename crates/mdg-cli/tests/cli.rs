//! In-process CLI tests (ported from the TypeScript suite). Byte-level
//! parity with the original CLI is covered by `golden.rs`; these check the
//! effects of commands whose output contains generated IDs.

use std::path::{Path, PathBuf};

use mdg_cli::commander::Output;
use mdg_cli::{run_cli, CliIo};
use mdg_core::*;
use serde_json::Value;

#[derive(Default)]
struct BufferIo {
    stdout: String,
    stderr: String,
    stdin: String,
}

impl Output for BufferIo {
    fn out(&mut self, text: &str) {
        self.stdout.push_str(text);
    }
    fn err(&mut self, text: &str) {
        self.stderr.push_str(text);
    }
}

impl CliIo for BufferIo {
    fn read_stdin(&mut self) -> String {
        std::mem::take(&mut self.stdin)
    }
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| panic!("{e}: {}", self.stdout))
    }
}

fn mdg(args: &[&str]) -> Run {
    mdg_stdin(args, "")
}

fn mdg_stdin(args: &[&str], stdin: &str) -> Run {
    let mut io = BufferIo { stdin: stdin.to_string(), ..Default::default() };
    let argv: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let code = run_cli(&argv, &mut io);
    Run { code, stdout: io.stdout, stderr: io.stderr }
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    file: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let file = root.join("arch.mdg");
    write_mdg(&file, &create_authentication_example_v1()).unwrap();
    Fixture { _dir: dir, file: file.display().to_string(), root }
}

fn module<'a>(doc: &'a Document, id: &str) -> &'a Module {
    doc.modules.get(id).unwrap()
}

#[test]
fn creates_a_document_and_builds_a_graph_entirely_through_commands() {
    let fx = fixture();
    let f = fx.root.join("new.mdg").display().to_string();
    assert_eq!(mdg(&["new", &f, "--title", "Shop", "--root-id", "shop"]).code, 0);
    assert_eq!(mdg(&["new", &f, "--title", "Again"]).code, 3);
    let add =
        mdg(&["module", "add", &f, "--parent", "shop", "--name", "Cart", "--json", "--description", "Holds items."]);
    let cart = add.json()["moduleId"].as_str().unwrap().to_string();
    assert!(cart.starts_with("mod-") && is_valid_id(&cart));
    mdg(&["module", "add", &f, "--parent", "shop", "--name", "Checkout", "--id", "checkout"]);
    mdg(&["port", "add", &f, &cart, "--direction", "output", "--name", "Cart Ready", "--id", "ready"]);
    mdg(&["port", "add", &f, "checkout", "--direction", "input", "--name", "Start", "--id", "start"]);
    let conn = mdg(&["connect", &f, &format!("{cart}:ready"), "checkout:start", "--id", "c1", "--json"]);
    let json = conn.json();
    assert_eq!((json["ok"].as_bool(), json["connectionId"].as_str()), (Some(true), Some("c1")));
    assert_eq!(json["containerId"], "shop");
    mdg(&["module", "rename", &f, "checkout", "Checkout Flow"]);
    mdg(&["port", "rename", &f, "checkout", "start", "Begin"]);
    mdg_stdin(&["module", "set-description", &f, "checkout", "--description-file", "-"], "# Checkout\nTakes payment.");
    mdg(&["port", "set-description", &f, "checkout", "start", "--description", "Receives the cart."]);
    let doc = read_mdg(Path::new(&f)).unwrap();
    assert!(validate_document(&doc, ValidationOptions::default()).valid);
    assert_eq!(module(&doc, "checkout").name, "Checkout Flow");
    assert_eq!(module(&doc, "checkout").description, "# Checkout\nTakes payment.");
    assert_eq!(
        module(&doc, "checkout").inputs[0],
        Port {
            id: "start".into(),
            name: "Begin".into(),
            description: "Receives the cart.".into(),
            ..Default::default()
        }
    );
    assert_eq!(mdg(&["disconnect", &f, "c1"]).code, 0);
    assert!(module(&read_mdg(Path::new(&f)).unwrap(), "shop").connections.is_empty());
}

#[test]
fn supports_dry_runs_without_touching_the_file() {
    let fx = fixture();
    let before = std::fs::read(&fx.file).unwrap();
    let r = mdg(&["module", "remove", &fx.file, "authentication", "--cascade", "--dry-run", "--json"]);
    assert_eq!(r.code, 0);
    let json = r.json();
    assert_eq!(json["dryRun"], true);
    let mut removed: Vec<&str> =
        json["removedModules"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    removed.sort();
    assert_eq!(removed, vec!["authentication", "credential-validator", "token-issuer"]);
    assert_eq!(json["removedConnections"], serde_json::json!(["api-login", "api-token-issued", "api-login-failed"]));
    assert_eq!(std::fs::read(&fx.file).unwrap(), before);
}

#[test]
fn rejects_unsafe_edits_and_leaves_the_file_unchanged() {
    let fx = fixture();
    let f = fx.file.as_str();
    let before = std::fs::read(f).unwrap();
    let remove = mdg(&["module", "remove", f, "authentication"]);
    assert_eq!(remove.code, 1);
    assert!(remove.stderr.contains("cascade"));
    assert_eq!(mdg(&["module", "move", f, "protected-endpoint", "--to", "application"]).code, 1);
    assert!(mdg(&["module", "move", f, "api-service", "--to", "user-routes"]).stderr.contains("cycle"));
    let cross = mdg(&["connect", f, "protected-endpoint:http-response", "frontend:profile-received"]);
    assert_eq!(cross.code, 1);
    assert!(cross.stderr.contains("--route"));
    assert_eq!(mdg(&["port", "add", f, "application", "--direction", "input", "--name", "x"]).code, 1);
    assert_eq!(mdg(&["module", "add", f, "--parent", "application", "--name", "x", "--id", "a:b"]).code, 2);
    assert_eq!(std::fs::read(f).unwrap(), before);
}

#[test]
fn routes_across_levels_with_route() {
    let fx = fixture();
    let f = fx.file.as_str();
    mdg(&[
        "port",
        "add",
        f,
        "protected-endpoint",
        "--direction",
        "output",
        "--name",
        "Audit Event",
        "--id",
        "audit-event",
    ]);
    mdg(&["module", "add", f, "--parent", "application", "--name", "Audit Log", "--id", "audit"]);
    mdg(&["port", "add", f, "audit", "--direction", "input", "--name", "Record", "--id", "record"]);
    let dry = mdg(&["connect", f, "protected-endpoint:audit-event", "audit:record", "--route", "--dry-run"]);
    assert_eq!(dry.code, 0);
    assert!(dry.stdout.contains("[dry run] Routed") && dry.stdout.contains("+ output user-routes:"));
    let r = mdg(&["connect", f, "protected-endpoint:audit-event", "audit:record", "--route", "--json"]);
    let kinds: Vec<String> =
        r.json()["plan"]["steps"].as_array().unwrap().iter().map(|s| s["kind"].as_str().unwrap().to_string()).collect();
    assert_eq!(kinds, ["create-port", "create-connection", "create-port", "create-connection", "create-connection"]);
    let doc = read_mdg(Path::new(f)).unwrap();
    assert_eq!(validate_document(&doc, ValidationOptions::default()).errors, vec![]);
    let traced = mdg(&["trace", f, "protected-endpoint", "audit-event", "--json"]).json();
    let last = traced["routes"][0]["hops"].as_array().unwrap().last().unwrap().clone();
    assert_eq!((last["moduleId"].as_str(), last["portId"].as_str()), (Some("audit"), Some("record")));
    // Routing the same pair again reuses everything and creates nothing.
    let again = mdg(&["connect", f, "protected-endpoint:audit-event", "audit:record", "--route", "--json"]).json();
    assert!(again["plan"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["kind"].as_str().unwrap().starts_with("reuse")));
}

#[test]
fn reads_deep_hierarchies() {
    let fx = fixture();
    let f = fx.root.join("deep.mdg");
    let mut doc =
        create_document(CreateDocumentOptions { root_id: Some("root".into()), ..Default::default() }).unwrap();
    let mut parent = "root".to_string();
    for i in 0..2000 {
        let id = format!("m{i}");
        let opts =
            AddModuleOptions { id: Some(id.clone()), parent_id: parent, name: format!("M{i}"), ..Default::default() };
        doc = add_module(&doc, opts).unwrap().document;
        parent = id;
    }
    write_mdg(&f, &doc).unwrap();
    let f = f.display().to_string();
    // Run on a big stack, like the binary does.
    let tree = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || mdg(&["tree", &f, "--json"]))
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(tree.code, 0);
    // Too deep for serde_json's default recursion limit, so check the text.
    assert!(tree.stdout.starts_with(
        "{
  \"id\": \"root\",
  \"name\": \"Untitled\",
  \"children\": [
    {
      \"id\": \"m0\""
    ));
    assert!(tree.stdout.contains("\"id\": \"m1999\""));
}

#[test]
fn manages_request_ports() {
    let fx = fixture();
    let f = fx.root.join("rr.mdg").display().to_string();
    mdg(&["new", &f, "--title", "RR", "--root-id", "app"]);
    mdg(&["module", "add", &f, "--parent", "app", "--name", "Web", "--id", "web"]);
    mdg(&["module", "add", &f, "--parent", "app", "--name", "Api", "--id", "api"]);
    let add = mdg(&[
        "port",
        "add",
        &f,
        "web",
        "--direction",
        "output",
        "--kind",
        "request",
        "--name",
        "Fetch",
        "--id",
        "fetch",
        "--description",
        "GET /items",
        "--response",
        "The items.",
        "--json",
    ]);
    assert_eq!(add.code, 0, "{}", add.stderr);
    assert_eq!(add.json()["kind"], "request");
    mdg_stdin(
        &[
            "port",
            "add",
            &f,
            "api",
            "--direction",
            "input",
            "--kind",
            "request",
            "--name",
            "Items",
            "--id",
            "items",
            "--response-file",
            "-",
        ],
        "A JSON list.",
    );
    mdg(&["port", "add", &f, "api", "--direction", "input", "--name", "Ping", "--id", "ping"]);

    let rejected = mdg(&["port", "add", &f, "api", "--direction", "input", "--name", "X", "--response", "no"]);
    assert_eq!(rejected.code, 2, "{}", rejected.stderr);
    let mismatch = mdg(&["connect", &f, "web:fetch", "api:ping"]);
    assert_ne!(mismatch.code, 0);
    assert!(mismatch.stderr.contains("same kind"), "{}", mismatch.stderr);
    assert_eq!(mdg(&["connect", &f, "web:fetch", "api:items", "--id", "c1"]).code, 0);

    let blocked = mdg(&["port", "set-kind", &f, "api", "items", "event"]);
    assert!(blocked.stderr.contains("c1"), "{}", blocked.stderr);
    assert_eq!(mdg(&["port", "set-response", &f, "api", "items", "--response", "Items as JSON."]).code, 0);
    assert_ne!(mdg(&["port", "set-response", &f, "api", "ping", "--response", "x"]).code, 0);
    assert_eq!(mdg(&["port", "set-kind", &f, "api", "ping", "request"]).code, 0);

    let doc = read_mdg(Path::new(&f)).unwrap();
    assert!(validate_document(&doc, ValidationOptions::default()).valid);
    assert_eq!(doc.format_version, REQUEST_PORTS_SCHEMA_VERSION);
    let items = &module(&doc, "api").inputs[0];
    assert_eq!((items.kind, items.response.as_str()), (PortKind::Request, "Items as JSON."));
    assert_eq!(module(&doc, "api").inputs[1].kind, PortKind::Request);

    let inputs = mdg(&["inputs", &f, "api"]);
    assert!(inputs.stdout.contains("## Items\n- ID: `items`\n- Kind: request/response, handled here\n\n### Request\n\n_No description._\n\n### Response\n\nItems as JSON."), "{}", inputs.stdout);
    let json = mdg(&["outputs", &f, "web", "--json"]).json();
    assert_eq!(json["ports"][0]["response"], "The items.");
}
