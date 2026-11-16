//! Purpose:
//! Retires nested property receivers captured by opaque eval call arguments.
//!
//! Called from:
//! - Managed-libxml2 executable CI through the SimpleXML test module.
//!
//! Key details:
//! - Typed failures, named calls and closures exercise temporary XML wrapper owners.
//! - Ordinary by-reference writes keep their receiver until normal or throwing writeback.

use crate::support::*;

/// Call argument cleanup releases XML views and preserves ordinary property reference writeback.
#[test]
fn test_simplexml_eval_call_argument_receiver_owners_bundle() {
    if skip_without_xml_native("test_simplexml_eval_call_argument_receiver_owners_bundle") { return; }
    let source = r#"<?php
class EvalArgReceiverCapability extends SimpleXMLElement {}
$capability = new EvalArgReceiverCapability('<r/>');
$source = $argc > 0 ? '
class EvalArgReceiverXml extends SimpleXMLElement {}
class EvalArgReceiverBox { public mixed $inner; public mixed $value = "old"; }
class EvalArgReceiverThrowRead {
    public function __get(string $name): mixed { throw new RuntimeException("read"); }
}
function evalArgTyped(SimpleXMLElement $node): void { echo "unexpected|"; }
function evalArgNullable(?SimpleXMLElement $node): void { echo "nullable|"; }
function evalArgRef(&$value): void { $value = "set"; }
function evalArgRefThrow(&$value): void { $value = "throw"; throw new RuntimeException("write"); }
function evalArgThrowName(): string { throw new RuntimeException("name"); }
function evalArgFreshBox(): EvalArgReceiverBox { return new EvalArgReceiverBox(); }
$root = new EvalArgReceiverXml("<r/>");
try { evalArgTyped($root->missing->child); } catch (TypeError $error) { echo "typed|"; unset($error); }
evalArgNullable($root->missing->child);
evalArgNullable(node: $root->missing->child);
$property = "child"; evalArgNullable($root->missing->{$property});
$callback = function (?SimpleXMLElement $node): void { echo "closure|"; };
$callback($root->missing->child);
$box = new EvalArgReceiverBox(); $box->inner = new EvalArgReceiverBox();
evalArgRef($box->inner->value); echo $box->inner->value, "|";
try { evalArgRefThrow($box->inner->value); }
catch (RuntimeException $error) { echo $error->getMessage(), ":", $box->inner->value, "|"; unset($error); }
try { evalArgNullable((new EvalArgReceiverThrowRead())->value); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
try { evalArgNullable(evalArgFreshBox()->{evalArgThrowName()}); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
$indexed = new EvalArgReceiverXml("<r><a><b/></a></r>");
evalArgNullable($indexed->a->b[0]);
$box->inner->value = new ArrayObject([null]);
evalArgNullable($box->inner->value[0]);
try { evalArgNullable($indexed->a->b[evalArgThrowName()]); }
catch (RuntimeException $error) { echo "index|"; unset($error); }
$box->inner->value = new EvalArgReceiverBox();
try { evalArgNullable($box->inner->value[0]); }
catch (Error $error) { echo "offset|"; unset($error); }
unset($root, $indexed, $box, $callback, $property);
' : '';
eval($source); unset($source, $capability);
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "typed|nullable|nullable|nullable|closure|set|write:throw|read|name|nullable|nullable|index|offset|";
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[expected]);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}
