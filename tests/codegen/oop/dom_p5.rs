//! Purpose:
//! php-verified coverage for DOM package P5: the DOMElement attribute API, the ParentNode /
//! ChildNode helpers, `getElementsByTagNameNS`, ID attributes, the DOMDocument `create*` family,
//! `importNode` / `adoptNode`, `DOMImplementation`, `cloneNode` and the document properties.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/oop.rs`.
//!
//! Key details:
//! - Each program in `dom_p5/` ran under `php -n` (8.5); its stdout is the `.out` file next to it.
//!   The elephc output must equal it byte for byte, with a clean heap-debug leak summary where the
//!   program frees everything it makes.
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

/// Keeps the SimpleXML bridge reachable (see package S): a plain AOT `loadXML` does not find
/// `__elephc_simplexml_load_dom` unless the program also names a SimpleXML entry point.
fn with_opaque_eval(source: &str) -> String {
    format!("{source}\nif ($argc > 99) {{ simplexml_import_dom($d); }}\n")
}

/// The attribute API, ParentNode / ChildNode helpers, IDs and element-only traversal properties
/// on a free-standing tree.
#[test]
fn test_dom_p5_free_standing_element_api() {
    check("t1", include_str!("dom_p5/t1.php"), include_str!("dom_p5/t1.out"), true);
}

/// The same API on a parsed (libxml2-bound) document: attributes are read, written, listed and
/// removed natively, namespaces are declared, and `create*` nodes are appended under it.
#[test]
fn test_dom_p5_bound_element_api() {
    if skip_without_xml_native("test_dom_p5_bound_element_api") { return; }
    check("t2", &with_opaque_eval(include_str!("dom_p5/t2.php")), include_str!("dom_p5/t2.out"), false);
}

/// `create*`, `importNode`, `adoptNode`, `DOMImplementation`, `cloneNode` and the document
/// properties of a free-standing document.
#[test]
fn test_dom_p5_document_api() {
    check("t3", include_str!("dom_p5/t3.php"), include_str!("dom_p5/t3.out"), true);
}
