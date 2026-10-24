//! Purpose:
//! SimpleXML compiler and ABI coverage for every supported target.
//!
//! Called from:
//! - AST-to-EIR unit tests in CI, without linking native target archives.
//!
//! Key details:
//! - Covers document factories, dimensions, casts, JSON, DOM import, and iteration.

/// Compiles the same complete SimpleXML fixture through one supported target's emitter.
fn verify_simplexml_target(target: &str) {
    let module = super::lower_source_at_for_target(r###"<?php
$xml = new SimpleXMLIterator('<r id="a"><item>42</item><item>3</item></r>');
$xml->item[0] = '7';
$xml['id'] = 'b';
$xml->addChild('p:child', 'value', 'urn:p');
foreach ($xml->item as $key => $node) { echo $key, (string)$node, (int)$node, (float)$node; }
var_dump((bool)$xml->missing);
echo json_encode($xml), json_encode((array)$xml), $xml->asXML();
$doc = new DOMDocument(); $doc->loadXML('<r><v>shared</v></r>');
$imported = simplexml_import_dom($doc);
if ($imported instanceof SimpleXMLElement) { $imported->v = 'changed'; }
echo $doc->saveXML();
"###, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains("elephc_xml_sx_load"), "{target}: document parsing boundary");
    assert!(assembly.contains("elephc_xml_sx_mutate"), "{target}: document mutation boundary");
    assert!(assembly.contains(&crate::names::method_symbol("SimpleXMLElement", "__elephc_array")),
        "{target}: native property projection body");
}

/// Schedules target-sensitive SimpleXML fixtures independently under CI's ordinary timeout.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Verifies SimpleXML lowering and emitted calls on the selected supported target.
        #[test]
        fn $name() { verify_simplexml_target($target); }
    };
}
target_test!(simplexml_target_macos, "macos-aarch64");
target_test!(simplexml_target_ios, "ios-arm64");
target_test!(simplexml_target_ios_sim, "ios-sim-arm64");
target_test!(simplexml_target_linux_arm, "linux-aarch64");
target_test!(simplexml_target_linux_x86, "linux-x86_64");
