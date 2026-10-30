//! Purpose:
//! End-to-end tests for the compiled `GMP` object projection and compiler storage visibility.
//!
//! Called from:
//! - `cargo test --test codegen_tests gmp_class` through the codegen test module.
//!
//! Key details:
//! - `__elephc_num` is compiler storage, not a PHP property. It must stay out of
//!   `get_object_vars()`, `(array)` casts, JSON and the serialized property table while the
//!   class's own operators and debug projection keep reading it.
//! - Expected output is php-src 8.5 output; no object handles appear in the fixture.

use crate::support::*;

/// Verifies the compiler storage slot does not leak into PHP's property projections.
#[test]
fn test_gmp_storage_property_is_not_php_visible() {
    if crate::support::skip_without_gmp_native("test_gmp_storage_property_is_not_php_visible") {
        return;
    }
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$g = new GMP("42");
var_dump(get_object_vars($g));
var_dump((array) $g);
var_dump(json_encode($g));
var_dump(serialize($g));
echo gmp_strval($g), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(0) {\n}\n\
array(0) {\n}\n\
string(2) \"{}\"\n\
string(27) \"O:3:\"GMP\":1:{i:0;s:2:\"2a\";}\"\n\
42\n"
    );
}

/// Verifies PHP's three-method GMP surface and its php-src declaration order.
///
/// `method_exists()`, `get_class_methods()`, `ReflectionClass::hasMethod()`, `getMethod()`
/// and `getMethods()` must hide the compiler's operator/cast adapters, while the class's own
/// lowering keeps dispatching them internally.
#[test]
fn test_gmp_visible_method_surface_matches_php() {
    if crate::support::skip_without_gmp_native("test_gmp_visible_method_surface_matches_php") {
        return;
    }
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$r = new ReflectionClass("GMP");
var_dump($r->hasMethod("__construct"), $r->hasMethod("__serialize"), $r->hasMethod("__unserialize"));
var_dump($r->hasMethod("__toString"), $r->hasMethod("__elephc_add"), $r->hasMethod("serialize"));
foreach ($r->getMethods() as $m) { echo $m->getName(), ","; }
echo "\n";
$g = new GMP("42");
var_dump(method_exists($g, "__construct"), method_exists($g, "__toString"), method_exists($g, "__elephc_add"));
$methods = get_class_methods($g);
sort($methods);
echo implode(",", $methods), "\n";
var_dump(property_exists($g, "num"), property_exists($g, "__elephc_num"));
try { $r->getMethod("__elephc_add"); } catch (ReflectionException $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(true)\nbool(true)\n\
bool(false)\nbool(false)\nbool(false)\n\
__construct,__serialize,__unserialize,\n\
bool(true)\nbool(false)\nbool(false)\n\
__construct,__serialize,__unserialize\n\
bool(false)\nbool(false)\n\
Method GMP::__elephc_add() does not exist\n"
    );
}

/// Dispatches mixed arithmetic to the matching adapter when BcMath\Number and GMP coexist.
///
/// The operator rewrite used to select a single class by HashMap iteration order, so a
/// `mixed` operand holding GMP could fall through to the scalar path and lose the object.
#[test]
fn test_gmp_dynamic_operator_dispatch_with_bcmath() {
    if crate::support::skip_without_gmp_native("test_gmp_dynamic_operator_dispatch_with_bcmath") {
        return;
    }
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
function addmix(mixed $a, mixed $b): mixed { return $a + $b; }
$n = new BcMath\Number("5");
$g = gmp_init(3);
echo addmix($n, 4)->value, "|", gmp_strval(addmix($g, 2)), "|", gmp_strval(addmix(1, $g)), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "9|5|4\n");
}

/// Verifies GMP's object-versus-null ordering and its PHP 8.5 serialization surface.
///
/// `GMP` is final but not `Serializable`; `serialize()` and `unserialize()` are undefined
/// methods. The legacy `C:3:"GMP":...` payload is still accepted by the runtime without
/// exposing those methods to PHP.
#[test]
fn test_gmp_null_ordering_and_legacy_serialization_surface() {
    if crate::support::skip_without_gmp_native("test_gmp_null_ordering_and_legacy_serialization_surface") {
        return;
    }
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$g = gmp_init(5);
var_dump($g > null, $g >= null, $g < null, $g <= null);
var_dump(null > $g, null >= $g, null < $g, null <= $g);
var_dump($g <=> null, null <=> $g);
$r = new ReflectionClass("GMP");
var_dump($r->implementsInterface("Serializable"));
var_dump(method_exists($g, "serialize"), method_exists($g, "unserialize"));
try { $g->serialize(); } catch (Throwable $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo gmp_strval(unserialize('C:3:"GMP":15:{s:2:"42";a:0:{}}')), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\n\
bool(false)\nbool(false)\nbool(true)\nbool(true)\n\
int(1)\nint(-1)\n\
bool(false)\n\
bool(false)\nbool(false)\n\
Error: Call to undefined method GMP::serialize()\n\
42\n"
    );
}

/// Verifies reading GMP's virtual `num` name warns when no dynamic property exists.
#[test]
fn test_gmp_virtual_num_read_reports_undefined_property() {
    if crate::support::skip_without_gmp_native("test_gmp_virtual_num_read_reports_undefined_property") {
        return;
    }
    let out = compile_and_run_capture_merged_with_gmp(
        r#"<?php
$g = gmp_init(5);
var_dump($g->num);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Warning: Undefined property: GMP::$num"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("NULL"), "{}", out.stdout);
}
