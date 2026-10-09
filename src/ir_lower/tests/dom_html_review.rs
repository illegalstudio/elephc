//! Purpose:
//! Checks DOM HTML tree and NodeList lowering for all supported targets.
//!
//! Called from:
//! - The AST-to-EIR regression tests.
//!
//! Key details:
//! - Complex sibling trees exercise boxed stack pop and interface traversal.
//! - Target cases are independently scheduled under the normal CI timeout.
//! - Generic containers retain concrete DOM member types through the complete frontend.

/// Validates one target's DOM stack and interface method assembly boundaries.
fn verify(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
$doc = new DOMDocument();
class HtmlBox<T> { public function __construct(public T $node) {} }
$boxed = new HtmlBox<DOMDocument>($doc);
echo $boxed->node->nodeName;
$doc->loadHTML('<!DOCTYPE html><div title="it\'s">A<!not><!--hidden--> <span>B</span><b>C</b></div>', LIBXML_NOBLANKS);
$div = $doc->getElementsByTagName('div')->item(0);
foreach ($div->childNodes as $child) { echo $child->nodeValue; }
echo $doc->saveXML($div);
echo $doc->nodeValue === null ? 'N' : 'bad';
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen_support::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains(&crate::names::method_symbol("DOMNodeList", "valid")), "{target}");
    assert!(assembly.contains("__rt_array_take_boxed") || assembly.contains("__rt_hash_pop_boxed"), "{target}: stack pop path");
}

/// Schedules every target independently without weakening the CI timeout.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Verifies DOM tree and iterator codegen for this supported target.
        #[test]
        fn $name() { verify($target); }
    };
}

target_test!(dom_html_review_macos, "macos-aarch64");
target_test!(dom_html_review_ios, "ios-arm64");
target_test!(dom_html_review_ios_sim, "ios-sim-arm64");
target_test!(dom_html_review_linux_arm, "linux-aarch64");
target_test!(dom_html_review_linux_x86, "linux-x86_64");
