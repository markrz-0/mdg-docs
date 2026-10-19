//! Editor state. Kept deliberately separate:
//! - `document`: canonical model, changed only through core operations;
//! - `current_module_id`: navigation (whose internal graph is shown);
//! - `selection`: selected module / port / connection;
//! - `positions`: presentation metadata (persisted as the file's layout);
//! - `past` / `future`: undo history of (document, positions) snapshots;
//! - `dirty`: unsaved changes (current state differs from the saved snapshot).
//!
//! Snapshots are reference-counted and documents share unchanged modules,
//! so each undo step costs a few pointers. "Unchanged" is pointer identity:
//! undoing back to the saved snapshot makes the document clean again.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mdg_core::{
    add_connection, add_module, add_port, create_document, decode_mdg, encode_mdg, export_document_markdown,
    find_connection, find_module, find_port, get_module, remove_connection, remove_module, remove_port,
    validate_document, write_file_atomic, AddConnectionOptions, AddModuleOptions, AddPortOptions,
    CreateDocumentOptions, Document, EncodeOptions, Endpoint, MdgError, PortDirection, PortKind, Position, Result,
    ValidationOptions,
};
use serde::{Deserialize, Serialize};

use crate::layout::{flatten_layout, position_owner, split_layout, Positions};

pub const HISTORY_LIMIT: usize = 200;
/// Consecutive edits with the same coalesce key within this window form one undo step.
pub const COALESCE: Duration = Duration::from_millis(1500);
pub const UNTITLED: &str = "untitled.mdg";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Selection {
    #[serde(rename_all = "camelCase")]
    Module { module_id: String },
    #[serde(rename_all = "camelCase")]
    Port { module_id: String, port_id: String },
    #[serde(rename_all = "camelCase")]
    Connection { connection_id: String },
}

/// One undo step: the canonical document plus layout. Both are immutable.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub document: Arc<Document>,
    pub positions: Arc<Positions>,
}

impl Snapshot {
    fn same_as(&self, other: &Snapshot) -> bool {
        Arc::ptr_eq(&self.document, &other.document) && Arc::ptr_eq(&self.positions, &other.positions)
    }
}

/// Source of time for undo coalescing (replaceable in tests).
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// A route computed by a preview, committed only if the document is still
/// the one it was computed against.
#[derive(Debug, Clone)]
pub(crate) struct PendingRoute {
    pub token: u64,
    pub base: Arc<Document>,
    pub result: mdg_core::RouteResult,
}

pub struct Session {
    pub(crate) document: Arc<Document>,
    pub(crate) positions: Arc<Positions>,
    pub(crate) file_name: String,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) dirty: bool,
    pub(crate) current_module_id: String,
    pub(crate) selection: Option<Selection>,
    pub(crate) error: Option<String>,
    pub(crate) past: Vec<Snapshot>,
    pub(crate) future: Vec<Snapshot>,
    /// State at the last load or save; `dirty` compares against it.
    pub(crate) saved: Snapshot,
    /// Incremented by auto-layout so the canvas can re-fit the view.
    pub(crate) layout_version: u64,
    pub(crate) pending_route: Option<PendingRoute>,
    pub(crate) next_token: u64,
    last_key: Option<String>,
    last_time: Option<Instant>,
    clock: Clock,
}

fn new_document() -> Document {
    create_document(CreateDocumentOptions {
        title: Some("Untitled".into()),
        root_name: Some("System".into()),
        ..Default::default()
    })
    .expect("a new document is always valid")
}

/// Finds a free grid slot inside the current canvas for a new module.
pub fn next_position(doc: &Document, parent_id: &str, positions: &Positions) -> Position {
    let taken: Vec<Position> = get_module(doc, parent_id)
        .map(|m| m.child_ids.iter().filter_map(|id| positions.get(id).copied()).collect())
        .unwrap_or_default();
    (0..)
        .map(|i| Position::new(f64::from(i % 3) * 380.0, f64::from(i / 3) * 220.0))
        .find(|c| !taken.iter().any(|p| (p.x - c.x).abs() < 160.0 && (p.y - c.y).abs() < 110.0))
        .expect("an infinite grid has a free slot")
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(Instant::now))
    }

    pub fn with_clock(clock: Clock) -> Self {
        let document = Arc::new(new_document());
        let positions = Arc::new(Positions::new());
        Session {
            current_module_id: document.root_id.clone(),
            saved: Snapshot { document: document.clone(), positions: positions.clone() },
            document,
            positions,
            file_name: UNTITLED.to_string(),
            file_path: None,
            dirty: false,
            selection: None,
            error: None,
            past: vec![],
            future: vec![],
            layout_version: 0,
            pending_route: None,
            next_token: 0,
            last_key: None,
            last_time: None,
            clock,
        }
    }

    // ------------------------------------------------------------ accessors

    pub fn document(&self) -> &Arc<Document> {
        &self.document
    }

    pub fn positions(&self) -> &Arc<Positions> {
        &self.positions
    }

    pub fn current_module_id(&self) -> &str {
        &self.current_module_id
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    pub fn past_len(&self) -> usize {
        self.past.len()
    }

    pub fn future_len(&self) -> usize {
        self.future.len()
    }

    pub fn layout_version(&self) -> u64 {
        self.layout_version
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { document: self.document.clone(), positions: self.positions.clone() }
    }

    // ------------------------------------------------------------ history

    /// Drops navigation/selection that points at removed elements.
    fn normalize(&mut self) {
        let doc = &self.document;
        if find_module(doc, &self.current_module_id).is_none() {
            self.current_module_id = doc.root_id.clone();
        }
        let keep = match &self.selection {
            None => true,
            Some(Selection::Module { module_id }) => find_module(doc, module_id).is_some(),
            Some(Selection::Port { module_id, port_id }) => {
                find_module(doc, module_id).is_some_and(|m| find_port(m, port_id).is_some())
            }
            Some(Selection::Connection { connection_id }) => find_connection(doc, connection_id).is_some(),
        };
        if !keep {
            self.selection = None;
        }
    }

    /// Records the current state in history and installs the new one.
    fn commit(
        &mut self,
        document: Option<Arc<Document>>,
        positions: Option<Arc<Positions>>,
        coalesce_key: Option<&str>,
    ) {
        let previous = self.snapshot();
        let document_changed = document.is_some();
        let document = document.unwrap_or_else(|| self.document.clone());
        let mut positions = positions.unwrap_or_else(|| self.positions.clone());
        if document_changed {
            let stale: Vec<String> = positions
                .keys()
                .into_iter()
                .filter(|key| find_module(&document, position_owner(key)).is_none())
                .map(str::to_string)
                .collect();
            if !stale.is_empty() {
                let mut kept = (*positions).clone();
                for key in &stale {
                    kept.remove(key);
                }
                positions = Arc::new(kept);
            }
        }
        let now = (self.clock)();
        let merge = coalesce_key.is_some()
            && coalesce_key == self.last_key.as_deref()
            && self.last_time.is_some_and(|t| now.saturating_duration_since(t) < COALESCE);
        self.last_key = coalesce_key.map(str::to_string);
        self.last_time = Some(now);
        if !merge {
            self.past.push(previous);
            if self.past.len() > HISTORY_LIMIT {
                let excess = self.past.len() - HISTORY_LIMIT;
                self.past.drain(..excess);
            }
        }
        self.future.clear();
        self.document = document;
        self.positions = positions;
        self.dirty = !self.snapshot().same_as(&self.saved);
        self.error = None;
        self.normalize();
    }

    /// Runs an operation as one undo step. On failure, records the error
    /// and returns `None`; the document is unchanged.
    fn run<T>(
        &mut self,
        coalesce_key: Option<&str>,
        f: impl FnOnce(&Self) -> Result<(Document, T, Option<Positions>)>,
    ) -> Option<T> {
        match f(self) {
            Ok((document, value, positions)) => {
                self.commit(Some(Arc::new(document)), positions.map(Arc::new), coalesce_key);
                Some(value)
            }
            Err(error) => {
                self.error = Some(error.message);
                None
            }
        }
    }

    /// Applies a core operation as one undo step. Returns false (and records
    /// the error) when it fails. Edits sharing `coalesce_key` in quick
    /// succession (e.g. typing a description) merge into a single step.
    pub fn apply(&mut self, coalesce_key: Option<&str>, operation: impl FnOnce(&Document) -> Result<Document>) -> bool {
        self.run(coalesce_key, |s| Ok((operation(&s.document)?, (), None))).is_some()
    }

    fn restore(&mut self, target: Snapshot) {
        self.last_key = None;
        self.document = target.document;
        self.positions = target.positions;
        self.dirty = !self.snapshot().same_as(&self.saved);
        self.error = None;
        self.normalize();
    }

    pub fn undo(&mut self) {
        let Some(previous) = self.past.pop() else { return };
        self.future.insert(0, self.snapshot());
        self.restore(previous);
    }

    pub fn redo(&mut self) {
        if self.future.is_empty() {
            return;
        }
        let next = self.future.remove(0);
        self.past.push(self.snapshot());
        self.restore(next);
    }

    // ------------------------------------------------------------ documents

    fn install_fresh(
        &mut self,
        document: Document,
        positions: Positions,
        file_name: String,
        file_path: Option<PathBuf>,
    ) {
        let document = Arc::new(document);
        let positions = Arc::new(positions);
        self.last_key = None;
        self.current_module_id = document.root_id.clone();
        self.saved = Snapshot { document: document.clone(), positions: positions.clone() };
        self.document = document;
        self.positions = positions;
        self.past.clear();
        self.future.clear();
        self.dirty = false;
        self.selection = None;
        self.error = None;
        self.pending_route = None;
        self.file_name = file_name;
        self.file_path = file_path;
    }

    /// Replaces everything with a freshly loaded document. Its layout moves
    /// into the editor's position map.
    pub fn load(&mut self, mut document: Document, file_name: impl Into<String>, file_path: Option<PathBuf>) {
        let positions = flatten_layout(document.layout.as_ref());
        document.layout = None;
        self.install_fresh(document, positions, file_name.into(), file_path);
    }

    /// Starts a new, empty document.
    pub fn reset(&mut self) {
        self.install_fresh(new_document(), Positions::new(), UNTITLED.to_string(), None);
    }

    pub fn mark_saved(&mut self, file_name: impl Into<String>, file_path: Option<PathBuf>) {
        self.last_key = None; // never merge a later edit into the step that was saved
        self.file_name = file_name.into();
        self.file_path = file_path;
        self.dirty = false;
        self.saved = self.snapshot();
    }

    /// The canonical document with current positions attached as layout.
    pub fn document_for_save(&self) -> Document {
        let mut doc = (*self.document).clone();
        doc.layout = split_layout(&self.positions, |id| find_module(&self.document, id).is_some());
        doc
    }

    /// Opens a `.mdg` file. Structurally invalid documents are refused (the
    /// error points to `mdg validate`). Returns false on failure.
    pub fn open_file(&mut self, path: &Path) -> bool {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| UNTITLED.to_string());
        let doc =
            match std::fs::read(path).map_err(|e| MdgError::io(e.to_string())).and_then(|bytes| decode_mdg(&bytes)) {
                Ok(doc) => doc,
                Err(error) => {
                    self.error = Some(format!("Cannot open file: {}", error.message));
                    return false;
                }
            };
        let result = validate_document(&doc, ValidationOptions { warnings: false });
        if !result.valid {
            let first: Vec<&str> = result.errors.iter().take(3).map(|e| e.message.as_str()).collect();
            self.error = Some(format!(
                "{name} has {} structural error(s) and cannot be edited safely. {} Run `mdg validate` for details.",
                result.errors.len(),
                first.join(" ")
            ));
            return false;
        }
        self.load(doc, name, Some(path.to_path_buf()));
        true
    }

    /// Saves to `path` atomically and remembers it as the current file.
    pub fn save_to(&mut self, path: &Path) -> bool {
        let bytes = match encode_mdg(&self.document_for_save(), EncodeOptions::default()) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(format!("Cannot save: {}", error.message));
                return false;
            }
        };
        if let Err(error) = write_file_atomic(path, &bytes) {
            self.error = Some(format!("Save failed: {}", error.message));
            return false;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| UNTITLED.to_string());
        self.mark_saved(name, Some(path.to_path_buf()));
        true
    }

    /// Writes the whole hierarchy as one Markdown file.
    pub fn export_markdown(&mut self, path: &Path) -> bool {
        let result = export_document_markdown(&self.document)
            .and_then(|text| std::fs::write(path, text).map_err(|e| MdgError::io(e.to_string())));
        if let Err(error) = result {
            self.error = Some(format!("Export failed: {}", error.message));
            return false;
        }
        true
    }

    /// Default name for a Markdown export of the current file.
    pub fn markdown_file_name(&self) -> String {
        let name = &self.file_name;
        let stem = if name.to_lowercase().ends_with(".mdg") { &name[..name.len() - 4] } else { name.as_str() };
        format!("{stem}.md")
    }

    // ------------------------------------------------------------ navigation

    pub fn navigate(&mut self, module_id: &str) {
        if find_module(&self.document, module_id).is_none() {
            return;
        }
        self.current_module_id = module_id.to_string();
        self.selection = None;
    }

    pub fn select(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    pub fn set_error(&mut self, message: Option<String>) {
        self.error = message;
    }

    // ------------------------------------------------------------ layout

    /// Moves nodes. Several changes form one undo step (a multi-node drag).
    pub fn set_positions(&mut self, changes: &[(String, Position)]) {
        let changed: Vec<&(String, Position)> = changes
            .iter()
            .filter(|(key, next)| self.positions.get(key).is_none_or(|old| old.x != next.x || old.y != next.y))
            .collect();
        if changed.is_empty() {
            return;
        }
        let mut positions = (*self.positions).clone();
        for (key, p) in changed {
            positions.insert(key.clone(), *p);
        }
        self.commit(None, Some(Arc::new(positions)), None);
    }

    /// Installs auto-layout positions as one undo step and asks the canvas
    /// to re-fit the view.
    pub fn apply_layout(&mut self, computed: &[(String, Position)]) {
        if computed.is_empty() {
            return;
        }
        let mut positions = (*self.positions).clone();
        for (key, p) in computed {
            positions.insert(key.clone(), *p);
        }
        self.commit(None, Some(Arc::new(positions)), None);
        self.layout_version += 1;
    }

    // ------------------------------------------------------------ edits

    /// Adds a module inside the open one, at `at` or else the next free grid slot.
    pub fn add_child_module(&mut self, name: Option<&str>, at: Option<Position>) -> Option<String> {
        let parent = self.current_module_id.clone();
        let name = name.unwrap_or("New module").to_string();
        let id = self.run(None, |s| {
            let result =
                add_module(&s.document, AddModuleOptions { parent_id: parent.clone(), name, ..Default::default() })?;
            let mut positions = (*s.positions).clone();
            let position = at.unwrap_or_else(|| next_position(&s.document, &parent, &s.positions));
            positions.insert(result.id.clone(), position);
            Ok((result.document, result.id, Some(positions)))
        })?;
        self.selection = Some(Selection::Module { module_id: id.clone() });
        Some(id)
    }

    pub fn add_module_port(&mut self, module_id: &str, direction: PortDirection, kind: PortKind) -> Option<String> {
        let name = match (kind, direction) {
            (PortKind::Event, PortDirection::Input) => "New input",
            (PortKind::Event, PortDirection::Output) => "New output",
            (PortKind::Request, PortDirection::Input) => "New handled request",
            (PortKind::Request, PortDirection::Output) => "New request",
        };
        let id = self.run(None, |s| {
            let result = add_port(
                &s.document,
                module_id,
                direction,
                AddPortOptions { name: name.into(), kind, ..Default::default() },
            )?;
            Ok((result.document, result.id, None))
        })?;
        self.selection = Some(Selection::Port { module_id: module_id.to_string(), port_id: id.clone() });
        Some(id)
    }

    /// Connects two ports in the open module's internal graph.
    pub fn connect(&mut self, from: Endpoint, to: Endpoint) -> bool {
        let container_id = self.current_module_id.clone();
        let id = self.run(None, |s| {
            let options = AddConnectionOptions { from, to, container_id: Some(container_id), ..Default::default() };
            let result = add_connection(&s.document, options)?;
            Ok((result.document, result.id, None))
        });
        match id {
            Some(id) => {
                self.selection = Some(Selection::Connection { connection_id: id });
                true
            }
            None => false,
        }
    }

    pub fn delete_module(&mut self, module_id: &str, cascade: bool) -> bool {
        self.apply(None, |d| remove_module(d, module_id, cascade))
    }

    pub fn delete_port(&mut self, module_id: &str, port_id: &str, cascade: bool) -> bool {
        self.apply(None, |d| remove_port(d, module_id, port_id, cascade))
    }

    pub fn delete_connection(&mut self, connection_id: &str) -> bool {
        self.apply(None, |d| remove_connection(d, connection_id))
    }

    /// Deletes the selection (safe, non-cascading), as the Delete key does.
    pub fn delete_selection(&mut self) -> bool {
        match self.selection.clone() {
            None => false,
            Some(Selection::Connection { connection_id }) => self.delete_connection(&connection_id),
            Some(Selection::Port { module_id, port_id }) => self.delete_port(&module_id, &port_id, false),
            Some(Selection::Module { module_id }) => self.delete_module(&module_id, false),
        }
    }

    /// Commits a previewed route, if the document has not changed since,
    /// then shows the graph where the route joins (the lowest common
    /// ancestor) with the final connection selected.
    pub(crate) fn apply_route(&mut self, pending: PendingRoute) -> bool {
        let ok = self
            .run(None, |s| {
                if !Arc::ptr_eq(&s.document, &pending.base) {
                    return Err(MdgError::operation(
                        "The document changed since the route was previewed. Preview it again.",
                    ));
                }
                Ok((pending.result.document.clone(), (), None))
            })
            .is_some();
        if !ok {
            return false;
        }
        let plan = &pending.result.plan;
        self.current_module_id = plan.lowest_common_ancestor_id.clone();
        self.selection = plan
            .steps
            .last()
            .and_then(|s| s.connection_id())
            .map(|id| Selection::Connection { connection_id: id.to_string() });
        true
    }
}
