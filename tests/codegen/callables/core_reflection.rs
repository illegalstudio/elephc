//! Purpose:
//! End-to-end AOT coverage for Core class-introspection functions shared with eval.
//!
//! Called from:
//! - `cargo test` through Rust's integration test harness.
//!
//! Key details:
//! - Fixtures cover late static binding, object and class-name method lookup, property
//!   visibility, inherited defaults, named arguments, and case-insensitive function names.

use crate::support::*;

/// Verifies `get_called_class()` follows late static binding for instance and static calls.
#[test]
fn test_core_get_called_class_aot_late_static_binding() {
    let out = compile_and_run(
        r#"<?php
        class CoreCalledBase {
            public static function who(): string { return GeT_CaLlEd_ClAsS(); }
            public function instanceWho(): string { return get_called_class(); }
        }
        class CoreCalledChild extends CoreCalledBase {}
        echo CoreCalledChild::who(), ":", (new CoreCalledChild())->instanceWho(), ":", CoreCalledBase::who();
        "#,
    );
    assert_eq!(out, "CoreCalledChild:CoreCalledChild:CoreCalledBase");
}

/// Verifies `get_class_methods()` accepts literal names, named arguments, and typed objects.
#[test]
fn test_core_get_class_methods_aot_visibility_and_inputs() {
    let out = compile_and_run(
        r#"<?php
        class CoreMethodsBase {
            public function alpha(): void {}
            protected function beta(): void {}
            private function gamma(): void {}
            public static function delta(): void {}
            public static function inside(): string {
                $methods = get_class_methods(object_or_class: CoreMethodsBase::class);
                return (in_array("alpha", $methods) ? "A" : "a")
                    . (in_array("beta", $methods) ? "B" : "b")
                    . (in_array("gamma", $methods) ? "G" : "g");
            }
        }
        $outside = GET_CLASS_METHODS(new CoreMethodsBase());
        echo (in_array("alpha", $outside) ? "A" : "a"),
             (in_array("beta", $outside) ? "B" : "b"),
             (in_array("gamma", $outside) ? "G" : "g"),
             (in_array("delta", $outside) ? "D" : "d"), ":", CoreMethodsBase::inside();
        "#,
    );
    assert_eq!(out, "AbgD:ABG");
}

/// Verifies `get_class_vars()` materializes inherited and scoped defaults as fresh Mixed values.
#[test]
fn test_core_get_class_vars_aot_defaults_and_visibility() {
    let out = compile_and_run(
        r#"<?php
        class CoreVarsBase {
            public int $plain = 4;
            public array $items = [1, 2];
            protected string $protected = "p";
            private bool $private = true;
            public static string $static = "s";
            public static function inside(): string {
                $vars = get_class_vars(class: CoreVarsBase::class);
                return $vars["plain"] . $vars["items"][1] . $vars["protected"]
                    . ($vars["private"] ? "T" : "F") . $vars["static"];
            }
        }
        $outside = GET_CLASS_VARS("CoreVarsBase");
        echo $outside["plain"], $outside["items"][0], $outside["static"], ":",
             isset($outside["protected"]) ? "x" : "-", isset($outside["private"]) ? "x" : "-",
             ":", CoreVarsBase::inside();
        "#,
    );
    assert_eq!(out, "41s:--:42pTs");
}

/// Verifies runtime class-name strings and concrete object subclasses select AOT metadata.
#[test]
fn test_core_class_introspection_aot_dynamic_inputs() {
    let out = compile_and_run(
        r#"<?php
        class CoreDynamicBase {
            public int $base = 4;
            public function baseMethod(): void {}
        }
        class CoreDynamicChild extends CoreDynamicBase {
            public string $child = "c";
            public function childMethod(): void {}
        }
        trait CoreDynamicTrait {
            public function traitMethod(): void {}
            protected function hiddenTraitMethod(): void {}
        }
        function core_dynamic_name(): string { return "CoreDynamicChild"; }
        function core_dynamic_object(): CoreDynamicBase { return new CoreDynamicChild(); }

        $name = core_dynamic_name();
        $vars = get_class_vars($name);
        $methods = get_class_methods(core_dynamic_object());
        $traitName = "CoreDynamicTrait";
        $traitMethods = get_class_methods($traitName);
        $traitVars = get_class_vars($traitName);
        echo $vars["base"], $vars["child"], ":",
             in_array("baseMethod", $methods) ? "B" : "b",
             in_array("childMethod", $methods) ? "C" : "c", ":",
             in_array("traitMethod", $traitMethods) ? "T" : "t",
             in_array("hiddenTraitMethod", $traitMethods) ? "H" : "h",
             count($traitVars);
        "#,
    );
    assert_eq!(out, "4c:BC:Th0");
}

/// Verifies unknown runtime class names throw catchable PHP-compatible TypeErrors.
#[test]
fn test_core_class_introspection_aot_dynamic_invalid_names() {
    let output = compile_and_run_capture(
        r#"<?php
        $missing = "CoreMissingClass";
        try {
            get_class_vars($missing);
        } catch (TypeError $error) {
            echo $error->getMessage(), "\n";
        }
        try {
            get_class_methods($missing);
        } catch (TypeError $error) {
            echo $error->getMessage();
        }
        "#,
    );
    assert_eq!(
        (output.success, output.stdout.as_str(), output.stderr.as_str()),
        (true, "get_class_vars(): Argument #1 ($class) must be a valid class name, CoreMissingClass given\n\
get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, string given", "")
    );
}

/// Verifies `get_class_vars()` uses AOT metadata through CUF, CUFA, and an FCC.
#[test]
fn test_core_get_class_vars_aot_callable_forms() {
    let out = compile_and_run(
        r#"<?php
        class CoreCallableVars {
            public int $plain = 7;
            public array $items = [2, 4];
            protected string $hidden = "no";
        }
        $fromCuf = call_user_func("get_class_vars", "CoreCallableVars");
        $fromCufa = call_user_func_array("get_class_vars", ["class" => "CoreCallableVars"]);
        $callback = get_class_vars(...);
        $fromFcc = $callback("CoreCallableVars");
        echo $fromCuf["plain"], $fromCuf["items"][1], ":",
             $fromCufa["plain"], $fromCufa["items"][0], ":",
             $fromFcc["plain"], $fromFcc["items"][1], ":",
             isset($fromFcc["hidden"]) ? "bad" : "visible";
        "#,
    );
    assert_eq!(out, "74:72:74:visible");
}

/// Verifies `get_class_methods()` callable forms preserve dynamic names and live results.
#[test]
fn test_core_get_class_methods_aot_callable_forms() {
    let out = compile_and_run_capture(
        r#"<?php
        class CoreCallableMethods {
            public function alpha(): void {}
            public static function beta(): void {}
            private function hidden(): void {}
        }
        $name = "CoreCallableMethods";
        $direct = get_class_methods("CoreCallableMethods");
        $fromLiteralCuf = call_user_func("get_class_methods", "CoreCallableMethods");
        $fromDynamicCuf = call_user_func("get_class_methods", $name);
        $callback = get_class_methods(...);
        $fromFcc = $callback($name);
        echo implode(",", $direct), ":", implode(",", $fromLiteralCuf), ":",
             implode(",", $fromDynamicCuf), ":", implode(",", $fromFcc), "\n";
        var_export([$direct, $fromLiteralCuf]);
        "#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "alpha,beta:alpha,beta:alpha,beta:alpha,beta\narray (\n  0 => \n  array (\n    0 => 'alpha',\n    1 => 'beta',\n  ),\n  1 => \n  array (\n    0 => 'alpha',\n    1 => 'beta',\n  ),\n)"
    );
    assert_eq!(out.stderr, "");
}
