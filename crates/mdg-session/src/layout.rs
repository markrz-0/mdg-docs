//! The editor keeps every canvas position in one flat map so undo/redo can
//! snapshot layout as a single value. Child modules use their module ID; a
//! module's own boundary panels use `<moduleId>\0input` / `<moduleId>\0output`
//! (module IDs never contain NUL). The file stores these separately as
//! `layout.positions` and `layout.boundaries`.

use mdg_core::js::compare_utf16;
use mdg_core::{BoundaryLayout, JsMap, Layout, Position};

pub type Positions = JsMap<Position>;

const SEPARATOR: char = '\u{0}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundarySide {
    Input,
    Output,
}

impl BoundarySide {
    fn as_str(self) -> &'static str {
        match self {
            BoundarySide::Input => "input",
            BoundarySide::Output => "output",
        }
    }
}

pub fn boundary_key(module_id: &str, side: BoundarySide) -> String {
    format!("{module_id}{SEPARATOR}{}", side.as_str())
}

/// The module a position key belongs to.
pub fn position_owner(key: &str) -> &str {
    key.split(SEPARATOR).next().unwrap_or(key)
}

/// Flattens file layout into the editor's position map.
pub fn flatten_layout(layout: Option<&Layout>) -> Positions {
    let mut flat = Positions::new();
    let Some(layout) = layout else { return flat };
    for (id, p) in layout.positions.iter() {
        flat.insert(id, *p);
    }
    for (id, b) in layout.boundaries.iter().flat_map(|b| b.iter()) {
        if let Some(input) = b.input {
            flat.insert(boundary_key(id, BoundarySide::Input), input);
        }
        if let Some(output) = b.output {
            flat.insert(boundary_key(id, BoundarySide::Output), output);
        }
    }
    flat
}

/// Splits the editor's position map back into file layout, keeping only
/// entries whose module passes `keep`. `None` when nothing is left.
pub fn split_layout(flat: &Positions, keep: impl Fn(&str) -> bool) -> Option<Layout> {
    let mut keys = flat.keys();
    keys.sort_by(|a, b| compare_utf16(a, b));
    let mut layout = Layout::default();
    let mut boundaries: JsMap<BoundaryLayout> = JsMap::new();
    for key in keys {
        let owner = position_owner(key);
        if !keep(owner) {
            continue;
        }
        let p = *flat.get(key).expect("key exists");
        if owner == key {
            layout.positions.insert(key, p);
            continue;
        }
        let mut entry = boundaries.get(owner).copied().unwrap_or_default();
        match &key[owner.len() + 1..] {
            "input" => entry.input = Some(p),
            "output" => entry.output = Some(p),
            _ => continue,
        }
        boundaries.insert(owner, entry);
    }
    if !boundaries.is_empty() {
        layout.boundaries = Some(boundaries);
    }
    if layout.positions.is_empty() && layout.boundaries.is_none() {
        None
    } else {
        Some(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_and_splits_round_trip() {
        let mut positions = JsMap::new();
        positions.insert("a", Position::new(1.0, 2.0));
        let mut boundaries = JsMap::new();
        boundaries.insert("p", BoundaryLayout { input: Some(Position::new(-5.0, 0.0)), output: None });
        let layout = Layout { positions, boundaries: Some(boundaries) };
        let flat = flatten_layout(Some(&layout));
        assert_eq!(flat.keys(), vec!["a", "p\u{0}input"]);
        assert_eq!(position_owner("p\u{0}input"), "p");
        assert_eq!(split_layout(&flat, |_| true), Some(layout));
        assert_eq!(split_layout(&flat, |_| false), None);
    }
}
