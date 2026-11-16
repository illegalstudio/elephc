//! Purpose:
//! Integration tests for first-class callable closures created inside `eval()` over compiled
//! (native) methods: receiver lifetime of `$obj->m(...)` and `$obj(...)` closures, and inherited
//! native methods captured by a dynamically declared subclass.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (via `property_access/mod.rs`).
//!
//! Key details:
//! - Receivers echo `:drop` from `__destruct()`, so the expected stdout pins exactly when the
//!   closure (or a failed closure creation) releases its captured receiver.
//! - Expected output matches php for the same fixture.

use super::*;

/// Verifies `(new C())(...)` inside `eval()` on an `__invoke()` object keeps the temporary
/// receiver alive while the closure exists and destroys it when the closure is unset, before
/// the trailing echo. PHP outputs "value:drop:done".
#[test]
fn test_eval_invokable_closure_temporary_receiver_is_released() {
    let out = compile_and_run(r#"<?php
class InvokableClosureLifetime {
    public function __invoke(): string { return 'value'; }
    public function __destruct() { echo ':drop'; }
}
eval('$callback = (new InvokableClosureLifetime())(...);
echo $callback(); unset($callback); echo ":done";');
"#);
    assert_eq!(out, "value:drop:done");
}

/// Verifies a failed `(new C())->missing(...)` inside `eval()` releases the temporary receiver
/// before the `Error` reaches the catch block, so the destructor output precedes ":caught".
#[test]
fn test_eval_failed_method_closure_releases_temporary_receiver() {
    let out = compile_and_run(r#"<?php
class FailedMethodClosureLifetime {
    public function __destruct() { echo ':drop'; }
}
eval('try { $callback = (new FailedMethodClosureLifetime())->missing(...); }
catch (Error $error) { echo ":caught"; }');
"#);
    assert_eq!(out, ":drop:caught");
}

/// Verifies a `$receiver->value(...)` closure created in `eval()` holds its own reference to the
/// receiver: unsetting `$receiver` does not destroy it, the closure still runs, and the
/// destructor fires only when the closure is unset. PHP outputs "value:drop:done".
#[test]
fn test_eval_method_closure_retains_receiver_until_closure_release() {
    let out = compile_and_run(r#"<?php
class MethodClosureLifetime {
    public function value(): string { return 'value'; }
    public function __destruct() { echo ':drop'; }
}
eval('$receiver = new MethodClosureLifetime();
$callback = $receiver->value(...);
unset($receiver);
echo $callback();
unset($callback);
echo ":done";');
"#);
    assert_eq!(out, "value:drop:done");
}

/// Verifies a `(new C())->value(...)` closure created in `eval()` over a temporary receiver keeps
/// it alive for the call and releases it exactly when the closure is unset. PHP outputs
/// "value:drop:done".
#[test]
fn test_eval_method_closure_temporary_receiver_is_released() {
    let out = compile_and_run(r#"<?php
class TemporaryMethodClosureLifetime {
    public function value(): string { return 'value'; }
    public function __destruct() { echo ':drop'; }
}
eval('$callback = (new TemporaryMethodClosureLifetime())->value(...);
echo $callback();
unset($callback);
echo ":done";');
"#);
    assert_eq!(out, "value:drop:done");
}

/// Verifies an `eval()` subclass can capture an inherited `final protected` native method as
/// `$this->resolve(...)`, hand it to a compiled class, and have it invoked there with
/// `...$array` unpacking into `string|false`, `?string` and `string|bool` parameters. PHP
/// outputs "storage:id:method:1".
#[test]
fn test_inherited_native_method_closure_invokes_with_unpacked_union_arguments() {
    let out = compile_and_run(r#"<?php
class NativeCallbackBase {
    final protected function resolve(string|false $registry, string $id, ?string $method, string|bool $load): string {
        return $registry . ':' . $id . ':' . $method . ':' . (int) $load;
    }
}
class NativeCallbackBox {
    public function __construct(private Closure $factory, private array $map) {}
    public function get(string $id): string { return ($this->factory)(...$this->map[$id]); }
}
eval('class DynamicNativeCallbackProvider extends NativeCallbackBase {
    public function consumer() {
        return new NativeCallbackBox($this->resolve(...), ["item" => ["storage", "id", "method", true]]);
    }
}
echo (new DynamicNativeCallbackProvider())->consumer()->get("item");');
"#);
    assert_eq!(out, "storage:id:method:1");
}
