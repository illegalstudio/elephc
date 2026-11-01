//! Inherited native method closures created by dynamically declared subclasses.

use super::*;

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
