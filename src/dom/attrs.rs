//! An element's attribute list.
//!
//! DOM §4.9 calls it a LIST, not a map, and the order is observable in three
//! places at once: `getAttributeNames()` returns the qualified names "in
//! order", `element.attributes.item(i)` indexes that same order, and the HTML
//! fragment serializing algorithm (HTML §13.3) writes attributes in it. A
//! `HashMap` answers all three differently on every run.
//!
//! The order is: **first set wins the position, last set wins the value.**
//! Verified against Chrome — `<div id=d zebra=1 alpha=2>` then
//! `setAttribute("aaa", …)` gives `id,zebra,alpha,aaa`, and a later
//! `setAttribute("zebra", …)` leaves zebra at index 1 rather than moving it to
//! the end. That is the one place this differs from `css::Declarations`, where
//! a redeclaration DOES move: a CSS declaration's position is its cascade
//! order, an attribute's position is its identity.
//!
//! Elements carry a handful of attributes, so a linear scan over a `Vec` beats
//! hashing: no allocation per key, and the whole list is one cache line's worth
//! of pointers.

/// An ordered attribute list, with the `HashMap` surface the crate already
/// uses so existing call sites read unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttrMap {
    entries: Vec<(String, String)>,
}

impl AttrMap {
    pub fn new() -> Self {
        AttrMap {
            entries: Vec::new(),
        }
    }

    #[inline]
    fn position(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|(k, _)| k == key)
    }

    pub fn get(&self, key: impl AsRef<str>) -> Option<&String> {
        self.position(key.as_ref()).map(|i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: impl AsRef<str>) -> Option<&mut String> {
        match self.position(key.as_ref()) {
            Some(i) => Some(&mut self.entries[i].1),
            None => None,
        }
    }

    pub fn contains_key(&self, key: impl AsRef<str>) -> bool {
        self.position(key.as_ref()).is_some()
    }

    /// Set an attribute. A name already present keeps its POSITION and takes
    /// the new value; a new name is appended. Returns the previous value.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<String> {
        let key = key.into();
        let value = value.into();
        match self.position(&key) {
            Some(i) => Some(std::mem::replace(&mut self.entries[i].1, value)),
            None => {
                self.entries.push((key, value));
                None
            }
        }
    }

    pub fn remove(&mut self, key: impl AsRef<str>) -> Option<String> {
        self.position(key.as_ref())
            .map(|i| self.entries.remove(i).1)
    }

    /// The `entry(k).or_default()` shape, which several call sites use to
    /// append to an accumulating attribute (`class`, `style`) in place.
    pub fn entry_or_default(&mut self, key: impl Into<String>) -> &mut String {
        let key = key.into();
        let i = match self.position(&key) {
            Some(i) => i,
            None => {
                self.entries.push((key, String::new()));
                self.entries.len() - 1
            }
        };
        &mut self.entries[i].1
    }

    /// The tokenizer's duplicate-attribute rule: FIRST occurrence wins, so a
    /// name already present keeps BOTH its position and its value.
    pub fn or_insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut String {
        let key = key.into();
        let i = match self.position(&key) {
            Some(i) => i,
            None => {
                self.entries.push((key, value.into()));
                self.entries.len() - 1
            }
        };
        &mut self.entries[i].1
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    /// Positional access — what `NamedNodeMap.item(index)` is defined on.
    pub fn nth(&self, index: usize) -> Option<(&String, &String)> {
        self.entries.get(index).map(|(k, v)| (k, v))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn clear(&mut self) {
        self.entries.clear()
    }

    pub fn retain(&mut self, mut f: impl FnMut(&str, &str) -> bool) {
        self.entries.retain(|(k, v)| f(k, v));
    }
}

impl<'a> IntoIterator for &'a AttrMap {
    type Item = (&'a String, &'a String);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (String, String)>,
        fn(&'a (String, String)) -> (&'a String, &'a String),
    >;
    fn into_iter(self) -> Self::IntoIter {
        fn split<'b>(p: &'b (String, String)) -> (&'b String, &'b String) {
            (&p.0, &p.1)
        }
        self.entries
            .iter()
            .map(split as fn(&'a (String, String)) -> (&'a String, &'a String))
    }
}

impl FromIterator<(String, String)> for AttrMap {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(iter: T) -> Self {
        let mut m = AttrMap::new();
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}

impl<const N: usize> From<[(String, String); N]> for AttrMap {
    fn from(pairs: [(String, String); N]) -> Self {
        pairs.into_iter().collect()
    }
}

impl From<std::collections::HashMap<String, String>> for AttrMap {
    fn from(m: std::collections::HashMap<String, String>) -> Self {
        // Only reachable from callers that never had an order to lose.
        let mut v: Vec<(String, String)> = m.into_iter().collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        AttrMap { entries: v }
    }
}

// ─── Attr ───────────────────────────────────────────────────────────────────

/// One attribute, as DOM §4.9.2 defines it.
///
/// The spec makes `Attr` a `Node`, and in a browser it is addressable in the
/// node tree. Here it is a value: attributes live in the owning element's
/// `AttrMap`, not in the arena, so an `Attr` carries a snapshot plus the
/// identity of the element it came from. Everything the interface actually
/// answers — `name`, `value`, `localName`, `prefix`, `namespaceURI`,
/// `ownerElement`, `nodeType`, `nodeName`, `specified` — is answerable from
/// that, and `setAttributeNode` puts one back.
///
/// The one thing this cannot do is give an `Attr` its own `NodeId` so it could
/// be passed to `appendChild`. Nothing in the DOM asks for that: an `Attr`'s
/// parent is always null and its children are always empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// The element this attribute is on, or `0` once it has been removed —
    /// `ownerElement` answers null for a detached attribute, which Chrome
    /// demonstrates on the value returned by `removeAttributeNode`.
    pub owner_element: u32,
    /// The namespace URI, or `None` for the null namespace. HTML attributes
    /// are always in the null namespace.
    pub namespace: Option<String>,
    /// The QUALIFIED name — `xlink:href`, not `href`. This is what `name` and
    /// `nodeName` answer.
    pub name: String,
    pub value: String,
}

impl Attr {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Attr {
            owner_element: 0,
            namespace: None,
            name: name.into(),
            value: value.into(),
        }
    }

    /// `Node.nodeType` — `ATTRIBUTE_NODE`.
    pub fn node_type(&self) -> u16 {
        2
    }

    /// `Node.nodeName` is the qualified name, same as `name`.
    pub fn node_name(&self) -> &str {
        &self.name
    }

    /// `Node.nodeValue` is the value, same as `value`.
    pub fn node_value(&self) -> &str {
        &self.value
    }

    /// The part before the colon, or `None` when there is no colon.
    pub fn prefix(&self) -> Option<&str> {
        self.name.split_once(':').map(|(p, _)| p)
    }

    /// The part after the colon, or the whole name when there is no colon.
    pub fn local_name(&self) -> &str {
        self.name
            .split_once(':')
            .map(|(_, l)| l)
            .unwrap_or(&self.name)
    }

    pub fn namespace_uri(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    pub fn owner_element(&self) -> Option<u32> {
        if self.owner_element == 0 {
            None
        } else {
            Some(self.owner_element)
        }
    }

    /// Always true. The spec keeps it "for historical reasons" and requires
    /// exactly this answer.
    pub fn specified(&self) -> bool {
        true
    }
}
