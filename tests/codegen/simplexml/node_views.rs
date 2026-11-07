//! Purpose:
//! Pins non-element SimpleXML traversal, attribute projections and XML-node imports.
//!
//! Called from:
//! - Managed XML codegen CI for compiled PHP and opaque eval.
//!
//! Key details:
//! - One shared PHP fixture also feeds all five target emitter checks.
//! - Throwing writable fetches preserve RHS order and release selected native views.

use crate::support::*;

/// Attribute nodes differ from attribute lists, including writable fetches and native imports.
#[test]
fn test_simplexml_node_views_bundle() {
    if skip_without_xml_native("test_simplexml_node_views_bundle") { return; }
    let source = include_str!("../../fixtures/simplexml/node_views.php");
    let renamed = source.replace("NodeView", "EvalNodeView");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let failed = "SimpleXMLElement is not properly initialized|";
    let per_node = format!("rhs|{failed}rhs|{failed}key|rhs|{failed}rhs|{failed}{failed}key|{failed}{failed}rhs-error|direct|");
    let expected = per_node.repeat(4)
        + "<?xml version=\"1.0\"?>\n<r><a id=\"1\"/><!--note--><?target info?></r>\n"
        + &"1:0:0|1:0:0|".repeat(4)
        + "1:1|1:1|<?xml version=\"1.0\"?>\n<controls><a id=\"1\" b=\"\"/><z id=\"2\" b=\"\"/></controls>\n"
        + "NodeViewAlias:a|yes:0|SimpleXMLElement:r|<a shared=\"yes\">first</a>"
        + "<?xml version=\"1.0\"?>\n<r><a>second</a></r>\n"
        + &"2:simplexml_import_dom(): Invalid Nodetype to import|1|".repeat(3)
        + &"simplexml_import_dom(): Argument #1 ($node) must be a valid XML node|".repeat(2)
        + concat!(
            "filter:1:1:1:0:0:1:1|1:1:1|<?xml version=\"1.0\"?>\n<r id=\"1\" b=\"old\"/>\n",
            "<?xml version=\"1.0\"?>\n<r id=\"1\" b=\"old\" b=\"new\" b=\"next\" b=\"\"/>\n1:0|",
            "filter:1:1:1:0:0:1:1|1:1:1|<?xml version=\"1.0\"?>\n<r xmlns:p=\"urn:p\" p:id=\"1\" p:b=\"old\"/>\n",
            "<?xml version=\"1.0\"?>\n<r xmlns:p=\"urn:p\" p:id=\"1\" p:b=\"old\" b=\"new\" b=\"next\" b=\"\"/>\n1:0|<a>U</a>|UB:PB|",
        );
    let eval_expected = expected.replace("NodeView", "EvalNodeView");
    assert_bundle_with(&[
        BundleCase { name: "node_views_aot", source, expected: &expected },
        BundleCase { name: "node_views_eval", source: &eval, expected: &eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
        out.stdout
    });
}
