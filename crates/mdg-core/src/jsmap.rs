//! An insertion-ordered string map that iterates like a JavaScript object.
//!
//! The `.mdg` format and the CLI output were defined by a JavaScript
//! implementation, whose objects enumerate "array index" keys (canonical
//! decimal integers below 2^32 − 1) first in ascending numeric order, then
//! all other keys in insertion order. Module IDs such as `10` or `2` are
//! valid, so reproducing this order keeps validation output, ID resolution
//! and serialized bytes identical.

use indexmap::IndexMap;
use serde::{Serialize, Serializer};

/// Returns the numeric value when `key` is a JavaScript array index.
pub fn array_index(key: &str) -> Option<u32> {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 10 || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    let value: u64 = key.parse().ok()?;
    if value < u64::from(u32::MAX) {
        Some(value as u32)
    } else {
        None
    }
}

/// Orders keys the way a JavaScript object enumerates them.
pub fn js_key_order<'a, I: IntoIterator<Item = &'a str>>(keys: I) -> Vec<&'a str> {
    let mut indices: Vec<(u32, &'a str)> = Vec::new();
    let mut others: Vec<&'a str> = Vec::new();
    for key in keys {
        match array_index(key) {
            Some(n) => indices.push((n, key)),
            None => others.push(key),
        }
    }
    if indices.is_empty() {
        return others;
    }
    indices.sort_by_key(|(n, _)| *n);
    indices.into_iter().map(|(_, k)| k).chain(others).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsMap<V> {
    inner: IndexMap<String, V>,
    index_keys: usize,
}

impl<V> Default for JsMap<V> {
    fn default() -> Self {
        JsMap { inner: IndexMap::new(), index_keys: 0 }
    }
}

impl<V> JsMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.inner.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.inner.get_mut(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    /// Inserts or replaces. A replaced key keeps its position, like
    /// assigning to an existing JavaScript property.
    pub fn insert(&mut self, key: impl Into<String>, value: V) -> Option<V> {
        let key = key.into();
        let is_index = array_index(&key).is_some();
        let old = self.inner.insert(key, value);
        if old.is_none() && is_index {
            self.index_keys += 1;
        }
        old
    }

    /// Removes a key, keeping the order of the others (like `delete`).
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let old = self.inner.shift_remove(key);
        if old.is_some() && array_index(key).is_some() {
            self.index_keys -= 1;
        }
        old
    }

    /// Keys in JavaScript enumeration order.
    pub fn keys(&self) -> Vec<&str> {
        if self.index_keys == 0 {
            self.inner.keys().map(String::as_str).collect()
        } else {
            js_key_order(self.inner.keys().map(String::as_str))
        }
    }

    /// Entries in JavaScript enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> + '_ {
        self.keys().into_iter().map(move |k| (k, &self.inner[k]))
    }

    /// Values in JavaScript enumeration order.
    pub fn values(&self) -> impl Iterator<Item = &V> + '_ {
        self.iter().map(|(_, v)| v)
    }
}

impl<V, K: Into<String>> FromIterator<(K, V)> for JsMap<V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut map = JsMap::new();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

impl<V: Serialize> Serialize for JsMap<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_index_keys() {
        assert_eq!(array_index("0"), Some(0));
        assert_eq!(array_index("10"), Some(10));
        assert_eq!(array_index("4294967294"), Some(4294967294));
        assert_eq!(array_index("4294967295"), None);
        assert_eq!(array_index("01"), None);
        assert_eq!(array_index("1a"), None);
        assert_eq!(array_index(""), None);
    }

    #[test]
    fn enumerates_like_javascript() {
        let mut m = JsMap::new();
        m.insert("b", 1);
        m.insert("10", 2);
        m.insert("a", 3);
        m.insert("2", 4);
        assert_eq!(m.keys(), vec!["2", "10", "b", "a"]);
        m.insert("b", 5);
        assert_eq!(m.keys(), vec!["2", "10", "b", "a"]);
        m.remove("b");
        m.insert("b", 6);
        assert_eq!(m.keys(), vec!["2", "10", "a", "b"]);
        m.remove("2");
        m.remove("10");
        assert_eq!(m.keys(), vec!["a", "b"]);
    }
}
