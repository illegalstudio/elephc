//! Purpose:
//! Keep implicit nullable scalar return checks observable through optimizer call summaries.
//!
//! Called from:
//! - The optimizer group in the codegen integration harness.
//!
//! Key details:
//! - Discarded instance/static call results must still throw before scalar return coercion.

use crate::support::compile_and_run_with_heap_debug;

/// Discarded method calls retain both their return guard and the matching TypeError handler.
#[test]
fn test_nullable_return_coalesce_method_guards_remain_observable() {
    let output = compile_and_run_with_heap_debug(r#"<?php
class Boundaries {
    public function ints(?int $value): int { return $value ?? null; }
    public static function strings(?string $value): string { return $value ?? null; }
    public function floats(?float $value): float { return $value ?? null; }
    public static function bools(?bool $value): bool { return $value ?? null; }
}
function run_case(): void {
    $object = new Boundaries();
    try { $object->ints(null); } catch (TypeError $e) { echo "instance:int\n"; }
    try { Boundaries::strings(null); } catch (TypeError $e) { echo "static:string\n"; }
    try { $object->floats(null); } catch (TypeError $e) { echo "instance:float\n"; }
    try { Boundaries::bools(null); } catch (TypeError $e) { echo "static:bool\n"; }
}
run_case();
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "instance:int\nstatic:string\ninstance:float\nstatic:bool\n", "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
/// Discarded direct closures preserve implicit scalar TypeErrors and caught-exception cleanup.
#[test]
fn test_nullable_return_coalesce_direct_closure_guards_remain_observable() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function run_case(): void {
    try { (function (?int $value): int { return $value ?? null; })(null); } catch (TypeError $e) { echo "direct:int\n"; }
    try { (function (?string $value): string { return $value ?? null; })(null); } catch (TypeError $e) { echo "direct:string\n"; }
    try { (function (?float $value): float { return $value ?? null; })(null); } catch (TypeError $e) { echo "direct:float\n"; }
    try { (function (?bool $value): bool { return $value ?? null; })(null); } catch (TypeError $e) { echo "direct:bool\n"; }
}
run_case();
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "direct:int\ndirect:string\ndirect:float\ndirect:bool\n", "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Discarded alias closures preserve implicit scalar TypeErrors and caught-exception cleanup.
#[test]
fn test_nullable_return_coalesce_alias_closure_guards_remain_observable() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function run_case(): void {
    $call = (function (?int $value): int { return $value ?? null; });
    try { $call(null); } catch (TypeError $e) { echo "alias:int\n"; }
    $call = (function (?string $value): string { return $value ?? null; });
    try { $call(null); } catch (TypeError $e) { echo "alias:string\n"; }
    $call = (function (?float $value): float { return $value ?? null; });
    try { $call(null); } catch (TypeError $e) { echo "alias:float\n"; }
    $call = (function (?bool $value): bool { return $value ?? null; });
    try { $call(null); } catch (TypeError $e) { echo "alias:bool\n"; }
}
run_case();
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "alias:int\nalias:string\nalias:float\nalias:bool\n", "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Discarded arrow closures preserve implicit scalar TypeErrors and caught-exception cleanup.
#[test]
fn test_nullable_return_coalesce_arrow_closure_guards_remain_observable() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function run_case(): void {
    try { (fn(?int $value): int => $value ?? null)(null); } catch (TypeError $e) { echo "arrow:int\n"; }
    try { (fn(?string $value): string => $value ?? null)(null); } catch (TypeError $e) { echo "arrow:string\n"; }
    try { (fn(?float $value): float => $value ?? null)(null); } catch (TypeError $e) { echo "arrow:float\n"; }
    try { (fn(?bool $value): bool => $value ?? null)(null); } catch (TypeError $e) { echo "arrow:bool\n"; }
}
run_case();
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "arrow:int\narrow:string\narrow:float\narrow:bool\n", "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// First-class named calls retain the same implicit TypeError domain as direct calls.
#[test]
fn test_nullable_return_coalesce_first_class_function_guards_remain_observable() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function pick_int(?int $value): int { return $value ?? null; }
function pick_string(?string $value): string { return $value ?? null; }
function pick_float(?float $value): float { return $value ?? null; }
function pick_bool(?bool $value): bool { return $value ?? null; }
function safe(int $value): int { return $value + 1; }
function run_case(): void {
    try { (pick_int(...))(null); } catch (TypeError $e) { echo "fcc:int\n"; }
    try { (pick_string(...))(null); } catch (TypeError $e) { echo "fcc:string\n"; }
    try { (pick_float(...))(null); } catch (TypeError $e) { echo "fcc:float\n"; }
    try { (pick_bool(...))(null); } catch (TypeError $e) { echo "fcc:bool\n"; }
    $call = (pick_int(...));
    try { $call(null); } catch (TypeError $e) { echo "alias-fcc:int\n"; }
    $call = (pick_string(...));
    try { $call(null); } catch (TypeError $e) { echo "alias-fcc:string\n"; }
    $call = (pick_float(...));
    try { $call(null); } catch (TypeError $e) { echo "alias-fcc:float\n"; }
    $call = (pick_bool(...));
    try { $call(null); } catch (TypeError $e) { echo "alias-fcc:bool\n"; }
    echo (safe(...))(7), "\n";
}
run_case();
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "fcc:int\nfcc:string\nfcc:float\nfcc:bool\nalias-fcc:int\nalias-fcc:string\nalias-fcc:float\nalias-fcc:bool\n8\n", "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
