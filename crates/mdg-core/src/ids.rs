use crate::error::{MdgError, Result};

/// Allowed ID syntax for modules, ports and connections, as a regular
/// expression (used in messages).
///
/// IDs start with an ASCII letter or digit and may contain letters, digits,
/// `_`, `.` and `-` (max 128 chars). `:` and `/` are excluded so that
/// `module:port` CLI notation and `port/<module>/<port>` content IDs are
/// unambiguous.
pub const ID_PATTERN: &str = "^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$";

pub fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 || !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    bytes[1..].iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
}

pub fn assert_valid_id(id: &str, what: &str) -> Result<()> {
    if is_valid_id(id) {
        Ok(())
    } else {
        Err(MdgError::argument(format!("Invalid {what} ID \"{id}\". IDs must match {ID_PATTERN}.")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    Module,
    Port,
    Connection,
}

impl IdKind {
    fn prefix(self) -> &'static str {
        match self {
            IdKind::Module => "mod",
            IdKind::Port => "port",
            IdKind::Connection => "conn",
        }
    }
}

/// Produces a new, collision-resistant ID for the given kind of element.
pub trait IdGenerator {
    fn generate(&mut self, kind: IdKind) -> String;
}

/// Any `FnMut(IdKind) -> String` closure is a generator.
impl<F: FnMut(IdKind) -> String> IdGenerator for F {
    fn generate(&mut self, kind: IdKind) -> String {
        self(kind)
    }
}

/// Generates an opaque UUID-based ID, e.g. `mod-3f1c...`.
pub fn generate_id(kind: IdKind) -> String {
    format!("{}-{}", kind.prefix(), uuid::Uuid::new_v4())
}

/// The default generator: UUID-based IDs.
#[derive(Debug, Default, Clone, Copy)]
pub struct UuidIdGenerator;

impl IdGenerator for UuidIdGenerator {
    fn generate(&mut self, kind: IdKind) -> String {
        generate_id(kind)
    }
}

/// Deterministic generator for tests and reproducible scripts.
#[derive(Debug, Default, Clone)]
pub struct SequentialIdGenerator {
    prefix: String,
    module: u64,
    port: u64,
    connection: u64,
}

impl SequentialIdGenerator {
    pub fn new(prefix: impl Into<String>) -> Self {
        SequentialIdGenerator { prefix: prefix.into(), ..Default::default() }
    }
}

impl IdGenerator for SequentialIdGenerator {
    fn generate(&mut self, kind: IdKind) -> String {
        let counter = match kind {
            IdKind::Module => &mut self.module,
            IdKind::Port => &mut self.port,
            IdKind::Connection => &mut self.connection,
        };
        *counter += 1;
        format!("{}{}-{}", self.prefix, kind.prefix(), counter)
    }
}

/// Runs `f` with the given generator, or a UUID generator when none is given.
pub(crate) fn with_generator<T>(
    generator: Option<&mut dyn IdGenerator>,
    f: impl FnOnce(&mut dyn IdGenerator) -> T,
) -> T {
    match generator {
        Some(g) => f(g),
        None => f(&mut UuidIdGenerator),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_syntax() {
        assert!(is_valid_id("a"));
        assert!(is_valid_id("api-service.v2_x"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("-a"));
        assert!(!is_valid_id("a:b"));
        assert!(!is_valid_id("a/b"));
        assert!(!is_valid_id("__proto__"));
        assert!(is_valid_id(&"a".repeat(128)));
        assert!(!is_valid_id(&"a".repeat(129)));
        assert!(!is_valid_id("é"));
    }

    #[test]
    fn generated_ids() {
        let id = generate_id(IdKind::Module);
        assert!(id.starts_with("mod-") && is_valid_id(&id));
        let mut seq = SequentialIdGenerator::new("t-");
        assert_eq!(seq.generate(IdKind::Port), "t-port-1");
        assert_eq!(seq.generate(IdKind::Port), "t-port-2");
        assert_eq!(seq.generate(IdKind::Connection), "t-conn-1");
    }
}
