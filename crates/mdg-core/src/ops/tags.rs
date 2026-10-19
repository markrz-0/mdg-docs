use super::{require_name, update_module, CreateResult};
use crate::error::{MdgError, Result};
use crate::ids::{assert_valid_id, with_generator, IdGenerator, IdKind};
use crate::model::{is_valid_color, schema_version_for, Document, Tag};
use crate::queries::get_module;

/// Checks a `#rrggbb` colour and returns it in lowercase.
pub fn require_color(color: &str) -> Result<String> {
    let trimmed = crate::js::trim(color);
    if !is_valid_color(trimmed) {
        return Err(MdgError::argument(format!("Invalid colour \"{color}\". Use #rrggbb, e.g. #3b6fd8.")));
    }
    Ok(trimmed.to_ascii_lowercase())
}

pub fn get_tag<'a>(doc: &'a Document, tag_id: &str) -> Result<&'a Tag> {
    doc.find_tag(tag_id).ok_or_else(|| MdgError::not_found(format!("Tag \"{tag_id}\" does not exist.")))
}

/// Installs a changed document and raises its schema version if tags now
/// need it.
fn finish(mut document: Document) -> Document {
    document.format_version = schema_version_for(&document);
    document
}

#[derive(Default)]
pub struct AddTagOptions<'a> {
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    /// Explicit ID. Generated when omitted.
    pub id: Option<String>,
    pub id_generator: Option<&'a mut dyn IdGenerator>,
}

/// Adds a tag to the end of the document's tag list.
pub fn add_tag(doc: &Document, options: AddTagOptions<'_>) -> Result<CreateResult> {
    let id = match options.id {
        Some(id) => id,
        None => with_generator(options.id_generator, |g| g.generate(IdKind::Tag)),
    };
    assert_valid_id(&id, "tag")?;
    if doc.find_tag(&id).is_some() {
        return Err(MdgError::argument(format!("A tag with ID \"{id}\" already exists.")));
    }
    let tag = Tag { id: id.clone(), name: require_name(&options.name, "Tag")?, color: require_color(&options.color)? };
    let mut document = doc.clone();
    document.tags.push(tag);
    Ok(CreateResult { document: finish(document), id })
}

/// Renames a tag and/or changes its colour. Its ID never changes.
pub fn update_tag(doc: &Document, tag_id: &str, name: Option<&str>, color: Option<&str>) -> Result<Document> {
    get_tag(doc, tag_id)?;
    let name = name.map(|n| require_name(n, "Tag")).transpose()?;
    let color = color.map(require_color).transpose()?;
    let mut document = doc.clone();
    let tag = document.tags.iter_mut().find(|t| t.id == tag_id).expect("tag exists");
    if let Some(name) = name {
        tag.name = name;
    }
    if let Some(color) = color {
        tag.color = color;
    }
    Ok(finish(document))
}

/// Removes a tag from the document's list and from every module carrying it.
pub fn remove_tag(doc: &Document, tag_id: &str) -> Result<Document> {
    get_tag(doc, tag_id)?;
    let mut document = doc.clone();
    document.tags.retain(|t| t.id != tag_id);
    let tagged: Vec<String> =
        doc.modules.iter().filter(|(_, m)| m.tags.iter().any(|t| t == tag_id)).map(|(id, _)| id.to_string()).collect();
    for module_id in tagged {
        document = update_module(&document, &module_id, |m| m.tags.retain(|t| t != tag_id))?;
    }
    Ok(finish(document))
}

/// Moves a tag to `index` in the tag list (clamped to the end).
pub fn move_tag(doc: &Document, tag_id: &str, index: usize) -> Result<Document> {
    get_tag(doc, tag_id)?;
    let mut document = doc.clone();
    let from = document.tags.iter().position(|t| t.id == tag_id).expect("tag exists");
    let tag = document.tags.remove(from);
    let index = index.min(document.tags.len());
    document.tags.insert(index, tag);
    Ok(finish(document))
}

/// Replaces a module's tags. Every tag must exist; duplicates are dropped.
pub fn set_module_tags(doc: &Document, module_id: &str, tag_ids: &[String]) -> Result<Document> {
    get_module(doc, module_id)?;
    let mut tags: Vec<String> = Vec::with_capacity(tag_ids.len());
    for tag_id in tag_ids {
        get_tag(doc, tag_id)?;
        if !tags.contains(tag_id) {
            tags.push(tag_id.clone());
        }
    }
    Ok(finish(update_module(doc, module_id, |m| m.tags = tags)?))
}

/// Adds tags to a module, keeping the ones it already carries.
pub fn tag_module(doc: &Document, module_id: &str, tag_ids: &[String]) -> Result<Document> {
    let mut tags = get_module(doc, module_id)?.tags.clone();
    tags.extend(tag_ids.iter().cloned());
    set_module_tags(doc, module_id, &tags)
}

/// Removes tags from a module. Tags it does not carry are ignored.
pub fn untag_module(doc: &Document, module_id: &str, tag_ids: &[String]) -> Result<Document> {
    for tag_id in tag_ids {
        get_tag(doc, tag_id)?;
    }
    let tags: Vec<String> = get_module(doc, module_id)?.tags.iter().filter(|t| !tag_ids.contains(t)).cloned().collect();
    set_module_tags(doc, module_id, &tags)
}
