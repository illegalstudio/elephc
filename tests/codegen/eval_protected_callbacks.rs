//! Purpose:
//! End-to-end visibility coverage for callbacks created inside eval.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

/// An eval fragment inherits the native method's class scope, so an array
/// callback may invoke its protected receiver method exactly as PHP permits.
#[test]
fn test_eval_call_user_func_array_keeps_native_protected_method_scope() {
    let out = compile_and_run(
        r#"<?php
class NativeCallbackScope {
    protected function answer(string $value): string { return "ok:" . $value; }

    public function invoke(): string {
        return eval('return call_user_func_array([$this, "answer"], ["yes"]);');
    }
}

echo (new NativeCallbackScope())->invoke();
"#,
    );
    assert_eq!(out, "ok:yes");
}

/// A class declared by eval also retains its method scope while normalizing an
/// array callback to one of its protected methods.
#[test]
fn test_eval_class_call_user_func_array_keeps_protected_method_scope() {
    let out = compile_and_run(
        r#"<?php
$instance = eval('return new class {
    protected function answer(string $value): string { return "ok:" . $value; }

    public function invoke(): string {
        return call_user_func_array([$this, "answer"], ["yes"]);
    }
};');

echo $instance->invoke();
"#,
    );
    assert_eq!(out, "ok:yes");
}

/// A compiled parent method retains its class scope when its dynamic receiver
/// is an eval-owned child with a protected method.
#[test]
fn test_native_parent_dynamic_call_reaches_eval_child_protected_method() {
    let out = compile_and_run(
        r#"<?php
class NativeCallbackParent {
    public function invoke(string $method) {
        return $this->{$method}("yes");
    }
}

$instance = eval('return new class extends NativeCallbackParent {
    protected function answer(string $value): string { return "ok:" . $value; }
};');

echo $instance->invoke("answer");
"#,
    );
    assert_eq!(out, "ok:yes");
}
