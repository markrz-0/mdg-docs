//! ID resolution: an exact ID always wins; otherwise a unique ID prefix is
//! accepted (convenient for generated UUID IDs). Ambiguous prefixes fail.
//! Names are never used to resolve IDs.

use mdg_core::{find_module, get_module, Document, Endpoint, MdgError, Result};

fn resolve(kind: &str, input: &str, candidates: &[&str], hint: &str) -> Result<String> {
    if candidates.contains(&input) {
        return Ok(input.to_string());
    }
    let matches: Vec<&str> = candidates.iter().copied().filter(|id| id.starts_with(input)).collect();
    match matches.len() {
        1 => Ok(matches[0].to_string()),
        0 => Err(MdgError::not_found(format!("{kind} \"{input}\" does not exist. {hint}"))),
        n => {
            let shown = matches.iter().take(10).copied().collect::<Vec<_>>().join(", ");
            let more = if n > 10 { ", ..." } else { "" };
            Err(MdgError::argument(format!("{kind} ID \"{input}\" is ambiguous; it matches {n} IDs: {shown}{more}.")))
        }
    }
}

pub fn resolve_module_id(doc: &Document, input: &str) -> Result<String> {
    if find_module(doc, input).is_some() {
        return Ok(input.to_string());
    }
    resolve("Module", input, &doc.modules.keys(), "Run `mdg tree <file> --ids` to list module IDs.")
}

pub fn resolve_port_id(doc: &Document, module_id: &str, input: &str) -> Result<String> {
    let module = get_module(doc, module_id)?;
    let ids: Vec<&str> = module.all_ports().map(|p| p.id.as_str()).collect();
    resolve(
        &format!("Port on module \"{module_id}\""),
        input,
        &ids,
        &format!("Run `mdg inputs <file> {module_id}` or `mdg outputs <file> {module_id}` to list port IDs."),
    )
}

/// Parses and resolves `module:port`.
pub fn resolve_endpoint(doc: &Document, spec: &str) -> Result<Endpoint> {
    let invalid = || MdgError::argument(format!("Invalid endpoint \"{spec}\". Use <module-id>:<port-id>."));
    let index = spec.find(':').ok_or_else(invalid)?;
    if index == 0 || index == spec.len() - 1 {
        return Err(invalid());
    }
    let module_id = resolve_module_id(doc, &spec[..index])?;
    let port_id = resolve_port_id(doc, &module_id, &spec[index + 1..])?;
    Ok(Endpoint { module_id, port_id })
}
