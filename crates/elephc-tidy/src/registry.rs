//! Purpose:
//! Process-global document and node registries for the `elephc-tidy` bridge.
//!
//! Called from:
//! - `crate::document`, `crate::option`, and `crate::node` entry points.
//!
//! Key details:
//! - Handles are positive integers minted once per process; `0` is never valid.
//! - A document owns one libtidy `TidyDoc` and one growable `TidyBuffer` error
//!   sink. The buffer lives in a `Box` so `tidySetErrorBuffer` keeps a stable
//!   address across registry rehashes.
//! - Every live node handle holds a reference on its document, exactly like
//!   php-src's `PHPTidyDoc::ref_count`. Releasing the `tidy` object therefore
//!   does not free the document while a `tidyNode` still points into it.
//! - One `Mutex` covers both maps, so node creation can atomically look up its
//!   document and bump the reference count.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::ffi::{
    tidyBufFree, tidyBufInit, tidyCreate, tidyGetNextOption, tidyGetOptionList, tidyOptSetBool,
    tidyRelease, tidySetErrorBuffer, TidyBuffer, TidyDoc, TidyNode, OPTION_FORCE_OUTPUT,
    OPTION_MARK,
};

/// One live document and its PHP-visible bookkeeping.
pub(crate) struct DocEntry {
    /// The libtidy document pointer.
    pub(crate) doc: usize,
    /// The growable error sink installed with `tidySetErrorBuffer`.
    pub(crate) errbuf: Box<TidyBuffer>,
    /// Whether a parse has been attempted; php-src sets this before parsing.
    pub(crate) initialized: bool,
    /// Number of PHP wrappers (`tidy` object plus node handles) holding the document.
    pub(crate) refs: usize,
    /// Cached option pointers in libtidy's iterator order.
    pub(crate) options: Vec<usize>,
    /// Whether `options` has been populated.
    pub(crate) options_loaded: bool,
}

/// One live node handle and the document that owns the node.
#[derive(Clone, Copy)]
pub(crate) struct NodeEntry {
    /// The document handle this node belongs to.
    pub(crate) doc: i64,
    /// The libtidy node pointer.
    pub(crate) node: usize,
}

/// The process-wide handle tables.
pub(crate) struct Registry {
    /// Documents keyed by handle.
    pub(crate) docs: HashMap<i64, DocEntry>,
    /// Nodes keyed by handle.
    pub(crate) nodes: HashMap<i64, NodeEntry>,
}

/// Registry singleton, created on first use.
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
/// Next handle; never reused within a process.
static NEXT_HANDLE: Mutex<i64> = Mutex::new(1);

/// Returns the registry, creating it on first use.
pub(crate) fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            docs: HashMap::new(),
            nodes: HashMap::new(),
        })
    })
}

/// Locks the registry, recovering from a poisoned mutex.
pub(crate) fn lock() -> MutexGuard<'static, Registry> {
    registry().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Mints a fresh positive handle.
fn next_handle() -> i64 {
    let mut next = NEXT_HANDLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let handle = *next;
    *next += 1;
    handle
}

/// Creates a document with php-src's forced-output/no-tidy-mark defaults.
pub(crate) fn create_document() -> i64 {
    // Serialize every libtidy entry point on the registry lock: the library keeps
    // process-global state and the PHP extension is not built for concurrent ZTS calls.
    let mut registry = lock();
    let doc = unsafe { tidyCreate() };
    if doc.is_null() {
        return 0;
    }
    let mut errbuf = Box::new(TidyBuffer::zeroed());
    unsafe {
        tidyBufInit(&mut *errbuf);
        if tidySetErrorBuffer(doc, &mut *errbuf) != 0 {
            tidyBufFree(&mut *errbuf);
            tidyRelease(doc);
            return 0;
        }
        tidyOptSetBool(doc, OPTION_FORCE_OUTPUT, 1);
        tidyOptSetBool(doc, OPTION_MARK, 0);
    }
    let handle = next_handle();
    registry.docs.insert(
        handle,
        DocEntry {
            doc: doc as usize,
            errbuf,
            initialized: false,
            refs: 1,
            options: Vec::new(),
            options_loaded: false,
        },
    );
    handle
}

/// Drops one document reference and frees the document at zero.
pub(crate) fn release_document(handle: i64) -> i64 {
    let mut registry = lock();
    release_doc_locked(&mut registry, handle);
    1
}

/// Drops one reference while the registry lock is already held.
pub(crate) fn release_doc_locked(registry: &mut Registry, handle: i64) {
    let Some(entry) = registry.docs.get_mut(&handle) else {
        return;
    };
    if entry.refs > 1 {
        entry.refs -= 1;
        return;
    }
    let Some(entry) = registry.docs.remove(&handle) else {
        return;
    };
    let mut errbuf = entry.errbuf;
    unsafe {
        tidyBufFree(&mut *errbuf);
        tidyRelease(entry.doc as *mut TidyDoc);
    }
}

/// Drops every node handle owned by one document before libtidy replaces its tree.
///
/// `tidyParseBuffer` releases the document's current node tree. Node handles minted
/// against that tree would otherwise hold stale `TidyNode` pointers and a document
/// reference, so a later accessor could call into freed memory and a `tidyNode`
/// object could keep the document alive after it no longer owns a valid tree. The
/// document's own reference is left untouched, so the handle remains usable for the
/// replacement parse.
pub(crate) fn drop_nodes_for_document(registry: &mut Registry, doc_handle: i64) {
    let stale = registry
        .nodes
        .iter()
        .filter_map(|(handle, entry)| (entry.doc == doc_handle).then_some(*handle))
        .collect::<Vec<_>>();
    for handle in stale {
        if let Some(entry) = registry.nodes.remove(&handle) {
            release_doc_locked(registry, entry.doc);
        }
    }
}

/// Registers a node pointer under `doc_handle`, taking one document reference.
pub(crate) fn create_node(registry: &mut Registry, doc_handle: i64, node: *mut TidyNode) -> i64 {
    if node.is_null() {
        return 0;
    }
    let Some(doc) = registry.docs.get_mut(&doc_handle) else {
        return 0;
    };
    doc.refs += 1;
    let handle = next_handle();
    registry.nodes.insert(
        handle,
        NodeEntry {
            doc: doc_handle,
            node: node as usize,
        },
    );
    handle
}

/// Releases one node handle and its document reference.
pub(crate) fn release_node(handle: i64) -> i64 {
    let mut registry = lock();
    let Some(entry) = registry.nodes.remove(&handle) else {
        return 1;
    };
    release_doc_locked(&mut registry, entry.doc);
    1
}

/// Runs `body` with the document entry and its raw pointer.
pub(crate) fn with_doc<T>(
    handle: i64,
    fallback: T,
    body: impl FnOnce(&mut DocEntry, *mut TidyDoc) -> T,
) -> T {
    let mut registry = lock();
    match registry.docs.get_mut(&handle) {
        Some(entry) => {
            let doc = entry.doc as *mut TidyDoc;
            body(entry, doc)
        }
        None => fallback,
    }
}

/// Runs `body` with the registry, the document handle, and its raw pointer.
///
/// Used when the body needs to register a child node (which requires the whole
/// registry) without holding a second mutable borrow of the document entry.
pub(crate) fn with_doc_handle<T>(
    handle: i64,
    fallback: T,
    body: impl FnOnce(&mut Registry, i64, *mut TidyDoc) -> T,
) -> T {
    let mut registry = lock();
    let Some(entry) = registry.docs.get(&handle) else {
        return fallback;
    };
    let doc = entry.doc as *mut TidyDoc;
    body(&mut registry, handle, doc)
}

/// Runs `body` with the node entry, its document entry, and both raw pointers.
pub(crate) fn with_node<T>(
    handle: i64,
    fallback: T,
    body: impl FnOnce(&mut Registry, NodeEntry, *mut TidyDoc, *mut TidyNode) -> T,
) -> T {
    let mut registry = lock();
    let Some(entry) = registry.nodes.get(&handle).copied() else {
        return fallback;
    };
    let Some(doc_entry) = registry.docs.get(&entry.doc) else {
        return fallback;
    };
    let doc = doc_entry.doc as *mut TidyDoc;
    let node = entry.node as *mut TidyNode;
    body(&mut registry, entry, doc, node)
}

/// Returns a document's cached option pointers, loading them on first use.
pub(crate) fn options(entry: &mut DocEntry) -> &[usize] {
    if !entry.options_loaded {
        let doc = entry.doc as *mut TidyDoc;
        let mut iter = unsafe { tidyGetOptionList(doc) };
        while !iter.is_null() {
            let option = unsafe { tidyGetNextOption(doc, &mut iter) };
            if option.is_null() {
                break;
            }
            entry.options.push(option as usize);
        }
        entry.options_loaded = true;
    }
    &entry.options
}

#[cfg(all(test, elephc_tidy_native))]
mod tests {
    //! Purpose:
    //! Handle-table guards that do not need a linked libtidy.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Unknown handles must fail closed without touching libtidy.

    use super::*;

    /// Unknown document and node handles answer their fallback.
    #[test]
    fn unknown_handles_fail_closed() {
        assert_eq!(with_doc(999_999, -1, |_, _| 0), -1);
        assert_eq!(with_node(999_999, -1, |_, _, _, _| 0), -1);
        assert_eq!(release_node(999_999), 1);
    }

    /// Reparsing drops the previous tree's node handles without dropping the document.
    #[test]
    fn reparsing_invalidates_previous_nodes() {
        let mut registry = Registry {
            docs: HashMap::from([(
                7,
                DocEntry {
                    doc: 0,
                    errbuf: Box::new(TidyBuffer::zeroed()),
                    initialized: false,
                    refs: 2,
                    options: Vec::new(),
                    options_loaded: false,
                },
            )]),
            nodes: HashMap::from([(
                9,
                NodeEntry {
                    doc: 7,
                    node: 0,
                },
            )]),
        };

        drop_nodes_for_document(&mut registry, 7);

        assert!(!registry.nodes.contains_key(&9));
        assert_eq!(registry.docs.get(&7).map(|entry| entry.refs), Some(1));
    }
}
