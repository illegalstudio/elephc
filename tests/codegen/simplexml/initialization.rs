//! Purpose:
//! Covers recovered rootless documents and SimpleXML objects without a parent constructor.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on Linux and macOS.
//!
//! Key details:
//! - Missing native nodes throw while valid empty selections keep their PHP behavior.
//! - AOT and opaque eval run with both optimizer modes and strict default heap checks.

use crate::support::*;

/// Recovery keeps documents alive without treating rootless objects as initialized selections.
#[test]
fn test_simplexml_initialization_state_bundle() {
    if skip_without_xml_native("test_simplexml_initialization_state_bundle") { return; }
    let source = r#"<?php
class StateXmlNoParent extends SimpleXMLElement { public function __construct() {} }
function StateXmlMissingNode(SimpleXMLElement $xml, int $selectedOperation): void {
    echo 'scalars:', (int)$xml, ':', (float)$xml, ':[', (string)$xml, ']|';
    for ($operation = 0; $operation < 25; $operation++) {
        if ($selectedOperation >= 0 && $operation !== $selectedOperation) { continue; }
        echo $operation, ':';
        try {
            if ($operation === 0) { var_dump(count($xml)); }
            elseif ($operation === 1) { var_dump($xml->getName()); }
            elseif ($operation === 2) { var_dump($xml->asXML()); }
            elseif ($operation === 3) { var_dump($xml->saveXML()); }
            elseif ($operation === 4) { var_dump($xml->xpath('a')); }
            elseif ($operation === 5) { var_dump($xml->children()); }
            elseif ($operation === 6) { var_dump($xml->attributes()); }
            elseif ($operation === 7) { var_dump($xml->getNamespaces()); }
            elseif ($operation === 8) { var_dump((bool)$xml); }
            elseif ($operation === 9) { var_dump($xml->getDocNamespaces(false, false)); }
            elseif ($operation === 10) { $xml->addChild('c'); }
            elseif ($operation === 11) { $xml->addAttribute('a', 'b'); }
            elseif ($operation === 12) { echo json_encode((array)$xml); }
            elseif ($operation === 13) { echo json_encode($xml); }
            elseif ($operation === 14) { $xml->__debugInfo(); }
            elseif ($operation === 15) { var_dump($xml->missing); }
            elseif ($operation === 16) { var_dump(isset($xml->missing)); }
            elseif ($operation === 17) { unset($xml->missing); }
            elseif ($operation === 18) { var_dump($xml['id']); }
            elseif ($operation === 19) { var_dump(isset($xml['id'])); }
            elseif ($operation === 20) { unset($xml['id']); }
            elseif ($operation === 21) { $xml->rewind(); }
            elseif ($operation === 22) { foreach ($xml as $item) { echo 'unexpected-item'; } }
            elseif ($operation === 23) { $xml->newChild = 'v'; }
            else { $xml['id'] = 'v'; }
            echo 'accepted|';
        } catch (Error $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    }
    var_dump($xml->valid(), $xml->hasChildren(), $xml->getChildren());
    try { $xml->current(); } catch (Error $error) { echo $error->getMessage(), '|'; }
    try { $xml->key(); } catch (Error $error) { echo $error->getMessage(), '|'; }
    $xml->next(); echo 'next|';
    try { var_dump($xml->getDocNamespaces(false, true)); }
    catch (Error $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    try { var_dump($xml->registerXPathNamespace('p', 'urn:p')); }
    catch (Error $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    $copy = clone $xml;
    echo 'copy:[', (string)$copy, ']|';
    try { var_dump($copy->getDocNamespaces()); }
    catch (Error $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    unset($copy, $error);
}
function StateXmlEmpty(SimpleXMLElement $xml): void {
    echo 'empty:', count($xml), ':', $xml->getName(), ':', (int)(bool)$xml, ':', json_encode((array)$xml), ':', json_encode($xml), '|';
    var_dump($xml->asXML(), $xml->children(), $xml->attributes(), $xml->xpath('a'));
    echo json_encode($xml->getNamespaces()), ':', json_encode($xml->getDocNamespaces()), ':', json_encode($xml->getDocNamespaces(false, false)), '|';
    var_dump($xml->registerXPathNamespace('p', 'urn:p'));
    foreach ($xml as $item) { echo 'unexpected-item|'; }
}
set_error_handler(function(int $level, string $message): bool { echo 'warning|'; return true; });
$selectedOperation = $argc > 1 ? (int)$argv[1] : -1;
$loaded = simplexml_load_string('XXXXXXX^', options: LIBXML_RECOVER);
if ($loaded instanceof SimpleXMLElement) { StateXmlMissingNode($loaded, $selectedOperation); } else { echo 'load-failed|'; }
file_put_contents('rootless-state.xml', 'XXXXXXX^');
try {
    $file = simplexml_load_file('rootless-state.xml', options: LIBXML_RECOVER);
    if ($file instanceof SimpleXMLElement) { StateXmlMissingNode($file, $selectedOperation); } else { echo 'file-failed|'; }
    $constructed = new SimpleXMLElement('XXXXXXX^', LIBXML_RECOVER);
    StateXmlMissingNode($constructed, $selectedOperation);
    $url = new SimpleXMLElement('rootless-state.xml', LIBXML_RECOVER, true);
    StateXmlMissingNode($url, $selectedOperation);
} finally { unlink('rootless-state.xml'); }
$bare = new StateXmlNoParent();
StateXmlMissingNode($bare, $selectedOperation);
$root = new SimpleXMLElement('<r/>');
echo 'root:', count($root), ':', (int)(bool)$root, ':[', (string)$root, ']:', $root->getName(), '|';
$children = $root->children();
if ($children instanceof SimpleXMLElement) { StateXmlEmpty($children); } else { echo 'children-failed|'; }
$attributes = $root->attributes();
if ($attributes instanceof SimpleXMLElement) { StateXmlEmpty($attributes); } else { echo 'attributes-failed|'; }
$missing = $root->missing;
if ($missing instanceof SimpleXMLElement) { StateXmlEmpty($missing); } else { echo 'missing-failed|'; }
restore_error_handler();
unset($loaded, $file, $constructed, $url, $bare, $root, $children, $attributes, $missing);
"#;
    let error = "Error:SimpleXMLElement is not properly initialized|";
    let expected_for = |selected: Option<usize>| {
        let failures: String = (0..25).filter(|index| selected.is_none_or(|selected| selected == *index))
            .map(|index| format!("{index}:{error}")).collect();
        let common = format!("scalars:0:0:[]|{failures}bool(false)\nbool(false)\nNULL\nIterator not initialized or already consumed|Iterator not initialized or already consumed|next|");
        let recovered = format!("warning|warning|warning|{common}bool(false)\nbool(true)\ncopy:[]|bool(false)\n");
        let bare = format!("{common}{error}{error}copy:[]|{error}");
        let empty = "empty:0::0:[]:{}|bool(false)\nNULL\nNULL\nNULL\n[]:[]:[]|bool(true)\n";
        format!("{}{bare}root:0:0:[]:r|{}", recovered.repeat(4), empty.repeat(3))
    };
    let expected = expected_for(None);
    let evaluated = source.strip_prefix("<?php").unwrap().replace("StateXml", "EvalStateXml")
        .replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $activate = new SimpleXMLElement('<activate/>'); unset($activate); $source = $argc > 0 ? '{evaluated}' : ''; eval($source); unset($source);");
    let cases = [
        BundleCase { name: "initialization_aot", source, expected: &expected },
        BundleCase { name: "initialization_eval", source: &eval, expected: &expected },
    ];
    let selections: Vec<String> = (0..=25).map(|index| index.to_string()).collect();
    let mut argvs: Vec<Vec<&str>> = vec![vec![]];
    argvs.extend(selections.iter().map(|selection| vec![selection.as_str()]));
    let argvs: Vec<&[&str]> = argvs.iter().map(Vec::as_slice).collect();
    let run = || {
        let outputs = compile_and_run_with_heap_debug_per_argv(&bundle_source(&cases), &argvs);
        assert_eq!(outputs.len(), argvs.len());
        let mut failures = Vec::new();
        for (index, out) in outputs.iter().enumerate() {
            let selected = index.checked_sub(1);
            let label = selected.map_or_else(|| "all operations".to_string(), |selected| format!("operation {selected}"));
            let expected = expected_for(selected);
            if !out.success || !out.stderr.contains("HEAP DEBUG: leak summary: clean") {
                failures.push(format!("{label}: stdout={} stderr={}", out.stdout, out.stderr));
            }
            match split_bundle_output(&cases, &out.stdout) {
                Ok(actual) => for (case, actual) in cases.iter().zip(actual) {
                    if actual != expected { failures.push(format!("{label}, {}: stdout={actual} expected={expected}", case.name)); }
                },
                Err(error) => failures.push(format!("{label}: {error}")),
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    };
    run();
    without_ir_opt(run);
}
