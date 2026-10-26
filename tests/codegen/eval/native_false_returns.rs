//! Purpose:
//! Checks literal false members in registered AOT callable return metadata.
//!
//! Called from:
//! - Codegen CI through the eval test module.
//!
//! Key details:
//! - Runtime-selected eval forces native function and method return validation.

use crate::support::*;

/// A native class-or-false signature accepts its false branch as well as its object branch.
#[test]
fn test_eval_native_function_and_method_false_union_returns() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NativeFalseValue { public string $label = 'value'; }
function nativeFalseFactory(bool $create): NativeFalseValue|false {
    if ($create) { return new NativeFalseValue(); }
    return false;
}
class NativeFalseFactory {
    public function make(bool $create): NativeFalseValue|false { return nativeFalseFactory($create); }
}
$source = $argc > 0 ? '
var_dump(nativeFalseFactory(false));
echo nativeFalseFactory(true)->label, "|";
$factory = new NativeFalseFactory();
var_dump($factory->make(false));
echo $factory->make(true)->label;
' : '';
eval($source);
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "bool(false)\nvalue|bool(false)\nvalue");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
