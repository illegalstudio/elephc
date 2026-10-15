//! Purpose:
//! Regressions for dynamic hops in delayed class-static property write receivers.
//!
//! Called from:
//! - The OOP codegen integration test module.
//!
//! Key details:
//! - RHS hooks run once and each intermediate null receiver throws a catchable Error.
//! - Computed names are captured before the RHS; variable names remain late-bound.

use crate::support::*;

/// Plain and compound dynamic-hop writes use the receiver installed by a single RHS hook.
#[test]
fn test_static_receiver_dynamic_review_hooks() {
    let out = compile_and_run(r#"<?php
class O { public int $v = 7; public ?O $child = null; }
class C { public static ?O $o = null; }
function make(int $v): O {
    $root = new O(); $root->child = new O();
    $root->child->child = new O(); $root->child->child->v = $v;
    return $root;
}
class Hook {
    public int $value { get { echo 'H'; C::$o = make(7); return 9; } }
}
$name = $argc > 0 ? 'child' : 'absent';
$hook = new Hook();
C::$o = make(1);
C::$o->$name->child->v = $hook->value;
echo ':', C::$o->child->child->v, '|';
C::$o = make(1);
C::$o->$name->child->v += $hook->value;
echo ':', C::$o->child->child->v;
"#);
    assert_eq!(out, "H:9|H:16");
}

/// A dynamic intermediate hop on null throws after the RHS rather than dropping the write.
#[test]
fn test_static_receiver_dynamic_review_null() {
    let out = compile_and_run(r#"<?php
class O { public int $v = 0; public ?O $child = null; }
class C { public static ?O $o = null; }
function rhs(): int { echo 'R'; return 9; }
$name = $argc > 0 ? 'child' : 'absent';
try { C::$o->$name->v = rhs(); echo 'bad'; }
catch (Error $error) { echo ':', $error->getMessage(); }
"#);
    assert_eq!(out, "R:Attempt to modify property \"child\" on null");
}

/// Computed names run once before the RHS and remain available to the null diagnostic.
#[test]
fn test_static_receiver_dynamic_review_computed_name() {
    let out = compile_and_run(r#"<?php
class O { public int $v = 0; public ?O $child = null; }
class C { public static ?O $o = null; }
function name(): string { echo 'N'; return str_repeat('child', 1); }
function rhs(): int { echo 'R'; return 9; }
$name = $argc > 0 ? 'child' : 'absent';
try { C::$o->{name()}->v = rhs(); echo 'bad'; }
catch (Error $error) { echo ':', $error->getMessage(); }
"#);
    assert_eq!(out, "NR:Attempt to modify property \"child\" on null");
}

/// Repeated caught null-hop errors release the captured RHS and name owners.
#[test]
fn test_static_receiver_dynamic_review_null_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class O { public mixed $v; public ?O $child = null; }
class C { public static ?O $o = null; }
function rhs(): string { return str_repeat('x', 32); }
function name(): string { return str_repeat('child', 1); }
for ($i = 0; $i < 16; $i++) {
    try { C::$o->{name()}->v = rhs(); echo 'bad'; }
    catch (Error $error) { echo 'E'; }
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "E".repeat(16), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Late-bound variables and boxed string names preserve receiver selection and cleanup.
#[test]
fn test_static_receiver_dynamic_review_name_timing() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class O { public int $v = 1; public ?O $child = null; public ?O $other = null; }
class C { public static ?O $o = null; }
function make(int $v): O {
    $root = new O(); $root->child = new O(); $root->other = new O();
    $root->child->v = $v; $root->other->v = $v;
    return $root;
}
function rhs(): int { global $name; echo 'R'; $name = 'other'; return 9; }
function value(): int { echo 'R'; return 9; }
function runtimeName(): mixed { echo 'B'; return str_repeat('child', 1); }
for ($i = 0; $i < 8; $i++) {
    C::$o = make(1); $name = 'child';
    C::$o->$name->v = rhs();
    echo ':', C::$o->child->v, ':', C::$o->other->v, '|';
    C::$o = make(1); $runtimeName = runtimeName();
    C::$o->$runtimeName->v = value();
    echo ':', C::$o->child->v, '|';
}
C::$o = null;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "R:1:9|BR:9|".repeat(8), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dynamic name timing and conversion owners are independent of EIR optimization.
#[test]
fn test_static_receiver_dynamic_review_name_timing_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_review_name_timing);
}

/// Unknown names retain a nullable object result even when every declared slot is non-null.
#[test]
fn test_static_receiver_dynamic_review_nonnull_slot() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Leaf { public int $v = 1; }
class Box { public Leaf $child; public function __construct() { $this->child = new Leaf(); } }
class Holder { public static ?Box $root = null; }
function rhs(): int { echo 'R'; return 9; }
function write(string $name): void { Holder::$root->$name->v = rhs(); }
Holder::$root = new Box();
write('child');
echo ':', Holder::$root->child->v, '|';
// Handle the lookup warning quietly to isolate the subsequent null write and its owners.
set_error_handler(function(int $level, string $message): bool { return true; });
try { write('missing'); echo 'bad'; }
catch (Error $error) { echo ':', $error->getMessage(); }
restore_error_handler();
Holder::$root = null;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "R:9|R:Attempt to assign property \"v\" on null", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime-name miss results and cleanup remain valid without EIR optimization.
#[test]
fn test_static_receiver_dynamic_review_nonnull_slot_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_review_nonnull_slot);
}

/// Dynamic receiver hook order is independent of EIR optimization.
#[test]
fn test_static_receiver_dynamic_review_hooks_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_review_hooks);
}

/// Dynamic null-hop diagnostics remain catchable without EIR optimization.
#[test]
fn test_static_receiver_dynamic_review_null_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_review_null);
}

/// Computed-name capture remains single-evaluation without EIR optimization.
#[test]
fn test_static_receiver_dynamic_review_computed_name_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_review_computed_name);
}

/// Null-hop name and RHS cleanup is balanced without EIR optimization.
#[test]
fn test_static_receiver_dynamic_review_null_owners_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_review_null_owners);
}

