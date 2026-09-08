//! Purpose:
//! Verifies owned by-value Mixed parameter cells across native and eval calls.
//!
//! Called from:
//! - The codegen integration harness through `runtime_gc`.
//!
//! Key details:
//! - Parameter returns must survive caller cleanup without leaking an extra owner.
//! - Array payloads preserve copy-on-write and resource aliases preserve shared identity.

use crate::support::*;

/// Native Mixed and untyped identity results outlive eval argument cells and preserve array COW.
#[test]
fn test_core_eval_native_mixed_parameter_results_own_their_cells() {
    let source = r#"<?php
class NativeOwnedMixed {
    public function identity(mixed $value): mixed { return $value; }
    public static function relay($value) { return $value; }
    public function append(mixed $value): mixed { $value[] = 9; return $value; }
}
$source = '$object = new NativeOwnedMixed();
$result = $object->identity("mixed-ok"); $reuse = "overwrite"; echo $result, "|";
$result = NativeOwnedMixed::relay("abc"); echo gettype($result), ":", $result, "|";
$result = $object->identity([]); echo gettype($result), ":", count($result), "|";
$original = [7]; $result = $object->append($original);
echo count($original), ":", $original[0], ":", count($result), ":", $result[1];' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "mixed-ok|string:abc|array:0|1:7:2:9");
}

/// AOT direct, first-class and method calls release caller temporaries independently of returns.
#[test]
fn test_core_aot_owned_mixed_parameters_cover_calls_and_cow() {
    let source = r#"<?php
function ownedIdentity(mixed $value): mixed { return $value; }
function ownedForward(mixed $value): mixed { return ownedIdentity($value); }
function ownedAppend(mixed $value): mixed { $value[] = 9; return $value; }
class OwnedMixedMethods {
    public function identity(mixed $value): mixed { return $value; }
    public static function forward(mixed $value): mixed { return ownedForward($value); }
}
$callback = ownedIdentity(...);
$object = new OwnedMixedMethods();
echo ownedIdentity("direct"), "|", $callback("callable"), "|";
echo $object->identity("method"), "|", OwnedMixedMethods::forward("static"), "|";
$original = [7]; $result = ownedAppend($original);
echo count($original), ":", $original[0], ":", count($result), ":", $result[1];
"#;
    assert_eq!(compile_and_run(source), "direct|callable|method|static|1:7:2:9");
}

/// Resource-valued parameter shadows retain shared identity instead of duplicating the handle.
#[test]
fn test_core_owned_mixed_parameter_resource_returns_keep_alias_identity() {
    let source = r#"<?php
function ownedResource(mixed $value): mixed { return $value; }
class OwnedResourceMethod {
    public function identity(mixed $value): mixed { return $value; }
}
$original = fopen("php://temp", "w+");
$alias = ownedResource($original);
echo get_resource_id($original) == get_resource_id($alias) ? "same" : "bad";
fclose($alias); echo ":", get_resource_type($original), "|";
$source = '$object = new OwnedResourceMethod(); $original = fopen("php://temp", "w+");
$alias = $object->identity($original);
echo get_resource_id($original) == get_resource_id($alias) ? "same" : "bad";
fclose($alias); echo ":", get_resource_type($original);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "same:Unknown|same:Unknown");
}

/// Repeated native Mixed calls release both discarded parameters and returned cell owners.
#[test]
fn test_core_aot_owned_mixed_parameters_release_each_activation() {
    assert_mixed_parameter_cleanup(false);
}

/// Repeated eval-to-native calls do not leak source cells, shadow cells, or returned arrays.
#[test]
fn test_core_eval_owned_mixed_parameters_release_each_activation() {
    assert_mixed_parameter_cleanup(true);
}

/// Compares live allocations with fixed declarations and no allocating loop-control expressions.
fn assert_mixed_parameter_cleanup(in_eval: bool) {
    let native = r#"
function ownedMixedGcIdentity(mixed $value): mixed { return $value; }
function ownedMixedGcForward(mixed $value): mixed {
    $result = ownedMixedGcIdentity($value);
    return $result;
}
class OwnedMixedGc {
    public function identity(mixed $value): mixed { return $value; }
    public function discard(mixed $value): void {}
    public static function relay(mixed $value): mixed { return $value; }
    public function nested(mixed $value): mixed { return ownedMixedGcForward($value); }
}
"#;
    let body = r#"
$result = $object->identity("string"); unset($result);
$result = $object->identity([1, [2, 3]]); unset($result);
$result = OwnedMixedGc::relay(42); unset($result);
$result = $object->nested([4, 5]); unset($result);
$object->discard("discarded");
"#;
    let live = |iterations| {
        let body = body.repeat(iterations);
        let source = if in_eval {
            format!(r#"<?php {native}
$source = '$object = new OwnedMixedGc(); {body} unset($object); return 42;' . ' // ' . $argc;
echo eval($source);
"#)
        } else {
            format!("<?php {native} $object = new OwnedMixedGc(); {body} unset($object); echo 42;")
        };
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42", "{}", output.stderr);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "Mixed activation owners leaked, eval={in_eval}");
}
