//! Purpose:
//! php-verified coverage for DOM package P4: node tree mutation (insertBefore, appendChild,
//! removeChild, replaceChild, normalize, cloneNode), node kinds and namespaces.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/oop.rs`.
//!
//! Key details:
//! - Each program in `dom_p4/` ran under `php -n` (8.5); its stdout is the `.out` file next to it
//!   (t1 and t2 build their fragments with `new DOMDocumentFragment()`, the php run used
//!   `$d->createDocumentFragment()`, which package P5 adds). The elephc output must equal it byte
//!   for byte, with a clean heap-debug leak summary where the program frees everything it makes.
//! - `loadXML` needs the managed libxml2 native package, hence the skip gate.

use crate::support::*;

/// Runs `source` and checks stdout against the php-recorded `expected` and the leak summary.
fn check(name: &str, source: &str, expected: &str, leak_clean: bool) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{name}: stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{name}");
    if leak_clean {
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{name}: {}", out.stderr);
    }
}

/// A free-standing tree: insertBefore / replaceChild / removeChild / appendChild order, siblings,
/// the hierarchy / wrong-document / not-found errors, fragments, normalize, cloneNode, isEqualNode,
/// getNodePath and the kind and namespace properties.
#[test]
fn test_dom_p4_free_standing_tree_mutation() {
    check("t1", include_str!("dom_p4/t1.php"), include_str!("dom_p4/t1.out"), true);
}

/// The same operations on a parsed (libxml2-bound) document: nodes keep their identity when moved,
/// removed nodes can be inserted again, splitText links after the node, normalize merges, and the
/// namespace lookups read the native tree.
#[test]
fn test_dom_p4_bound_tree_mutation_and_namespaces() {
    if skip_without_xml_native("test_dom_p4_bound_tree_mutation_and_namespaces") { return; }
    check("t2", include_str!("dom_p4/t2.php"), include_str!("dom_p4/t2.out"), false);
}

/// A free-standing parent/child tree is freed with its last reference (parent and sibling links
/// are weak), splitText inserts the tail right after the node, and a child outliving its parent
/// has none.
#[test]
fn test_dom_p4_free_standing_tree_does_not_leak() {
    check("t3", include_str!("dom_p4/t3.php"), include_str!("dom_p4/t3.out"), true);
}
