//! MDG desktop editor. The Rust session owns the document, history and
//! files; the webview renders the [`View`] each command returns and calls
//! back into these commands. Rejected edits come back as `view.error`.

use std::path::PathBuf;
use std::sync::Mutex;

use mdg_core::{
    rename_module, rename_port, set_port_kind, update_metadata, update_module_description, update_port_description,
    update_port_response, Endpoint, PortDirection, PortKind, Position,
};
use mdg_session::{ConnectRole, RouteChoices, RouteMode, RoutePreview, RouteRequest, Selection, Session, View};
use serde::{Deserialize, Serialize};
use tauri::State;

pub struct AppState(Mutex<Session>);

type Shared<'a> = State<'a, AppState>;

/// Runs `f` on the session and returns the resulting view.
fn with(state: &Shared<'_>, f: impl FnOnce(&mut Session)) -> View {
    let mut session = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut session);
    session.view()
}

#[derive(Serialize)]
pub struct Outcome {
    ok: bool,
    view: View,
}

fn outcome(state: &Shared<'_>, f: impl FnOnce(&mut Session) -> bool) -> Outcome {
    let mut ok = false;
    let view = with(state, |s| ok = f(s));
    Outcome { ok, view }
}

#[derive(Deserialize)]
pub struct PositionChange {
    key: String,
    x: f64,
    y: f64,
}

fn changes(list: Vec<PositionChange>) -> Vec<(String, Position)> {
    list.into_iter().map(|c| (c.key, Position::new(c.x, c.y))).collect()
}

// ---------------------------------------------------------------- state and files

#[tauri::command]
fn get_view(state: Shared<'_>) -> View {
    with(&state, |_| {})
}

#[tauri::command]
fn new_document(state: Shared<'_>) -> View {
    with(&state, Session::reset)
}

#[tauri::command]
fn open_file(state: Shared<'_>, path: PathBuf) -> Outcome {
    outcome(&state, |s| s.open_file(&path))
}

#[tauri::command]
fn save_to(state: Shared<'_>, path: PathBuf) -> Outcome {
    outcome(&state, |s| s.save_to(&path))
}

#[tauri::command]
fn export_markdown(state: Shared<'_>, path: PathBuf) -> Outcome {
    outcome(&state, |s| s.export_markdown(&path))
}

// ---------------------------------------------------------------- navigation and history

#[tauri::command]
fn navigate(state: Shared<'_>, module_id: String) -> View {
    with(&state, |s| s.navigate(&module_id))
}

#[tauri::command]
fn select(state: Shared<'_>, selection: Option<Selection>) -> View {
    with(&state, |s| s.select(selection))
}

#[tauri::command]
fn set_error(state: Shared<'_>, message: Option<String>) -> View {
    with(&state, |s| s.set_error(message))
}

#[tauri::command]
fn undo(state: Shared<'_>) -> View {
    with(&state, Session::undo)
}

#[tauri::command]
fn redo(state: Shared<'_>) -> View {
    with(&state, Session::redo)
}

// ---------------------------------------------------------------- edits

#[tauri::command]
fn set_title(state: Shared<'_>, title: String) -> View {
    with(&state, |s| {
        s.apply(None, |d| Ok(update_metadata(d, Some(&title), None)));
    })
}

#[tauri::command]
fn rename_module_cmd(state: Shared<'_>, module_id: String, name: String) -> View {
    with(&state, |s| {
        s.apply(None, |d| rename_module(d, &module_id, &name));
    })
}

#[tauri::command]
fn set_module_description(state: Shared<'_>, module_id: String, text: String) -> View {
    let key = format!("desc:{module_id}");
    with(&state, |s| {
        s.apply(Some(&key), |d| update_module_description(d, &module_id, &text));
    })
}

#[tauri::command]
fn rename_port_cmd(state: Shared<'_>, module_id: String, port_id: String, name: String) -> View {
    with(&state, |s| {
        s.apply(None, |d| rename_port(d, &module_id, &port_id, &name));
    })
}

#[tauri::command]
fn set_port_description(state: Shared<'_>, module_id: String, port_id: String, text: String) -> View {
    let key = format!("desc:{module_id}:{port_id}");
    with(&state, |s| {
        s.apply(Some(&key), |d| update_port_description(d, &module_id, &port_id, &text));
    })
}

#[tauri::command]
fn set_port_response(state: Shared<'_>, module_id: String, port_id: String, text: String) -> View {
    let key = format!("response:{module_id}:{port_id}");
    with(&state, |s| {
        s.apply(Some(&key), |d| update_port_response(d, &module_id, &port_id, &text));
    })
}

#[tauri::command]
fn set_port_kind_cmd(state: Shared<'_>, module_id: String, port_id: String, kind: PortKind) -> View {
    with(&state, |s| {
        s.apply(None, |d| set_port_kind(d, &module_id, &port_id, kind));
    })
}

#[tauri::command]
fn add_child_module(state: Shared<'_>) -> View {
    with(&state, |s| {
        s.add_child_module(None);
    })
}

#[tauri::command]
fn add_port(state: Shared<'_>, module_id: String, direction: PortDirection, kind: PortKind) -> View {
    with(&state, |s| {
        s.add_module_port(&module_id, direction, kind);
    })
}

#[tauri::command]
fn connect(state: Shared<'_>, from: Endpoint, to: Endpoint) -> View {
    with(&state, |s| {
        s.connect(from, to);
    })
}

#[tauri::command]
fn delete_module(state: Shared<'_>, module_id: String, cascade: bool) -> View {
    with(&state, |s| {
        s.delete_module(&module_id, cascade);
    })
}

#[tauri::command]
fn delete_port(state: Shared<'_>, module_id: String, port_id: String, cascade: bool) -> View {
    with(&state, |s| {
        s.delete_port(&module_id, &port_id, cascade);
    })
}

#[tauri::command]
fn delete_connection(state: Shared<'_>, connection_id: String) -> View {
    with(&state, |s| {
        s.delete_connection(&connection_id);
    })
}

#[tauri::command]
fn delete_selection(state: Shared<'_>) -> View {
    with(&state, |s| {
        s.delete_selection();
    })
}

// ---------------------------------------------------------------- layout

#[tauri::command]
fn set_positions(state: Shared<'_>, changes: Vec<PositionChange>) -> View {
    let changes = self::changes(changes);
    with(&state, |s| s.set_positions(&changes))
}

#[tauri::command]
fn apply_layout(state: Shared<'_>, positions: Vec<PositionChange>) -> View {
    let positions = changes(positions);
    with(&state, |s| s.apply_layout(&positions))
}

#[tauri::command]
fn connection_partners(state: Shared<'_>, endpoint: Endpoint, role: ConnectRole) -> Vec<Endpoint> {
    let session = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    session.connection_partners(&endpoint, role).unwrap_or_default()
}

// ---------------------------------------------------------------- cross-level routing

#[tauri::command]
fn route_choices(state: Shared<'_>, source: Endpoint, mode: Option<RouteMode>) -> Result<RouteChoices, String> {
    let session = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    session.route_choices(&source, mode).map_err(|e| e.message)
}

#[tauri::command]
fn preview_route(state: Shared<'_>, request: RouteRequest) -> RoutePreview {
    let mut session = state.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    session.preview_route(&request)
}

#[tauri::command]
fn commit_route(state: Shared<'_>, token: u64) -> Outcome {
    outcome(&state, |s| s.commit_route(token))
}

/// Starts the editor. A `.mdg` path on the command line (e.g. from a file
/// association) is opened at startup.
pub fn run() {
    let mut session = Session::new();
    if let Some(path) = std::env::args_os().nth(1) {
        session.open_file(&PathBuf::from(path));
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState(Mutex::new(session)))
        .invoke_handler(tauri::generate_handler![
            get_view,
            new_document,
            open_file,
            save_to,
            export_markdown,
            navigate,
            select,
            set_error,
            undo,
            redo,
            set_title,
            rename_module_cmd,
            set_module_description,
            rename_port_cmd,
            set_port_description,
            set_port_response,
            set_port_kind_cmd,
            add_child_module,
            add_port,
            connect,
            delete_module,
            delete_port,
            delete_connection,
            delete_selection,
            set_positions,
            apply_layout,
            connection_partners,
            route_choices,
            preview_route,
            commit_route,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the MDG editor");
}
