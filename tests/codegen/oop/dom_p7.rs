//! Purpose:
//! php-verified coverage for DOM package P7: the serializers (`saveXML`, `save`, `saveHTML`,
//! `saveHTMLFile`, `C14N`, `C14NFile`, `formatOutput`) and the `textContent` setter.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/oop.rs`.
//!
//! Key details:
//! - Each program in `dom_p7/` ran under `php -n` (8.5); its stdout is the `.out` file next to it.
//!   The elephc output must equal it byte for byte.
//! - Bound documents are serialized by libxml2 itself (`xmlSaveToIO`, `xmlSaveTree`,
//!   `xmlC14NExecute`); free-standing trees by the PHP writer of `writer_xml.rs`, and HTML by
//!   importing the tree into a native HTML document (`htmlDocDumpMemoryFormat`).
//! - `loadXML` / `loadHTML` need the managed libxml2 native package, hence the skip gate;
//!   serializer and loader programs pull the SimpleXML bridge in by themselves.

use crate::support::*;

/// Runs `source` and checks stdout against the php-recorded `expected`.
fn check(name: &str, source: &str, expected: &str) {
    if skip_without_xml_native(name) { return; }
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{name}: stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{name}");
}

/// A free-standing tree: header, escaping rules, node kinds, formatOutput, options, node form, save().
#[test]
fn test_dom_p7_save_xml_free_standing() {
    check("t1", include_str!("dom_p7/t1.php"), include_str!("dom_p7/t1.out"));
}

/// A parsed document: libxml2 serialization, options, node form, C14N, save() and C14NFile().
#[test]
fn test_dom_p7_save_xml_bound_and_c14n() {
    check("t2", include_str!("dom_p7/t2.php"), include_str!("dom_p7/t2.out"));
}

/// `textContent` / `nodeValue` assigned to a free-standing element replace its children.
#[test]
fn test_dom_p7_text_content_setter() {
    check("t3", include_str!("dom_p7/t3.php"), include_str!("dom_p7/t3.out"));
}

/// `saveHTML` / `saveHTMLFile` of a built tree, with libxml2's HTML 4 serializer: document and node forms.
#[test]
fn test_dom_p7_save_html() {
    check("t4", include_str!("dom_p7/t4.php"), include_str!("dom_p7/t4.out"));
}

/// `saveHTML` of a parsed document: the serializer is native, the tree comes from `loadHTML`, whose
/// current parser (replaced by package P6) does not build libxml2's html/body structure.
#[test]
#[ignore = "needs the libxml2 HTML4 parser of package P6"]
fn test_dom_p7_save_html_of_a_parsed_document() {
    check("t5", include_str!("dom_p7/t5.php"), include_str!("dom_p7/t5.out"));
}
