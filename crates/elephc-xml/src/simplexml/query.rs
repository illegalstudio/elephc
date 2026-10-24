//! Purpose:
//! SimpleXML XPath and declared/used namespace queries.
//!
//! Called from:
//! - The SimpleXML C ABI and PHP prelude.
//!
//! Key details:
//! - XPath nodes remain document-owned; temporary XPath objects are always released.

use super::*;

impl View {
    /// Evaluates XPath with in-scope prefixes followed by explicitly registered overrides.
    fn xpath(&self, expression: &str) -> Option<Vec<Node>> {
        let node = self.first()?;
        unsafe {
            let context = ffi::xpath_context(self.doc.raw, node);
            if context.is_null() { return None; }
            for (prefix, uri) in self.xpath_ns.borrow().iter() {
                ffi::xpath_register(context, cstring(prefix).as_ptr(), cstring(uri).as_ptr());
            }
            let result = ffi::xpath_eval(context, cstring(expression).as_ptr());
            let count = ffi::xpath_count(result);
            let mut nodes = Vec::new();
            for index in 0..count.max(0) {
                let raw = ffi::xpath_node(result, index);
                if raw.is_null() { continue; }
                match ffi::node_type(raw) {
                    1 | 2 => nodes.push(raw),
                    3 | 4 => { let parent = ffi::parent(raw); if !parent.is_null() { nodes.push(parent); } }
                    _ => (),
                }
            }
            ffi::xpath_free(result);
            ffi::xpath_context_free(context);
            (count >= 0).then_some(nodes)
        }
    }
    /// Enumerates namespace declarations or namespace uses in PHP traversal order.
    fn namespaces(&self, declared: bool, recursive: bool, from_root: bool) -> Vec<(String, String)> {
        let start = if from_root { Some(unsafe { ffi::root(self.doc.raw) }) } else { self.first() };
        let Some(start) = start else { return vec![]; };
        let mut entries = Vec::new();
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            unsafe {
                if ffi::node_type(node) != 1 { continue; }
                if declared {
                    let mut ns = ffi::ns_defs(node);
                    while !ns.is_null() {
                        insert_namespace(&mut entries, string(ffi::ns_value(ns, 1)), string(ffi::ns_value(ns, 0)));
                        ns = ffi::ns_next(ns);
                    }
                } else {
                    insert_namespace(&mut entries, string(ffi::ns(node, 1)), string(ffi::ns(node, 0)));
                    let mut attr = ffi::attrs(node);
                    while !attr.is_null() {
                        insert_namespace(&mut entries, string(ffi::ns(attr, 1)), string(ffi::ns(attr, 0)));
                        attr = ffi::next(attr);
                    }
                }
                if recursive {
                    let mut children = Vec::new();
                    let mut child = ffi::child(node);
                    while !child.is_null() { children.push(child); child = ffi::next(child); }
                    pending.extend(children.into_iter().rev());
                }
            }
        }
        entries
    }
}

/// Preserves the first namespace bound to a prefix, omitting the no-namespace marker.
fn insert_namespace(entries: &mut Vec<(String, String)>, prefix: String, uri: String) {
    if !uri.is_empty() && !entries.iter().any(|(name, _)| *name == prefix) { entries.push((prefix, uri)); }
}
