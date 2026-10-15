//! Replays the golden corpus recorded from the original TypeScript CLI
//! (`scripts/gen-golden.mjs`) against this binary. Every invocation must
//! produce the same exit code, stdout and stderr, and edits must write the
//! same bytes.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use base64::Engine;
use mdg_core::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Step {
    args: Vec<String>,
    stdin: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Expected {
    code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    setup: String,
    steps: Vec<Step>,
    files: Vec<String>,
    bytes: bool,
    results: Vec<Expected>,
    written: serde_json::Map<String, serde_json::Value>,
}

fn write(dir: &Path, name: &str, doc: &Document) {
    write_mdg(&dir.join(name), doc).unwrap();
}

fn small_doc(title: &str) -> Document {
    create_document(CreateDocumentOptions {
        title: Some(title.into()),
        root_id: Some("root".into()),
        ..Default::default()
    })
    .unwrap()
}

fn setup(kind: &str, dir: &Path) {
    match kind {
        "none" => {}
        "example" => write(dir, "arch.mdg", &create_authentication_example_v1()),
        "bad" => {
            write(dir, "arch.mdg", &create_authentication_example_v1());
            let mut doc = create_authentication_example_v1();
            let mut app = (**doc.modules.get("application").unwrap()).clone();
            app.child_ids = vec!["frontend".into(), "api-service".into()];
            doc.modules.insert("application", Arc::new(app));
            std::fs::write(dir.join("bad.mdg"), encode_mdg(&doc, EncodeOptions { validate: false }).unwrap()).unwrap();
        }
        "garbage" => std::fs::write(dir.join("garbage.mdg"), "not an mdg file at all, definitely not").unwrap(),
        "cycle" => {
            let mut doc = small_doc("Cycle");
            for id in ["a", "b"] {
                let opts = AddModuleOptions {
                    id: Some(id.into()),
                    parent_id: "root".into(),
                    name: id.to_uppercase(),
                    ..Default::default()
                };
                doc = add_module(&doc, opts).unwrap().document;
            }
            for (m, d, id) in [
                ("a", PortDirection::Input, "in"),
                ("a", PortDirection::Output, "out"),
                ("b", PortDirection::Input, "in"),
                ("b", PortDirection::Output, "out"),
            ] {
                let opts = AddPortOptions { id: Some(id.into()), name: format!("{m} {id}"), ..Default::default() };
                doc = add_port(&doc, m, d, opts).unwrap().document;
            }
            for (id, f, t) in [
                ("a-pass", "a:in", "a:out"),
                ("b-pass", "b:in", "b:out"),
                ("ab", "a:out", "b:in"),
                ("ba", "b:out", "a:in"),
            ] {
                let ep = |s: &str| {
                    let (m, p) = s.split_once(':').unwrap();
                    Endpoint::new(m, p)
                };
                let opts = AddConnectionOptions { id: Some(id.into()), from: ep(f), to: ep(t), ..Default::default() };
                doc = add_connection(&doc, opts).unwrap().document;
            }
            write(dir, "cycle.mdg", &doc);
        }
        "numeric" => {
            let mut doc = small_doc("Numbers");
            for id in ["b", "10", "a", "9", "10a"] {
                let opts = AddModuleOptions {
                    id: Some(id.into()),
                    parent_id: "root".into(),
                    name: format!("N{id}"),
                    ..Default::default()
                };
                doc = add_module(&doc, opts).unwrap().document;
            }
            write(dir, "num.mdg", &doc);
        }
        "description" => {
            write(dir, "arch.mdg", &create_authentication_example_v1());
            std::fs::write(dir.join("desc.md"), "# From file\n\nLoaded from **disk**.\n").unwrap();
        }
        other => panic!("unknown setup {other}"),
    }
}

/// Replaces the working directory and generated UUIDs, like the corpus
/// generator does.
fn normalize(text: &str, dir: &Path) -> String {
    let text = text.replace(&dir.display().to_string(), "<dir>");
    let mut rest = text.as_str();
    let mut out = String::with_capacity(text.len());
    'outer: while !rest.is_empty() {
        for prefix in ["mod-", "port-", "conn-"] {
            if rest.starts_with(prefix) && rest.len() >= prefix.len() + 36 {
                let candidate = &rest[prefix.len()..prefix.len() + 36];
                let is_uuid = candidate.char_indices().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == '-'
                    } else {
                        c.is_ascii_hexdigit() && !c.is_ascii_uppercase()
                    }
                });
                let boundary_before = out.chars().last().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                if is_uuid && boundary_before {
                    out.push_str(prefix);
                    out.push_str("<uuid>");
                    rest = &rest[prefix.len() + 36..];
                    continue 'outer;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Cases whose help text lists request/response port options and commands
/// the TypeScript CLI never had. Only their output text is exempt; exit codes
/// are still compared. The new commands are covered in `cli.rs`.
const EXTENDED_HELP: [&str; 3] = ["port without subcommand", "port help flag", "help port add"];

#[test]
fn matches_the_typescript_cli() {
    let corpus =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/cases.json")).unwrap();
    let cases: Vec<Case> = serde_json::from_str(&corpus).unwrap();
    let binary = env!("CARGO_BIN_EXE_mdg");
    let mut failures: Vec<String> = Vec::new();
    let mut invocations = 0;

    for case in &cases {
        let dir = tempfile::tempdir().unwrap();
        setup(&case.setup, dir.path());
        for (step, expected) in case.steps.iter().zip(&case.results) {
            invocations += 1;
            let mut child = Command::new(binary)
                .args(&step.args)
                .current_dir(dir.path())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(step.stdin.as_deref().unwrap_or("").as_bytes()).unwrap();
            let output = child.wait_with_output().unwrap();
            let code = output.status.code().unwrap_or(-1);
            let stdout = normalize(&String::from_utf8_lossy(&output.stdout), dir.path());
            let stderr = normalize(&String::from_utf8_lossy(&output.stderr), dir.path());
            let label = format!("[{}] mdg {}", case.name, step.args.join(" "));
            if code != expected.code {
                failures.push(format!("{label}\n  exit code {code}, expected {}", expected.code));
            }
            if EXTENDED_HELP.contains(&case.name.as_str()) {
                continue;
            }
            if stdout != expected.stdout {
                failures.push(format!("{label}\n  stdout:\n{stdout}\n  expected:\n{}", expected.stdout));
            }
            if stderr != expected.stderr {
                failures.push(format!("{label}\n  stderr:\n{stderr}\n  expected:\n{}", expected.stderr));
            }
        }
        for file in &case.files {
            let path = dir.path().join(file);
            let actual = std::fs::read(&path).ok();
            match case.written.get(file) {
                Some(serde_json::Value::Null) | None => {
                    if actual.is_some() {
                        failures.push(format!("[{}] {file} should not exist", case.name));
                    }
                }
                Some(serde_json::Value::String(expected)) => {
                    let Some(actual) = actual else {
                        failures.push(format!("[{}] {file} was not written", case.name));
                        continue;
                    };
                    let same = if file.ends_with(".mdg") {
                        !case.bytes || base64::engine::general_purpose::STANDARD.decode(expected).unwrap() == actual
                    } else {
                        String::from_utf8_lossy(&actual) == *expected
                    };
                    if !same {
                        failures.push(format!("[{}] {file} differs from the TypeScript output", case.name));
                    }
                }
                Some(other) => panic!("bad corpus entry {other}"),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {invocations} invocations differ:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
