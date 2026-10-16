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
function countItems(mixed $items): int { return count($items); }
echo count($div->childNodes), countItems($div->childNodes);
foreach ($div->childNodes as $child) {
    echo $child->nodeValue, $div->getElementsByTagName('span')->length;
    foreach ($div->childNodes as $inner) { echo $inner->nodeName; }
}
echo $doc->saveXML($div);
echo $doc->nodeValue === null ? 'N' : 'bad';
$doc->loadHTML("<div title=\"&euro;&#x1F600;\r\n\">a&nbsp;\r\n<script>a<b&amp;]]></SCRIPT>x</ div>y</div>");
$div = $doc->getElementsByTagName('div')->item(0);
echo $div->getAttribute('title'), $div->nodeValue, $doc->saveXML($div);
echo $doc->ownerDocument === null ? 'N' : 'bad';
echo $div->ownerDocument === $doc ? 'O' : 'bad';
$doc->loadHTML('<DIV CLASS="first" class="ignored">a<?One x><?Two?>b</DIV>');
$div = $doc->getElementsByTagName('div')->item(0);
echo $div->getAttribute('CLASS'), $doc->getElementsByTagName('DIV')->length;
foreach ($div->childNodes as $child) {
    if ($child instanceof DOMProcessingInstruction) { echo $child->target, $child->data; }
}
echo $doc->saveXML($div);
$doc->loadHTML('<!--lead--><?worker data?><div>A</not>B</div>');
echo $doc->childNodes->length;
$div = $doc->getElementsByTagName('div')->item(0);
echo $div->childNodes->length;
$doc->loadHTML('<div>a<!--unfinished');
echo $doc->saveXML($doc->getElementsByTagName('div')->item(0));
$doc->loadHTML('<!--lead--><html><head></head><body>x</body></html><!--tail--><?Tail data?>');
echo $doc->childNodes->length, $doc->getElementsByTagName('html')->length;
foreach ($doc->childNodes as $child) { echo $child->nodeName; }
$doc->loadHTML("<html><body>x</body></html> \n");
echo $doc->saveXML($doc->getElementsByTagName('html')->item(0));
$doc->loadHTML('<body>A</body><div>B</div>');
echo $doc->getElementsByTagName('body')->item(0)->nodeValue, $doc->saveXML();
$doc->loadHTML('<html><body>A</body></html><?worker data><div>B</div><!--inside--><div>C</div>');
echo $doc->getElementsByTagName('body')->length, $doc->saveXML();
echo $doc->childNodes->item(0)->nextSibling === $doc->childNodes->item(1) ? 'linked' : 'bad';
$doc->loadHTML(' <div>Hi</div> ', LIBXML_NOBLANKS);
echo count($doc->getElementsByTagName('body')->item(0)->childNodes);
$doc->loadHTML("<html>\n<head> <title> A </title> </head>\n<body> <div>Hi</div> </body>\n</html>", LIBXML_NOBLANKS);
echo $doc->saveXML($doc->getElementsByTagName('html')->item(0));
$doc->loadHTML('<?xml encoding="UTF-8"><div title="é&nbsp;€😀">é</div>');
echo $doc->saveXML($doc->getElementsByTagName('div')->item(0));
$doc->loadHTML('<html><head></head><div>B</div></html><html><span>C</span></html>');
echo $doc->saveXML(), $doc->getElementsByTagName('body')->item(0)->childNodes->item(0)->nodeName;
$doc->loadHTML('<div>a<embed>b</embed><source>c</source><track>d</track><wbr>e</wbr></div>');
echo $doc->saveXML();
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen_support::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains(&crate::names::method_symbol("DOMNodeList", "getiterator")), "{target}: aggregate factory");
    assert!(assembly.contains(&crate::names::method_symbol("DOMNodeList", "count")), "{target}: Countable dispatch");
    assert!(assembly.contains(&crate::names::method_symbol("__ElephcDOMNodeListIterator", "valid")), "{target}: independent iterator");
    assert!(assembly.contains(&crate::names::method_symbol("DOMProcessingInstruction", "__construct")), "{target}: PI construction");
    assert!(assembly.contains("__rt_array_take_boxed") || assembly.contains("__rt_hash_pop_boxed"), "{target}: stack pop path");
    assert!(module.functions.iter().any(|function| {
        function.name == "__elephc_dom_escape_attr_non_ascii"
    }), "{target}: Unicode attribute references use the ordinary target-aware PHP lowering path");
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
