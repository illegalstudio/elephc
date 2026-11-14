//! Purpose:
//! C ABI entry points for `tidyNode`: document root accessors, node metadata,
//! tree navigation, children, attributes, and handle release.
//!
//! Called from:
//! - Compiled PHP programs through the `extern "elephc_tidy"` block declared by
//!   `elephc::tidy_prelude`.
//!
//! Key details:
//! - Every node handle owns one document reference; releasing it can free the
//!   document once the `tidy` object is gone.
//! - New node handles are minted for every accessor call, matching php-src's
//!   `tidy_create_node_object` behavior: `getParent()`, `child`, and siblings
//!   each return a distinct PHP wrapper around the same libtidy pointer.
//! - `tidyNodeGetText` output is copied before the temporary buffer is freed.

use std::ffi::{c_char, c_uint};

use crate::abi::{buffer_text, c_bytes, flag, guarded, stash, ATTR_NAME, ATTR_VALUE, NODE_NAME, NODE_TEXT};
use crate::ffi::{
    tidyAttrFirst, tidyAttrName, tidyAttrNext, tidyAttrValue, tidyBufFree, tidyBufInit,
    tidyGetBody, tidyGetChild, tidyGetHead, tidyGetHtml, tidyGetNext, tidyGetParent, tidyGetPrev,
    tidyGetRoot, tidyNodeColumn, tidyNodeGetId, tidyNodeGetName, tidyNodeGetText, tidyNodeGetType,
    tidyNodeIsProp, tidyNodeLine, TidyAttr, TidyBuffer, TidyDoc, TidyNode, NODE_TYPE_ASP,
    NODE_TYPE_COMMENT, NODE_TYPE_END, NODE_TYPE_JSTE, NODE_TYPE_PHP, NODE_TYPE_START,
    NODE_TYPE_START_END, NODE_TYPE_TEXT,
};
use crate::registry::{create_node, release_node, with_doc_handle, with_node};

/// Returns the node at `index` in `node`'s child list.
fn child_at(node: *mut TidyNode, index: i64) -> *mut TidyNode {
    if index < 0 {
        return std::ptr::null_mut();
    }
    let mut child = unsafe { tidyGetChild(node) };
    let mut position = 0i64;
    while !child.is_null() {
        if position == index {
            return child;
        }
        position += 1;
        child = unsafe { tidyGetNext(child) };
    }
    std::ptr::null_mut()
}

/// Counts a node's direct children.
fn child_count(node: *mut TidyNode) -> i64 {
    let mut child = unsafe { tidyGetChild(node) };
    let mut count = 0i64;
    while !child.is_null() {
        count += 1;
        child = unsafe { tidyGetNext(child) };
    }
    count
}

/// Returns the attribute at `index` in `node`'s attribute list.
fn attr_at(node: *mut TidyNode, index: i64) -> *mut TidyAttr {
    if index < 0 {
        return std::ptr::null_mut();
    }
    let mut attr = unsafe { tidyAttrFirst(node) };
    let mut position = 0i64;
    while !attr.is_null() {
        // php-src skips a malformed attribute whose name is NULL; exposing it here would
        // shift every following index and return an empty name for a real attribute slot.
        if !unsafe { tidyAttrName(attr) }.is_null() {
            if position == index {
                return attr;
            }
            position += 1;
        }
        attr = unsafe { tidyAttrNext(attr) };
    }
    std::ptr::null_mut()
}

/// Counts a node's named attributes, matching php-src's NULL-name skip.
fn attr_count(node: *mut TidyNode) -> i64 {
    let mut attr = unsafe { tidyAttrFirst(node) };
    let mut count = 0i64;
    while !attr.is_null() {
        if !unsafe { tidyAttrName(attr) }.is_null() {
            count += 1;
        }
        attr = unsafe { tidyAttrNext(attr) };
    }
    count
}

/// Mints a handle for one of a document's fixed root nodes.
fn document_node(
    handle: i64,
    getter: impl FnOnce(*mut TidyDoc) -> *mut TidyNode,
) -> i64 {
    guarded(0, || {
        with_doc_handle(handle, 0, |registry, doc_handle, doc| {
            create_node(registry, doc_handle, getter(doc))
        })
    })
}

/// Mints a handle for a node reachable from an existing handle.
fn related_node(
    handle: i64,
    getter: impl FnOnce(*mut TidyNode) -> *mut TidyNode,
) -> i64 {
    guarded(0, || {
        with_node(handle, 0, |registry, entry, _, node| {
            create_node(registry, entry.doc, getter(node))
        })
    })
}

/// Returns an i64 answer from one node accessor.
fn node_int(
    handle: i64,
    getter: impl FnOnce(*mut TidyDoc, *mut TidyNode) -> i64,
) -> i64 {
    guarded(0, || with_node(handle, 0, |_, _, doc, node| getter(doc, node)))
}

/// C ABI: returns the document root node handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_root(handle: i64) -> i64 {
    document_node(handle, |doc| unsafe { tidyGetRoot(doc) })
}

/// C ABI: returns the `html` element node handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_html(handle: i64) -> i64 {
    document_node(handle, |doc| unsafe { tidyGetHtml(doc) })
}

/// C ABI: returns the `head` element node handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_head(handle: i64) -> i64 {
    document_node(handle, |doc| unsafe { tidyGetHead(doc) })
}

/// C ABI: returns the `body` element node handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_body(handle: i64) -> i64 {
    document_node(handle, |doc| unsafe { tidyGetBody(doc) })
}

/// C ABI: releases one node handle.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_release(handle: i64) -> i64 {
    guarded(0, || release_node(handle))
}

/// C ABI: returns the node type (`TidyNodeType`).
#[no_mangle]
pub extern "C" fn elephc_tidy_node_type(handle: i64) -> i64 {
    node_int(handle, |_, node| unsafe { tidyNodeGetType(node) as i64 })
}

/// C ABI: returns the node's element name (empty for anonymous nodes).
#[no_mangle]
pub extern "C" fn elephc_tidy_node_name(handle: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_node(handle, Vec::new(), |_, _, _, node| unsafe {
            c_bytes(tidyNodeGetName(node))
        })
    });
    stash(&NODE_NAME, bytes)
}

/// C ABI: serializes the node subtree and returns its text.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_text(handle: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_node(handle, Vec::new(), |_, _, doc, node| {
            let mut buffer = TidyBuffer::zeroed();
            unsafe {
                tidyBufInit(&mut buffer);
                let ok = tidyNodeGetText(doc, node, &mut buffer) != 0;
                let text = if ok {
                    buffer_text(&buffer)
                } else {
                    Vec::new()
                };
                tidyBufFree(&mut buffer);
                text
            }
        })
    });
    stash(&NODE_TEXT, bytes)
}

/// C ABI: returns the node's source line.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_line(handle: i64) -> i64 {
    node_int(handle, |_, node| unsafe { tidyNodeLine(node) as c_uint as i64 })
}

/// C ABI: returns the node's source column.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_column(handle: i64) -> i64 {
    node_int(handle, |_, node| unsafe { tidyNodeColumn(node) as c_uint as i64 })
}

/// C ABI: returns whether the node is proprietary.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_proprietary(handle: i64) -> i64 {
    node_int(handle, |doc, node| {
        flag(unsafe { tidyNodeIsProp(doc, node) } != 0)
    })
}

/// C ABI: returns the node's tag id (`TidyTagId`).
#[no_mangle]
pub extern "C" fn elephc_tidy_node_id(handle: i64) -> i64 {
    node_int(handle, |_, node| unsafe { tidyNodeGetId(node) as i64 })
}

/// C ABI: returns whether the node has at least one child.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_has_children(handle: i64) -> i64 {
    node_int(handle, |_, node| flag(unsafe { !tidyGetChild(node).is_null() }))
}

/// C ABI: returns whether the node has a next sibling.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_has_siblings(handle: i64) -> i64 {
    node_int(handle, |_, node| flag(unsafe { !tidyGetNext(node).is_null() }))
}

/// C ABI: returns whether the node is a comment.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_is_comment(handle: i64) -> i64 {
    node_int(handle, |_, node| {
        flag(unsafe { tidyNodeGetType(node) } == NODE_TYPE_COMMENT)
    })
}

/// C ABI: returns whether the node is an HTML start, end, or empty tag.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_is_html(handle: i64) -> i64 {
    node_int(handle, |_, node| {
        flag(matches!(
            unsafe { tidyNodeGetType(node) },
            NODE_TYPE_START | NODE_TYPE_END | NODE_TYPE_START_END
        ))
    })
}

/// C ABI: returns whether the node is text.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_is_text(handle: i64) -> i64 {
    node_int(handle, |_, node| {
        flag(unsafe { tidyNodeGetType(node) } == NODE_TYPE_TEXT)
    })
}

/// C ABI: returns whether the node is JSTE.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_is_jste(handle: i64) -> i64 {
    node_int(handle, |_, node| {
        flag(unsafe { tidyNodeGetType(node) } == NODE_TYPE_JSTE)
    })
}

/// C ABI: returns whether the node is ASP.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_is_asp(handle: i64) -> i64 {
    node_int(handle, |_, node| {
        flag(unsafe { tidyNodeGetType(node) } == NODE_TYPE_ASP)
    })
}

/// C ABI: returns whether the node is PHP.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_is_php(handle: i64) -> i64 {
    node_int(handle, |_, node| {
        flag(unsafe { tidyNodeGetType(node) } == NODE_TYPE_PHP)
    })
}

/// C ABI: returns the parent node handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_parent(handle: i64) -> i64 {
    related_node(handle, |node| unsafe { tidyGetParent(node) })
}

/// C ABI: returns the previous sibling handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_previous(handle: i64) -> i64 {
    related_node(handle, |node| unsafe { tidyGetPrev(node) })
}

/// C ABI: returns the next sibling handle, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_next(handle: i64) -> i64 {
    related_node(handle, |node| unsafe { tidyGetNext(node) })
}

/// C ABI: returns the number of direct children.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_child_count(handle: i64) -> i64 {
    node_int(handle, |_, node| child_count(node))
}

/// C ABI: returns the child handle at `index`, or 0.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_child(handle: i64, index: i64) -> i64 {
    guarded(0, || {
        with_node(handle, 0, |registry, entry, _, node| {
            create_node(registry, entry.doc, child_at(node, index))
        })
    })
}

/// C ABI: returns the number of attributes.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_attr_count(handle: i64) -> i64 {
    node_int(handle, |_, node| attr_count(node))
}

/// C ABI: returns the attribute name at `index`, or an empty string.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_attr_name(handle: i64, index: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_node(handle, Vec::new(), |_, _, _, node| {
            let attr = attr_at(node, index);
            if attr.is_null() {
                Vec::new()
            } else {
                unsafe { c_bytes(tidyAttrName(attr)) }
            }
        })
    });
    stash(&ATTR_NAME, bytes)
}

/// C ABI: returns the attribute value at `index`, or an empty string.
#[no_mangle]
pub extern "C" fn elephc_tidy_node_attr_value(handle: i64, index: i64) -> *const c_char {
    let bytes = guarded(Vec::new(), || {
        with_node(handle, Vec::new(), |_, _, _, node| {
            let attr = attr_at(node, index);
            if attr.is_null() {
                Vec::new()
            } else {
                unsafe { c_bytes(tidyAttrValue(attr)) }
            }
        })
    });
    stash(&ATTR_VALUE, bytes)
}

#[cfg(all(test, elephc_tidy_native))]
mod tests {
    //! Purpose:
    //! Pure list-walk guards for the node C ABI.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Negative indexes and null nodes fail closed without calling libtidy.

    use super::*;

    /// Negative indexes never walk a tree.
    #[test]
    fn negative_indexes_fail_closed() {
        assert!(child_at(std::ptr::null_mut(), -1).is_null());
        assert!(attr_at(std::ptr::null_mut(), -1).is_null());
    }
}
