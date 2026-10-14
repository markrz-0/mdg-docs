use std::sync::Arc;

use crate::error::Result;
use crate::ids::{assert_valid_id, with_generator, IdGenerator, IdKind};
use crate::js;
use crate::jsmap::JsMap;
use crate::model::{Document, Metadata, Module, SCHEMA_VERSION};

#[derive(Default)]
pub struct CreateDocumentOptions<'a> {
    /// Document title. Default "Untitled".
    pub title: Option<String>,
    /// Optional document-level description (metadata).
    pub description: Option<String>,
    /// Root module name. Defaults to the title.
    pub root_name: Option<String>,
    /// Root module Markdown description.
    pub root_description: Option<String>,
    /// Explicit root ID. Generated when omitted.
    pub root_id: Option<String>,
    pub id_generator: Option<&'a mut dyn IdGenerator>,
}

fn non_empty_trimmed(value: &Option<String>) -> Option<String> {
    value.as_deref().map(js::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Creates a new document containing only a root module.
pub fn create_document(options: CreateDocumentOptions<'_>) -> Result<Document> {
    let title = non_empty_trimmed(&options.title).unwrap_or_else(|| "Untitled".to_string());
    let root_id = match options.root_id {
        Some(id) => id,
        None => with_generator(options.id_generator, |g| g.generate(IdKind::Module)),
    };
    assert_valid_id(&root_id, "module")?;
    let root = Module {
        id: root_id.clone(),
        parent_id: None,
        name: non_empty_trimmed(&options.root_name).unwrap_or_else(|| title.clone()),
        description: options.root_description.unwrap_or_default(),
        inputs: vec![],
        outputs: vec![],
        child_ids: vec![],
        connections: vec![],
    };
    let mut modules = JsMap::new();
    modules.insert(root_id.clone(), Arc::new(root));
    Ok(Document {
        format_version: SCHEMA_VERSION.to_string(),
        root_id,
        modules,
        metadata: Metadata { title, description: options.description, extra: JsMap::new() },
        layout: None,
        extensions: None,
    })
}

/// Updates document-level metadata (title / description).
pub fn update_metadata(doc: &Document, title: Option<&str>, description: Option<&str>) -> Document {
    let mut next = doc.clone();
    if let Some(title) = title {
        next.metadata.title = title.to_string();
    }
    if let Some(description) = description {
        next.metadata.description = Some(description.to_string());
    }
    next
}
