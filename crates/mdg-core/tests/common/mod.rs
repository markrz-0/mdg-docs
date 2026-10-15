#![allow(dead_code)]

use std::sync::Arc;

use mdg_core::*;

pub fn ep(spec: &str) -> Endpoint {
    let (m, p) = spec.split_once(':').expect("module:port");
    Endpoint::new(m, p)
}

/// Small fluent builder with explicit IDs for readable tests.
pub struct Builder {
    pub doc: Document,
}

impl Builder {
    pub fn new() -> Self {
        Self::with_root("root")
    }

    pub fn with_root(root_id: &str) -> Self {
        let doc = create_document(CreateDocumentOptions {
            title: Some("Test".into()),
            root_id: Some(root_id.into()),
            ..Default::default()
        })
        .unwrap();
        Builder { doc }
    }

    pub fn module(self, id: &str) -> Self {
        self.module_in(id, "root")
    }

    pub fn module_in(self, id: &str, parent: &str) -> Self {
        self.named_module(id, parent, id)
    }

    pub fn named_module(mut self, id: &str, parent: &str, name: &str) -> Self {
        self.doc = add_module(
            &self.doc,
            AddModuleOptions { id: Some(id.into()), parent_id: parent.into(), name: name.into(), ..Default::default() },
        )
        .unwrap()
        .document;
        self
    }

    pub fn input(self, module_id: &str, id: &str) -> Self {
        self.named_port(PortDirection::Input, module_id, id, id)
    }

    pub fn output(self, module_id: &str, id: &str) -> Self {
        self.named_port(PortDirection::Output, module_id, id, id)
    }

    pub fn named_input(self, module_id: &str, id: &str, name: &str) -> Self {
        self.named_port(PortDirection::Input, module_id, id, name)
    }

    pub fn named_output(self, module_id: &str, id: &str, name: &str) -> Self {
        self.named_port(PortDirection::Output, module_id, id, name)
    }

    fn named_port(mut self, direction: PortDirection, module_id: &str, id: &str, name: &str) -> Self {
        self.doc = add_port(
            &self.doc,
            module_id,
            direction,
            AddPortOptions { id: Some(id.into()), name: name.into(), ..Default::default() },
        )
        .unwrap()
        .document;
        self
    }

    /// A request port the module handles, with "<id> request/response" contracts.
    pub fn handles(self, module_id: &str, id: &str) -> Self {
        self.request_port(PortDirection::Input, module_id, id)
    }

    /// A request port the module sends, with "<id> request/response" contracts.
    pub fn calls(self, module_id: &str, id: &str) -> Self {
        self.request_port(PortDirection::Output, module_id, id)
    }

    fn request_port(mut self, direction: PortDirection, module_id: &str, id: &str) -> Self {
        self.doc = add_port(
            &self.doc,
            module_id,
            direction,
            AddPortOptions {
                id: Some(id.into()),
                name: id.into(),
                kind: PortKind::Request,
                description: Some(format!("{id} request")),
                response: Some(format!("{id} response")),
                ..Default::default()
            },
        )
        .unwrap()
        .document;
        self
    }

    pub fn connect(mut self, id: &str, from: &str, to: &str) -> Self {
        self.doc = add_connection(
            &self.doc,
            AddConnectionOptions { id: Some(id.into()), from: ep(from), to: ep(to), ..Default::default() },
        )
        .unwrap()
        .document;
        self
    }
}

/// Returns `doc` with the module replaced by `f(module)`, bypassing validation.
pub fn edit_module(doc: &Document, id: &str, f: impl FnOnce(&mut Module)) -> Document {
    let mut next = doc.clone();
    let mut module = (**next.modules.get(id).unwrap()).clone();
    f(&mut module);
    next.modules.insert(id, Arc::new(module));
    next
}

pub fn module<'a>(doc: &'a Document, id: &str) -> &'a Module {
    doc.modules.get(id).unwrap_or_else(|| panic!("module {id} missing"))
}

pub fn codes(issues: &[ValidationIssue]) -> Vec<&'static str> {
    issues.iter().map(|i| i.code.as_str()).collect()
}

pub fn error_codes(doc: &Document) -> Vec<&'static str> {
    codes(&validate_document(doc, ValidationOptions::default()).errors)
}
