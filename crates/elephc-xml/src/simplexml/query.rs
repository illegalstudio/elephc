//! Purpose:
//! SimpleXML XPath and declared/used namespace queries.
//!
//! Called from:
//! - The SimpleXML C ABI and PHP prelude.
//!
//! Key details:
//! - XPath nodes remain document-owned; temporary XPath objects are always released.

use super::*;

/// Distinguishes invalid XPath from a scalar result that requires PHP's typed warning.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum XPathError { Invalid, Scalar(i32) }

impl View {
    /// Evaluates XPath with in-scope prefixes taking precedence over registered aliases.
    pub(super) fn xpath(&self, expression: &str) -> Result<Vec<Node>, XPathError> {
        diagnostics::reset();
        let node = self.first().ok_or(XPathError::Invalid)?;
        unsafe {
            let context = ffi::xpath_context(self.doc.raw, node);
            if context.is_null() { return Err(XPathError::Invalid); }
            for (prefix, uri) in self.xpath_ns.borrow().iter() {
                let prefix = cstring(prefix);
                if ffi::xpath_lookup(context, prefix.as_ptr()).is_null() {
                    ffi::xpath_register(context, prefix.as_ptr(), cstring(uri).as_ptr());
                }
            }
            let mut messages = std::ptr::null_mut();
            let result = ffi::xpath_eval(context, cstring(expression).as_ptr(), &mut messages);
            diagnostics::capture(messages);
            let count = ffi::xpath_count(result);
            let kind = ffi::xpath_type(result);
            let mut nodes = Vec::new();
            for index in 0..count.max(0) {
                let raw = ffi::xpath_node(result, index);
                if raw.is_null() { continue; }
                match ffi::node_type(raw) {
                    1 | 2 | 7 | 8 => nodes.push(raw),
                    3 => { let parent = ffi::parent(raw); if !parent.is_null() { nodes.push(parent); } }
                    _ => (),
                }
            }
            ffi::xpath_free(result);
            ffi::xpath_context_free(context);
            if count >= 0 { Ok(nodes) }
            else if kind == 0 { Err(XPathError::Invalid) }
            else { Err(XPathError::Scalar(kind)) }
        }
    }
    /// Enumerates namespace declarations or namespace uses in PHP traversal order.
    pub(super) fn namespaces(&self, declared: bool, recursive: bool, from_root: bool) -> Vec<(String, String)> {
        let start = if from_root { Some(unsafe { ffi::root(self.doc.raw) }) }
            else if declared { (!self.identity.is_null()).then_some(self.identity) }
            else { self.first() };
        let Some(start) = start else { return vec![]; };
        let mut entries = Vec::new();
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            unsafe {
                if !declared && ffi::node_type(node) == 2 {
                    insert_namespace(&mut entries, string(ffi::ns(node, 1)), string(ffi::ns(node, 0)), false);
                    continue;
                }
                if ffi::node_type(node) != 1 { continue; }
                if declared {
                    let mut ns = ffi::ns_defs(node);
                    while !ns.is_null() {
                        insert_namespace(&mut entries, string(ffi::ns_value(ns, 1)), string(ffi::ns_value(ns, 0)), true);
                        ns = ffi::ns_next(ns);
                    }
                } else {
                    insert_namespace(&mut entries, string(ffi::ns(node, 1)), string(ffi::ns(node, 0)), false);
                    let mut attr = ffi::attrs(node);
                    while !attr.is_null() {
                        insert_namespace(&mut entries, string(ffi::ns(attr, 1)), string(ffi::ns(attr, 0)), false);
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

/// Preserves the first binding, including empty declarations but omitting unused no-namespace markers.
fn insert_namespace(entries: &mut Vec<(String, String)>, prefix: String, uri: String, declared: bool) {
    if (declared || !uri.is_empty()) && !entries.iter().any(|(name, _)| *name == prefix) { entries.push((prefix, uri)); }
}
