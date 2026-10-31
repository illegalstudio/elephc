//! Purpose:
//! Covers erased nested dimension receivers whose current value is a native XML object.
//!
//! Called from:
//! - Managed XML executable codegen CI.
//!
//! Key details:
//! - The final key may replace the parent array before its object element is selected.

use crate::support::*;

/// AOT and eval select the current nested object after key effects replace its parent array.
#[test]
fn test_simplexml_nested_storage_unset_selects_after_key_bundle() {
    if skip_without_xml_native("test_simplexml_nested_storage_unset_selects_after_key_bundle") { return; }
    let source = r#"<?php
function replaceNestedStorageEntries(SimpleXMLElement $replacement): string {
    global $nestedStorageEntries;
    echo 'key|';
    $nestedStorageEntries = [$replacement];
    return 'id';
}
$old = new SimpleXMLElement('<old id="old"/>');
$replacement = new SimpleXMLElement('<new id="new"/>');
$nestedStorageEntries = [$old];
unset($nestedStorageEntries[0][replaceNestedStorageEntries($replacement)]);
echo $old->asXML(), $replacement->asXML();
unset($old, $replacement, $nestedStorageEntries);
"#;
    let renamed = source.replace("replaceNestedStorageEntries", "replaceEvalNestedStorageEntries");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $activate = new SimpleXMLElement('<activate/>'); unset($activate); $source = $argc > 0 ? '{body}' : ''; eval($source); unset($source);");
    let expected = "key|<?xml version=\"1.0\"?>\n<old id=\"old\"/>\n<?xml version=\"1.0\"?>\n<new/>\n";
    let ordered = r#"<?php
function orderedUnsetKey(SimpleXMLElement $replacement): string {
    global $orderedUnsetRows;
    echo 'key|'; $orderedUnsetRows = [$replacement]; return 'id';
}
function orderedUnsetIndex(): int { echo 'inner|'; return 0; }
function orderedUnsetRoot(): array {
    global $orderedUnsetRows;
    echo 'root|';
    if (!is_array($orderedUnsetRows)) { throw new Error('expected array'); }
    return (array)$orderedUnsetRows;
}
function orderedUnsetLateKey(): string {
    global $orderedUnsetPosition;
    echo 'late|'; $orderedUnsetPosition = 1; return 'id';
}
function orderedUnsetFailKey(): string { echo 'fail|'; throw new Error('stop'); }
$old = new SimpleXMLElement('<old id="old"/>');
$replacement = new SimpleXMLElement('<new id="new"/>'); $orderedUnsetRows = [$old];
unset($orderedUnsetRows[orderedUnsetIndex()][orderedUnsetKey(
    $replacement instanceof SimpleXMLElement ? $replacement : throw new Error('expected XML'))]);
echo isset($old['id']) ? 'old' : '-', ':', isset($replacement['id']) ? 'new' : '-', "\n";
$old = new SimpleXMLElement('<old id="old"/>');
$replacement = new SimpleXMLElement('<new id="new"/>'); $orderedUnsetRows = [$old];
unset(orderedUnsetRoot()[0][orderedUnsetKey(
    $replacement instanceof SimpleXMLElement ? $replacement : throw new Error('expected XML'))]);
echo isset($old['id']) ? 'old' : '-', ':', isset($replacement['id']) ? 'new' : '-', "\n";
$old = new SimpleXMLElement('<old id="old"/>');
$replacement = new SimpleXMLElement('<new id="new"/>'); $orderedUnsetRows = [$old, $replacement];
$orderedUnsetPosition = 0;
unset($orderedUnsetRows[$orderedUnsetPosition][orderedUnsetLateKey()]);
echo isset($old['id']) ? 'old' : '-', ':', isset($replacement['id']) ? 'new' : '-', "\n";
$old = new SimpleXMLElement('<old id="old"/>');
$replacement = new SimpleXMLElement('<new id="new"/>'); $orderedUnsetRows = [$old];
try { unset(orderedUnsetRoot()[orderedUnsetIndex()][orderedUnsetFailKey()]); }
catch (Error $error) { echo 'caught|'; }
echo isset($old['id']) ? 'old' : '-', ':', isset($replacement['id']) ? 'new' : '-', "\n";
unset($old, $replacement, $orderedUnsetRows, $orderedUnsetPosition, $error);
"#;
    let ordered_body = ordered.strip_prefix("<?php").unwrap()
        .replace("orderedUnset", "evalOrderedUnset").replace('\\', "\\\\").replace('\'', "\\'");
    let ordered_eval = format!("<?php $activate = new SimpleXMLElement('<activate/>'); unset($activate); $source = $argc > 0 ? '{ordered_body}' : ''; eval($source); unset($source);");
    let ordered_expected = "inner|key|old:-\nroot|key|-:new\nlate|old:-\nroot|inner|fail|caught|old:new\n";
    let scalar_body = r#"
$scalarCases = json_decode('["abc","",5,1.5,true,false,null]', true);
if (!is_array($scalarCases)) { throw new Error('expected array'); }
foreach ($scalarCases as $value) {
    $rows = [$value];
    try { unset($rows[0][0][0]); echo "quiet\n"; }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
}
$holder = new stdClass(); $holder->field = $scalarCases[0];
try { unset($holder->field[0][1]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$rows = [$scalarCases[2]];
unset($rows[0]->field[0]); echo "property-quiet\n";
$text = $scalarCases[0];
try { unset($text[0]->field[0]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
try { unset($holder->field[0]->next[0]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$name = 'next';
try { unset($holder->field[0]->{$name}[0]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$rows = $scalarCases;
try { unset($rows[0][1]->next[0]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$plain = json_decode('{}');
try { unset($plain[0][1]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$rows = $scalarCases; $rows[0] = $foreignUnsetClosure;
try { unset($rows[0][0][1]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$closureCases = $scalarCases;
$closureCases[0] = $foreignUnsetClosure; $closureCases[1] = static function(): int { return 2; };
for ($i = 0; $i < 2; $i++) {
    $closure = $closureCases[$i];
    $rows = $scalarCases; $rows[0] = $closure;
    try { unset($rows[0][1]); }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
    $erased = json_decode('{}'); $erased->p = $closure;
    try { unset($erased->p[1]); }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
    try { unset($closure->p[0]); }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
    $property = 'dyn';
    try { unset($closure->{$property}[0]); }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
    $erased->nested = $closure;
    try { unset($erased->nested->p[0]); }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
    unset($erased->nested->absent); echo "final-property-quiet\n";
}
$rows = $scalarCases; $rows[0] = $plain;
try { unset($rows[0][1]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$erased->p = $plain;
try { unset($erased->p[1]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
unset($scalarCases, $value, $rows, $holder, $error, $text, $plain, $foreignUnsetClosure, $name,
    $closureCases, $closure, $erased, $property, $i);
"#;
    let activation = "<?php $activate = new SimpleXMLElement('<activate/>'); unset($activate); $foreignUnsetClosure = static fn(): int => 1; ";
    let scalar_aot = format!("{activation}{scalar_body}");
    let scalar_body = scalar_body.replace('\\', "\\\\").replace('\'', "\\'");
    let scalar_eval = format!("{activation}$source = $argc > 0 ? '{scalar_body}' : ''; eval($source); unset($source);");
    let string_error = "Cannot use string offset as an array";
    let scalar_error = "Cannot unset offset in a non-array variable";
    let object_offset_error = "Cannot use string offset as an object";
    let closure_expected = "Cannot use object of type Closure as array\nCannot use object of type Closure as array\n\
        Cannot create dynamic property Closure::$p\nCannot create dynamic property Closure::$dyn\n\
        Cannot create dynamic property Closure::$p\nfinal-property-quiet\n";
    let scalar_expected = format!(
        "{string_error}\n{string_error}\n{scalar_error}\n{scalar_error}\n{scalar_error}\nquiet\nquiet\n{string_error}\nproperty-quiet\n\
         {object_offset_error}\n{object_offset_error}\n{object_offset_error}\n{object_offset_error}\nCannot use object of type stdClass as array\nCannot use object of type Closure as array\n\
         {closure_expected}{closure_expected}Cannot use object of type stdClass as array\nCannot use object of type stdClass as array\n");
    let dynamic_body = r#"
class EvalFinalUnsetAccess implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return false; }
    public function offsetGet(mixed $offset): mixed { return null; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void { echo 'dynamic:', $offset, '|'; }
}
return [new EvalFinalUnsetAccess(), static function(): int { return 3; }];
"#.replace('\\', "\\\\").replace('\'', "\\'");
    let dynamic = format!(r#"<?php
$activate = new SimpleXMLElement('<activate/>'); unset($activate);
$source = $argc > 0 ? '{dynamic_body}' : '';
$returned = eval($source);
if (!is_array($returned)) {{ throw new Error('expected array'); }}
$dynamic = $returned[0]; $closure = $returned[1];
$rows = json_decode('[0]', true);
if (!is_array($rows)) {{ throw new Error('expected array'); }}
$rows[0] = $dynamic; unset($rows[0][1]);
$holder = json_decode('{{}}'); $holder->p = $dynamic; unset($holder->p[2]);
$holder->p = $closure;
try {{ unset($holder->p[0][1]); }} catch (Error $error) {{ echo $error->getMessage(), "\n"; }}
try {{ unset($holder->p[0]); }} catch (Error $error) {{ echo $error->getMessage(), "\n"; }}
try {{ unset($holder->p->slot[0]); }} catch (Error $error) {{ echo $error->getMessage(), "\n"; }}
unset($holder->p->absent); echo "final-property-quiet\n";
unset($source, $returned, $dynamic, $closure, $rows, $holder, $error);
"#);
    let dynamic_expected = "dynamic:1|dynamic:2|Cannot use object of type Closure as array\n\
        Cannot use object of type Closure as array\nCannot create dynamic property Closure::$slot\nfinal-property-quiet\n";
    assert_bundle_with(&[
        BundleCase { name: "nested_unset_aot", source, expected },
        BundleCase { name: "nested_unset_eval", source: &eval, expected },
        BundleCase { name: "ordered_nested_unset_aot", source: ordered, expected: ordered_expected },
        BundleCase { name: "ordered_nested_unset_eval", source: &ordered_eval, expected: ordered_expected },
        BundleCase { name: "scalar_nested_unset_aot", source: &scalar_aot, expected: &scalar_expected },
        BundleCase { name: "scalar_nested_unset_eval", source: &scalar_eval, expected: &scalar_expected },
        BundleCase { name: "eval_declared_final_unset_access", source: &dynamic, expected: dynamic_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
        assert_eq!(out.stderr.matches("Automatic conversion of false to array is deprecated").count(), 2,
            "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
