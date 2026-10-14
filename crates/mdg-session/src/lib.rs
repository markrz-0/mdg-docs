//! Editor session for MDG: the state behind the desktop editor.
//!
//! The UI renders a [`View`] and calls session methods; every change to the
//! document goes through `mdg-core` operations, so the editor accepts and
//! rejects exactly what the CLI does.

pub mod layout;
mod routing;
mod session;
mod view;

pub use layout::{boundary_key, flatten_layout, position_owner, split_layout, BoundarySide, Positions};
pub use routing::{Candidate, RouteChoices, RouteMode, RoutePreview, RouteRequest};
pub use session::{next_position, Clock, Selection, Session, Snapshot, COALESCE, HISTORY_LIMIT, UNTITLED};
pub use view::{ConnectRole, ConnectionInfo, Details, DocumentView, ValidationSummary, View};
