//! Purpose:
//! Verifies string contexts on eval subclasses backed by native parent methods.
//!
//! Called from:
//! - The executable codegen CI suites.
//!
//! Key details:
//! - Inherited hooks, eval overrides, parameter coercion and throws share the same dispatch.

use crate::support::*;

/// Eval subclasses retain inherited native string conversion without hiding eval overrides.
#[test]
fn test_eval_subclasses_inherit_native_tostring_bundle() {
    let source = r#"<?php
class NativeInheritedString {
    public function __toString(): string { echo 'native|'; return 'base'; }
}
class NativeThrowingString {
    public function __toString(): string { throw new RuntimeException('cast failed'); }
}
class NativeNoString {}
$code = $argc > 0 ? '
class EvalInheritedString extends NativeInheritedString {}
class EvalIndirectString extends EvalInheritedString {}
class EvalOverrideString extends EvalInheritedString {
    public function __toString(): string { echo "eval|"; return "child"; }
}
class EvalThrowingString extends NativeThrowingString {}
class EvalNoString extends NativeNoString {}
function evalInheritedStringParameter(string $text): void { echo "param:", $text, "|"; }
$one = new EvalInheritedString();
$two = new EvalIndirectString();
echo $one, "|";
echo "concat:", "" . $two, "|";
evalInheritedStringParameter($two);
echo new EvalOverrideString(), "|";
try { echo new EvalThrowingString(); } catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
try { echo new EvalNoString(); } catch (Error $error) { echo $error->getMessage(), "|"; }
unset($one, $two, $error);
' : '';
eval($code);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "native|base|concat:native|base|native|param:base|eval|child|cast failed|",
        "Object of class EvalNoString could not be converted to string|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
