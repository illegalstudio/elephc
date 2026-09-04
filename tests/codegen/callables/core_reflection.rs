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
