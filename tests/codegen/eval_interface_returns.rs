//! Purpose:
//! Regression coverage for interface-typed objects returned from native code
//! when their concrete implementation is created by the eval runtime.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

/// A compiled caller can invoke an eval-owned object returned through an
/// interface type, including when the callee adapts a first-class callable.
#[test]
fn test_native_caller_invokes_eval_owned_interface_return() {
    let out = compile_and_run(
        r#"<?php
interface NativeEvalResolver {
    public function resolve(): array;
}

class NativeEvalResolverBase implements NativeEvalResolver {
    public function __construct(
        private readonly Closure $callable,
        private readonly Closure $arguments,
    ) {}

    public function resolve(): array {
        return [$this->callable, ($this->arguments)()];
    }
}

class NativeEvalRuntime {
    public function getResolver(callable $callable, ?ReflectionFunction $reflector = null): NativeEvalResolver {
        $callable = $callable(...);
        $parameters = ($reflector ?? new ReflectionFunction($callable))->getParameters();
        $arguments = static function () use ($parameters): array {
            $arguments = [];

            foreach ($parameters as $parameter) {
                if ('context' === $parameter->getName()) {
                    $arguments[] = $_SERVER;
                }
            }

            return $arguments;
        };

        return eval('class NativeEvalDynamicResolver extends NativeEvalResolverBase {}
        return new NativeEvalDynamicResolver($callable, $arguments);');
    }
}

$_SERVER['NATIVE_EVAL_CONTEXT'] = 'present';
$callback = eval('return static function (array $context): string {
    return $context["NATIVE_EVAL_CONTEXT"] ?? "missing";
};');
[$application, $arguments] = (new NativeEvalRuntime())->getResolver($callback)->resolve();
echo get_debug_type($arguments[0]), ':';
echo $application(...$arguments);
"#,
    );

    assert_eq!(out, "array:present");
}
