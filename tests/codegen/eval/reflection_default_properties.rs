//! Purpose:
//! Regression tests for compiled `getDefaultProperties()` on eval-built class reflectors.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval::reflection_default_properties` through Rust's
//!   test harness.
//!
//! Key details:
//! - The compiled getter reads the `__default_properties` slot, which eval never filled, so
//!   every eval-built `ReflectionClass` answered `NULL`. Eval now stores the map (or the
//!   retained default Error the getter throws) when it builds a class reflector.
//! - A property default using `self::CONST` is evaluated in the declaring class's scope, as a
//!   parameter default already was; before, reflection could not evaluate it at all.
//! - Defaults are printed as sorted `name=value` pairs: elephc lists static defaults after
//!   instance ones, php before them, and the order is not what this test is about.
//! - Fixtures run under heap debug inside a function. Expected output is php 8.5's.

use crate::support::*;

/// `getDefaultProperties()` on class and object reflectors built inside eval, for a compiled
/// class and an eval-declared one, lists static and instance defaults like php, including
/// through a member's `getDeclaringClass()`, and throws the retained default Error. The enum
/// reflector's (empty) answer is covered by `reflection_enum_declaring_class`, whose case
/// singletons would keep this fixture's heap from being clean.
#[test]
fn test_eval_built_reflector_default_properties_read_from_compiled_code() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class AotDefaults { public int $value = 7; public static $shared = "st"; public ?string $nullable = null; public $untyped; public int $typedNoDefault; }
class DeferredDefault { const ENABLED = false; public array $items = [self::ENABLED ? 'ok' : parent::A]; }
function sortedDefaults(ReflectionClass $c): string { $pairs = []; foreach ($c->getDefaultProperties() as $name => $value) { $pairs[] = $name . "=" . json_encode($value); } sort($pairs); return implode(",", $pairs); }
function probe(): void {
    $c = eval('return new ReflectionClass("AotDefaults");');
    echo sortedDefaults($c), "\n";
    $p = eval('return new ReflectionProperty("AotDefaults", "value");');
    echo sortedDefaults($p->getDeclaringClass()), "\n";
    $e = eval('class EvalDefaults { const M = 3; public int $a = 5; public $b; public array $c = [1, 2]; public static string $s = "x"; public string $noDefault; public int $fromConst = self::M; } return new ReflectionClass("EvalDefaults");');
    echo sortedDefaults($e), "\n";
    eval('echo (new ReflectionProperty("EvalDefaults", "fromConst"))->getDefaultValue(), "|", json_encode((new ReflectionClass("EvalDefaults"))->getDefaultProperties()["fromConst"]), "\n";');
    $o = eval('class EvalObjectDefaults { public int $a = 5; public $b; public static string $s = "x"; } return new ReflectionObject(new EvalObjectDefaults);');
    echo sortedDefaults($o), "\n";
    $d = eval('return new ReflectionClass("DeferredDefault");');
    try { $d->getDefaultProperties(); } catch (Error $error) { echo get_class($error), ":", $error->getMessage(), "\n"; }
}
probe();
"#,
    );
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "nullable=null,shared=\"st\",untyped=null,value=7\n",
            "nullable=null,shared=\"st\",untyped=null,value=7\n",
            "a=5,b=null,c=[1,2],fromConst=3,s=\"x\"\n",
            "3|3\n",
            "a=5,b=null,s=\"x\"\n",
            "Error:Cannot access \"parent\" when current class scope has no parent\n",
        )
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
