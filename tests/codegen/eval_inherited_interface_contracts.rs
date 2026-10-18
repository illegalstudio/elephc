//! Purpose:
//! Checks inherited AOT method signatures when eval classes implement AOT interfaces.
//!
//! Called from:
//! - The codegen integration suite through `codegen::eval_inherited_interface_contracts`.
//!
//! Key details:
//! - Source is runtime-opaque so declaration checks execute through Magician.
//! - Rejection checks forbid reaching the statement after declaration and leaking a Rust panic.

use crate::support::*;

/// Asserts declaration acceptance, independently of PHP's fatal-error wording.
fn assert_declaration(source: &str, accepted: bool) {
    let out = compile_and_run_with_heap_debug(source);
    assert_eq!(out.success, accepted, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    if accepted {
        assert_eq!(out.stdout, "accepted\n");
    } else {
        assert!(!out.stdout.contains("accepted"), "{}", out.stdout);
        assert!(out.stderr.contains("eval() runtime failed"), "{}", out.stderr);
    }
    assert!(!out.stderr.contains("panicked at"), "{}", out.stderr);
}

/// Pins inherited AOT method compatibility for return-mismatch.
#[test]
fn test_eval_inherited_aot_return_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public function ping(): string {return "s";}}
interface Marker {public function ping(): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}

/// Pins inherited AOT method compatibility for arity-mismatch.
#[test]
fn test_eval_inherited_aot_arity_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public function ping(int $x): int {return $x;}}
interface Marker {public function ping(): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}

/// Pins inherited AOT method compatibility for by-ref-mismatch.
#[test]
fn test_eval_inherited_aot_by_ref_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public function ping(int $x): int {return $x;}}
interface Marker {public function ping(int &$x): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}

/// Pins inherited AOT method compatibility for parameter-type-mismatch.
#[test]
fn test_eval_inherited_aot_parameter_type_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public function ping(string $x): int {return 7;}}
interface Marker {public function ping(int $x): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}

/// Pins inherited AOT method compatibility for static-mismatch.
#[test]
fn test_eval_inherited_aot_static_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public function ping(): int {return 7;}}
interface Marker {public static function ping(): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}

/// Pins inherited AOT method compatibility for static-valid.
#[test]
fn test_eval_inherited_aot_static_valid() {
    assert_declaration(
        r#"<?php
class Service {public static function ping(): int {return 7;}}
interface Marker {public static function ping(): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        true,
    );
}

/// Pins inherited AOT method compatibility for by-ref-valid.
#[test]
fn test_eval_inherited_aot_by_ref_valid() {
    assert_declaration(
        r#"<?php
class Service {public function ping(int &$x): int {return $x;}}
interface Marker {public function ping(int &$x): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        true,
    );
}

/// Pins inherited AOT method compatibility for optional-valid.
#[test]
fn test_eval_inherited_aot_optional_valid() {
    assert_declaration(
        r#"<?php
class Service {public function ping(int $x=0): int {return $x;}}
interface Marker {public function ping(): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        true,
    );
}

/// Pins inherited AOT method compatibility for variadic-valid.
#[test]
fn test_eval_inherited_aot_variadic_valid() {
    assert_declaration(
        r#"<?php
class Service {public function ping(int ...$x): int {return 7;}}
interface Marker {public function ping(int $x): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        true,
    );
}

/// Pins inherited AOT method compatibility for contravariant-valid.
#[test]
fn test_eval_inherited_aot_contravariant_valid() {
    assert_declaration(
        r#"<?php
class Service {public function ping(mixed $x): int {return 7;}}
interface Marker {public function ping(int $x): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        true,
    );
}

/// Resolves an inherited self return in the native declaring class rather than the child.
#[test]
fn test_eval_inherited_aot_relative_return_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public function ping(): self {return $this;}}
interface Marker {public function ping(): self;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}

/// Pins inherited AOT method compatibility for missing-return-mismatch.
#[test]
fn test_eval_inherited_aot_missing_return_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public function ping() {return 7;}}
interface Marker {public function ping(): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}

/// Pins inherited AOT method compatibility for reverse-static-mismatch.
#[test]
fn test_eval_inherited_aot_reverse_static_mismatch() {
    assert_declaration(
        r#"<?php
class Service {public static function ping(): int {return 7;}}
interface Marker {public function ping(): int;}
$code='class Child extends Service implements Marker {}';
eval($code . str_repeat(" ",$argc));
echo "accepted\n";
"#,
        false,
    );
}
