//! Purpose:
//! php-verified coverage for DOM package P3: character data, text, comment, CDATA, attribute,
//! fragment, doctype and named-node-map classes.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/oop.rs`.
//!
//! Key details:
//! - Each program in `dom_p3/` ran under `php -n` (8.5); its stdout is the `.out` file next to it.
//!   The elephc output must equal it byte for byte, with a clean heap-debug leak summary.
//! - `loadXML` needs the managed libxml2 native package, hence the skip gate.

use crate::support::*;

/// Runs `source` and checks stdout against the php-recorded `expected` and the leak summary.
fn check(name: &str, source: &str, expected: &str) {
    check_leaks(name, source, expected, true);
}

/// `check`, with the leak summary optional: a free-standing tree leaks through the parent/child
/// cycle of the base DOM model (fixed by package P4: attached nodes hold their parent, siblings and document weakly).
fn check_leaks(name: &str, source: &str, expected: &str, leak_clean: bool) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{name}: stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{name}");
    if leak_clean {
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{name}: {}", out.stderr);
    }
}

/// Keeps the SimpleXML bridge reachable: at the base of this work a plain AOT `loadXML` does not
/// find `__elephc_simplexml_load_dom` (13 of the 22 `dom_and_iterator` / `dom_cache` tests fail
/// the same way), because reachability prunes the XML group unless the program also names a
/// SimpleXML entry point. Remove this once that base defect is repaired.
fn with_opaque_eval(source: &str) -> String {
    format!("{source}\nif ($argc > 99) {{ simplexml_import_dom($d); }}\n")
}

/// Character-data offset methods and `data` / `length` on nodes bound to the native libxml2 tree.
#[test]
fn test_dom_p3_character_data_on_bound_nodes() {
    if skip_without_xml_native("test_dom_p3_character_data_on_bound_nodes") { return; }
    check("t1", &with_opaque_eval(include_str!("dom_p3/t1.php")), include_str!("dom_p3/t1.out"));
}

/// Constructors and properties of free-standing text, comment, CDATA, attribute, doctype and map.
#[test]
fn test_dom_p3_free_standing_node_constructors() {
    check("t2", include_str!("dom_p3/t2.php"), include_str!("dom_p3/t2.out"));
}

/// `splitText` on a text node bound to the native tree links the tail after the node through
/// `DOMNode::insertBefore` (package P4).
#[test]
fn test_dom_p3_split_text_on_bound_nodes() {
    if skip_without_xml_native("test_dom_p3_split_text_on_bound_nodes") { return; }
    check("t3", &with_opaque_eval(include_str!("dom_p3/t3.php")), include_str!("dom_p3/t3.out"));
}

/// Named-node-map lookups, doctype constructor, fragment `appendXML` (elephc-only setup, outputs
/// checked against php's documented behaviour: see the program's header).
#[test]
fn test_dom_p3_named_node_map_doctype_and_fragment() {
    if skip_without_xml_native("test_dom_p3_named_node_map_doctype_and_fragment") { return; }
    check_leaks("t5", &with_opaque_eval(include_str!("dom_p3/t5.php")), include_str!("dom_p3/t5.out"), false);
}

/// `splitText` and `wholeText` on free-standing nodes.
#[test]
fn test_dom_p3_split_text_and_whole_text() {
    check_leaks("t4", include_str!("dom_p3/t4.php"), include_str!("dom_p3/t4.out"), true);
}
