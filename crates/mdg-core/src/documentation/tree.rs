use serde::Serialize;

use crate::error::Result;
use crate::model::Document;
use crate::queries::get_module;

#[derive(Debug, Clone, Default)]
pub struct TreeOptions {
    /// Start module. Defaults to the root.
    pub root_id: Option<String>,
    /// Levels below the start module to include. Unlimited when `None`.
    pub depth: Option<usize>,
    /// Append module IDs.
    pub ids: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeNode {
    pub id: String,
    pub name: String,
    /// IDs of the module's tags. Omitted when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub children: Vec<TreeNode>,
    /// Present (true) when children exist but were cut off by `depth`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

struct Frame<'a> {
    id: &'a str,
    prefix: String,
    last: bool,
    depth: usize,
}

fn push_children<'a>(
    doc: &'a Document,
    stack: &mut Vec<Frame<'a>>,
    id: &str,
    prefix: String,
    depth: usize,
) -> Result<()> {
    let children = &get_module(doc, id)?.child_ids;
    for (i, child) in children.iter().enumerate().rev() {
        stack.push(Frame { id: child, prefix: prefix.clone(), last: i == children.len() - 1, depth });
    }
    Ok(())
}

/// Renders the hierarchy with box-drawing characters. Iterative.
pub fn render_tree(doc: &Document, options: &TreeOptions) -> Result<String> {
    let start_id = options.root_id.as_deref().unwrap_or(&doc.root_id);
    let max_depth = options.depth.unwrap_or(usize::MAX);
    let label = |id: &str| -> Result<String> {
        let m = get_module(doc, id)?;
        let mut label = if options.ids { format!("{} [{}]", m.name, m.id) } else { m.name.clone() };
        for tag in &m.tags {
            label.push_str(&format!(" #{tag}"));
        }
        Ok(label)
    };
    let mut lines = vec![label(start_id)?];

    let mut stack: Vec<Frame<'_>> = Vec::new();
    if max_depth > 0 {
        push_children(doc, &mut stack, start_id, String::new(), 1)?;
    }
    while let Some(frame) = stack.pop() {
        let m = get_module(doc, frame.id)?;
        let more = if !m.child_ids.is_empty() && frame.depth >= max_depth {
            format!(" (+{})", m.child_ids.len())
        } else {
            String::new()
        };
        let branch = if frame.last { "└── " } else { "├── " };
        lines.push(format!("{}{branch}{}{more}", frame.prefix, label(frame.id)?));
        if frame.depth < max_depth {
            let prefix = format!("{}{}", frame.prefix, if frame.last { "    " } else { "│   " });
            push_children(doc, &mut stack, frame.id, prefix, frame.depth + 1)?;
        }
    }
    Ok(lines.join("\n"))
}

/// Nested JSON form of the hierarchy. Built iteratively.
pub fn build_tree(doc: &Document, options: &TreeOptions) -> Result<TreeNode> {
    let start_id = options.root_id.as_deref().unwrap_or(&doc.root_id);
    let max_depth = options.depth.unwrap_or(usize::MAX);

    // Build a flat arena first (children are filled in by the iterative
    // walk), then assemble the nested tree bottom-up without recursion.
    struct Slot {
        id: String,
        name: String,
        tags: Vec<String>,
        children: Vec<usize>,
        truncated: bool,
    }
    let make = |id: &str| -> Result<Slot> {
        let m = get_module(doc, id)?;
        Ok(Slot { id: id.to_string(), name: m.name.clone(), tags: m.tags.clone(), children: vec![], truncated: false })
    };
    let mut arena = vec![make(start_id)?];
    let mut stack: Vec<(usize, usize)> = vec![(0, 0)];
    while let Some((index, depth)) = stack.pop() {
        let child_ids = &get_module(doc, &arena[index].id)?.child_ids;
        if child_ids.is_empty() {
            continue;
        }
        if depth >= max_depth {
            arena[index].truncated = true;
            continue;
        }
        for child_id in child_ids {
            let child = arena.len();
            arena.push(make(child_id)?);
            arena[index].children.push(child);
            stack.push((child, depth + 1));
        }
    }
    // Children always have larger indices than their parent.
    let mut built: Vec<Option<TreeNode>> = (0..arena.len()).map(|_| None).collect();
    for index in (0..arena.len()).rev() {
        let slot = &arena[index];
        let children = slot.children.iter().map(|&c| built[c].take().expect("child built")).collect();
        built[index] = Some(TreeNode {
            id: slot.id.clone(),
            name: slot.name.clone(),
            tags: slot.tags.clone(),
            children,
            truncated: slot.truncated.then_some(true),
        });
    }
    Ok(built[0].take().expect("root built"))
}
