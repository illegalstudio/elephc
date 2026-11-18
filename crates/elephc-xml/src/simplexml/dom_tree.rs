//! Purpose:
//! DOM tree surgery on the shared libxml2 tree: moving a node under a parent before a reference
//! node (`insertBefore` / `appendChild`) and removing one (`removeChild`).
//!
//! Called from:
//! - The DOM prelude's `DOMNode` tree members, through `elephc_xml_sx_dom_tree`.
//!
//! Key details:
//! - Only libxml2's public API is used. `xmlAddPrevSibling` / `xmlAddNextSibling` never merge
//!   adjacent text nodes (only `xmlAddChild` / `xmlAddSibling` do), so the node a PHP object is
//!   bound to keeps its identity and a text node can sit next to another one, as in PHP.
//! - A node that was removed earlier is owned by its retired subtree; linking it back transfers
//!   the ownership to the document again (`Document::unretire`).

use super::*;

extern "C" {
    #[link_name = "xmlAddPrevSibling"]
    fn add_prev_sibling(next: Node, cur: Node) -> Node;
    #[link_name = "xmlAddNextSibling"]
    fn add_next_sibling(prev: Node, cur: Node) -> Node;
    #[link_name = "xmlAddChild"]
    fn add_child(parent: Node, cur: Node) -> Node;
    #[link_name = "xmlGetLastChild"]
    fn last_child(parent: Node) -> Node;
    #[link_name = "xmlGetLineNo"]
    fn line_no(node: Node) -> std::ffi::c_long;
    #[link_name = "xmlSearchNs"]
    fn search_ns(doc: Node, node: Node, prefix: *const c_char) -> Node;
    #[link_name = "xmlSearchNsByHref"]
    fn search_ns_by_href(doc: Node, node: Node, href: *const c_char) -> Node;
}

impl Document {
    /// Hands a retired tree rooted at `node` back to the document that is about to relink it.
    fn unretire(self: &Rc<Self>, node: Node) {
        let Some(owner) = self.detached.borrow_mut().remove(&node).and_then(|weak| weak.upgrade()) else { return; };
        owner.adopted.set(true);
        for pin in self.pins.borrow().values().filter_map(Weak::upgrade) {
            let mut retired = pin.retired.borrow_mut();
            if retired.as_ref().is_some_and(|held| Rc::ptr_eq(held, &owner)) { *retired = None; }
        }
    }
}

/// Moves (`operation` 1) or removes (2) the node whose identity is `node`. The wrapper `handle`
/// supplies the document and, unless `document_parent` is set, the parent element of a move;
/// `document_parent` makes the document node itself the parent. `reference` is the sibling a
/// moved node goes before (0: last child). Returns 1 on success, 0 on refusal.
#[no_mangle]
pub extern "C" fn elephc_xml_sx_dom_tree(handle: i64, operation: i64, node: i64, reference: i64, document_parent: i64) -> i64 {
    super::abi::guard(|| {
        let Some(view) = super::abi::view(handle) else { return 0; };
        let node = node as usize as Node;
        if node.is_null() { return 0; }
        match operation {
            1 => {
                let parent = if document_parent != 0 { view.doc.raw } else { view.first().unwrap_or(std::ptr::null_mut()) };
                let reference = reference as usize as Node;
                if parent.is_null() { return 0; }
                let mut walk = parent;
                while !walk.is_null() {
                    if walk == node { return 0; }
                    walk = unsafe { ffi::parent(walk) };
                }
                let kind = unsafe { ffi::node_type(node) };
                if matches!(kind, 2 | 9 | 11 | 13 | 18) { return 0; }
                view.doc.unretire(node);
                let linked = unsafe {
                    if !reference.is_null() {
                        add_prev_sibling(reference, node)
                    } else {
                        let last = last_child(parent);
                        if last.is_null() { add_child(parent, node) }
                        else if last == node { node }
                        else { add_next_sibling(last, node) }
                    }
                };
                i64::from(!linked.is_null())
            }
            2 => { view.detach(node); 1 }
            3 => unsafe { line_no(node) as i64 },
            _ => 0,
        }
    })
}


/// Reads DOM namespace facts of the node a wrapper selects, as a C string ("" when absent).
/// Operations: 0 namespace URI of the node, 1 its prefix, 2 `lookupNamespaceURI(key)`,
/// 3 `lookupNamespaceURI(null)` (the default namespace), 4 `lookupPrefix(key)`. The lookups walk
/// the ancestors like libxml2's `xmlSearchNs`; a document node searches from its root element.
#[no_mangle]
pub unsafe extern "C" fn elephc_xml_sx_dom_ns(handle: i64, operation: i64, key: *const c_char) -> *const c_char {
    super::abi::guard(|| super::abi::output(super::abi::view(handle).and_then(|view| {
        let mut node = view.first()?;
        if ffi::node_type(node) == 9 || ffi::node_type(node) == 13 { node = ffi::root(view.doc.raw); }
        if node.is_null() { return None; }
        let key = cstring(bytes(key));
        let found = match operation {
            0 => return Some(bytes(ffi::ns(node, 0))),
            1 => return Some(bytes(ffi::ns(node, 1))),
            2 => if key.as_bytes().is_empty() { std::ptr::null_mut() } else { search_ns(view.doc.raw, node, key.as_ptr()) },
            3 => search_ns(view.doc.raw, node, std::ptr::null()),
            4 => search_ns_by_href(view.doc.raw, node, key.as_ptr()),
            _ => return None,
        };
        if found.is_null() { return None; }
        Some(bytes(ffi::ns_value(found, if operation == 4 { 1 } else { 0 })))
    }).unwrap_or_default()))
}
