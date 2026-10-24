//! Purpose:
//! SimpleXML tree writes and removal with document-owned detached nodes.
//!
//! Called from:
//! - SimpleXML C ABI mutation entry points.
//!
//! Key details:
//! - Assignments escape text; addChild preserves libxml2 entity semantics.

use super::*;

impl View {
    /// Creates a missing named child and its missing ancestors for a chained write.
    fn ensure_first(&self) -> Option<Node> {
        if let Some(node) = self.first() { return Some(node); }
        match &self.scope {
            Scope::Children(parent, Some(name)) => {
                let node = parent.ensure_first()?;
                parent.with_scope(Scope::Node(node)).add(name, None, None, false)
            },
            Scope::Index(parent, 0) => parent.ensure_first(),
            _ => None,
        }
    }
    /// Adds an element or attribute under the first selected element.
    pub(super) fn add(&self, name: &str, value: Option<&str>, namespace: Option<&str>, attribute: bool) -> Option<Node> {
        let parent = self.first()?;
        if unsafe { ffi::node_type(parent) } != 1 || name.is_empty() { return None; }
        if attribute {
            let uri = namespace.unwrap_or("");
            if !uri.is_empty() && !name.contains(':') { return None; }
            let local = name.split_once(':').map(|(_, local)| local).unwrap_or(name);
            let mut current = unsafe { ffi::attrs(parent) };
            while !current.is_null() {
                if unsafe { string(ffi::name(current)) == local && string(ffi::ns(current, 0)) == uri } {
                    return None;
                }
                current = unsafe { ffi::next(current) };
            }
        }
        let name = cstring(name);
        if unsafe { crate::ffi::xmlValidateName(name.as_ptr().cast(), 0) } != 0 { return None; }
        let text = cstring(value.unwrap_or(""));
        let uri = cstring(namespace.unwrap_or(""));
        let raw = unsafe { ffi::add(parent, name.as_ptr(), text.as_ptr(), uri.as_ptr(),
            i32::from(namespace.is_some()), i32::from(attribute), i32::from(value.is_some())) };
        (!raw.is_null()).then_some(raw)
    }
    /// Replaces direct content without freeing child wrappers that remain in user storage.
    pub(super) fn assign_node(&self, node: Node, value: &str) {
        unsafe {
            if !matches!(ffi::node_type(node), 1 | 2) {
                ffi::content(node, cstring(value).as_ptr());
                return;
            }
            let mut cursor = ffi::child(node);
            while !cursor.is_null() {
                let next = ffi::next(cursor);
                self.detach(cursor);
                cursor = next;
            }
            let text = cstring(value);
            ffi::assign(node, text.as_ptr());
        }
    }
    /// Writes a named property/attribute or numeric offset, returning a PHP status code.
    pub(super) fn write(&self, kind: i64, key: &str, index: i64, value: &str) -> i64 {
        if kind == 4 && index < 0 { return 0; }
        if kind == 5 {
            if let Scope::Children(parent, Some(name)) = &self.scope {
                return parent.add(name, None, None, false).map(|node| { self.assign_node(node, value); 1 }).unwrap_or(0);
            }
            return 0;
        }
        let target = self.select(kind, key, "", false, index.max(0) as usize);
        let nodes = target.nodes();
        if nodes.len() > 1 { return -2; }
        if let Some(node) = nodes.first() { self.assign_node(*node, value); return 1; }
        if kind == 3 || (kind == 2 && matches!(self.scope, Scope::Attributes(..))) {
            let owner = if matches!(self.scope, Scope::Attributes(..)) { parent_for_attributes(self) } else { Rc::new(self.clone()) };
            return i64::from(owner.ensure_first().and_then(|node| {
                owner.with_scope(Scope::Node(node)).add(key, Some(value), None, true)
            }).is_some());
        }
        if kind == 2 {
            if let Some(node) = target.ensure_first() { target.assign_node(node, value); return 1; }
        }
        if kind == 4 || kind == 5 {
            if let Scope::Children(parent, Some(name)) = &self.scope {
                if let Some(node) = parent.add(name, None, None, false) { self.assign_node(node, value); return 1; }
            }
        }
        0
    }
    /// Removes selected nodes while preserving all borrowed wrapper storage.
    pub(super) fn remove(&self, kind: i64, key: &str, index: i64) {
        if kind == 4 && index < 0 { return; }
        for node in self.select(kind, key, "", false, index.max(0) as usize).nodes() { self.detach(node); }
    }
}
