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

/// Checks temporary key and RHS owners across a delayed array-property receiver getter.
fn verify_array_operands_throwing_getter(operation: &str) {
    let operation = operation.replace("$argc + 7", "$width").replace("$argc", "$width");
    let source = r#"<?php
class SlotBox { public array $items = [1]; }
class ParentBox { public SlotBox $child { get { throw new Exception('getter'); } } }
class Holder { public static ?ParentBox $value = null; }
function run(int $width, mixed $payload): void {
Holder::$value = new ParentBox();
for ($i = 0; $i < 16; $i++) {
    try { UPDATE }
    catch (Exception $error) { echo 'caught|'; unset($error); }
}
Holder::$value = null;
}
run($argc + 7, 'payload');
"#.replace("UPDATE", &operation);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|".repeat(16), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Statement array writes retain their fresh RHS until delayed traversal completes.
#[test]
fn test_static_receiver_grok_array_rhs_throwing_getter() {
    verify_array_operands_throwing_getter("Holder::$value->child->items[0] = str_repeat('n', $argc + 7);");
    verify_array_operands_throwing_getter("Holder::$value->child->items[] = str_repeat('n', $argc + 7);");
}

/// Array RHS traversal guards are independent of EIR optimization.
#[test]
fn test_static_receiver_grok_array_rhs_throwing_getter_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_array_rhs_throwing_getter);
}

/// Replayable mixed casts remain guarded even when no PHP RHS capture is synthesized.
#[test]
fn test_static_receiver_grok_array_cast_rhs_throwing_getter() {
    verify_array_operands_throwing_getter("Holder::$value->child->items[0] = (string)$payload;");
    verify_array_operands_throwing_getter("Holder::$value->child->items[] = (string)$payload;");
}

/// Replayable RHS cast guards are independent of EIR optimization.
#[test]
fn test_static_receiver_grok_array_cast_rhs_throwing_getter_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_array_cast_rhs_throwing_getter);
}

/// An independently allocated array key stays unwind-visible through receiver getters.
#[test]
fn test_static_receiver_grok_array_key_throwing_getter() {
    verify_array_operands_throwing_getter("Holder::$value->child->items[str_repeat('k', $argc)] = 1;");
}

/// Array key traversal guards are independent of EIR optimization.
#[test]
fn test_static_receiver_grok_array_key_throwing_getter_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_array_key_throwing_getter);
}

/// Ordinary instance writes also guard a coerced name while a replayable RHS cast throws.
#[test]
fn test_static_receiver_grok_instance_name_throwing_cast() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Text { public function __toString(): string { throw new Exception('cast'); } }
class Box { public string $slot = 'old'; }
function write(mixed $name): void {
    $box = new Box(); $text = new Text();
    try { $box->{$name} = (string)$text; }
    catch (Exception $error) { echo 'caught|'; unset($error); }
    unset($box, $text);
}
for ($i = 0; $i < 16; $i++) { write('slot'); }
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "caught|".repeat(16), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Instance-name RHS guards are independent of EIR optimization.
#[test]
fn test_static_receiver_grok_instance_name_throwing_cast_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_instance_name_throwing_cast);
}

/// A mixed selector stays unwind-visible while a delayed receiver getter throws.
#[test]
fn test_static_receiver_greptile_mixed_name_throwing_getter() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Leaf { public int $value = 0; }
class Root { public Leaf $child { get { throw new Error('getter'); } } }
class Holder { public static ?Root $root = null; }
function write(mixed $name): void {
    try { Holder::$root->child->{$name} = 1; }
    catch (Error $error) { echo $error->getMessage(), '|'; unset($error); }
}
Holder::$root = new Root();
for ($i = 0; $i < 16; $i++) { write(str_repeat('value', 1)); }
Holder::$root = null;
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "getter|".repeat(16), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Getter unwind roots have the same lifetime without EIR optimization.
#[test]
fn test_static_receiver_greptile_mixed_name_throwing_getter_no_ir_opt() {
    without_ir_opt(test_static_receiver_greptile_mixed_name_throwing_getter);
}

/// An already evaluated object RHS is retired before a receiver getter's same-frame catch.
#[test]
fn test_static_receiver_greptile_owned_rhs_throwing_getter() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Payload { public function __destruct() { echo 'D'; } }
class Leaf { public mixed $value; }
class Root { public Leaf $child { get { echo 'H'; throw new Error('getter'); } } }
class Holder { public static ?Root $root = null; }
function value(): Payload { echo 'R'; return new Payload(); }
Holder::$root = new Root();
for ($i = 0; $i < 8; $i++) {
    try { Holder::$root->child->value = value(); }
    catch (Error $error) { echo 'caught|'; unset($error); }
}
Holder::$root = null;
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "RHDcaught|".repeat(8), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Object RHS cleanup across a throwing getter is independent of EIR optimization.
#[test]
fn test_static_receiver_greptile_owned_rhs_throwing_getter_no_ir_opt() {
    without_ir_opt(test_static_receiver_greptile_owned_rhs_throwing_getter);
}

/// Normal dynamic writes keep one string owner and preserve the target selected by a getter.
#[test]
fn test_static_receiver_greptile_mixed_name_present_getter() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Leaf { public int $value = 0; }
class Root {
    public Leaf $stored;
    public Leaf $child { get { return $this->stored; } }
    public function __construct() { $this->stored = new Leaf(); }
}
class Holder { public static ?Root $root = null; }
function write(mixed $name): void { Holder::$root->child->{$name} = 1; }
Holder::$root = new Root();
for ($i = 0; $i < 16; $i++) { write(str_repeat('value', 1)); }
echo Holder::$root->stored->value;
Holder::$root = null;
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Normal getter/name cleanup does not depend on EIR optimization.
#[test]
fn test_static_receiver_greptile_mixed_name_present_getter_no_ir_opt() {
    without_ir_opt(test_static_receiver_greptile_mixed_name_present_getter);
}

/// Repeats a null compound write whose mixed property selector requires an owned string cast.
fn verify_nullable_mixed_name(write: &str, property: &str, verb: &str) {
    let source = format!(r#"<?php
class Bag {{ public array $items = [1]; public int $number = 1; }}
class Holder {{ public static ?Bag $bag = null; }}
function write(mixed $name): void {{
    try {{ {write} }}
    catch (Error $error) {{ echo $error->getMessage(), '|'; unset($error); }}
}}
for ($i = 0; $i < 8; $i++) {{ write('{property}'); write(str_repeat('{property}', 1)); }}
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout,
        format!("Attempt to {verb} property \"{property}\" on null|").repeat(16),
        "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A nullable dynamic array compound update retires its coerced property name once.
#[test]
fn test_static_receiver_grok_nullable_mixed_name_array() {
    verify_nullable_mixed_name("Holder::$bag->{$name}[0] += 1;", "items", "modify");
}

/// A nullable dynamic scalar compound update retires its coerced property name once.
#[test]
fn test_static_receiver_grok_nullable_mixed_name_scalar() {
    verify_nullable_mixed_name("Holder::$bag->{$name} += 1;", "number", "assign");
}

/// Nullable dynamic array unwind ownership is independent of EIR optimization.
#[test]
fn test_static_receiver_grok_nullable_mixed_name_array_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_nullable_mixed_name_array);
}

/// Nullable dynamic scalar unwind ownership is independent of EIR optimization.
#[test]
fn test_static_receiver_grok_nullable_mixed_name_scalar_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_nullable_mixed_name_scalar);
}

/// Executes a direct dynamic-property array write with a name that replaces its static owner.
fn verify_direct_dynamic_array_write(write: &str, expected: &str) {
    let source = format!(r#"<?php
class Bag {{ public array $items = [1]; }}
class C {{ public static ?Bag $o = null; }}
function name(): string {{ echo 'N'; C::$o = new Bag(); C::$o->items = [7]; return str_repeat('items', 1); }}
function rhs(): int {{ echo 'R', json_encode(C::$o->items); return 9; }}
C::$o = new Bag();
{write}
echo json_encode(C::$o->items);
C::$o = null;
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A computed direct array-base name selects its replacement before the compound read.
#[test]
fn test_static_receiver_grok_direct_dynamic_array_compound() {
    verify_direct_dynamic_array_write("C::$o->{name()}[0] += 1;", "N[8]");
}

/// Computed names run before RHS side effects even for a one-dimensional plain write.
#[test]
fn test_static_receiver_grok_direct_dynamic_array_plain() {
    verify_direct_dynamic_array_write("C::$o->{name()}[0] = rhs(); echo ':';", "NR[7]:[9]");
}

/// Direct dynamic array compound writes preserve order without EIR optimization.
#[test]
fn test_static_receiver_grok_direct_dynamic_array_compound_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_direct_dynamic_array_compound);
}

/// Direct dynamic array plain writes preserve order without EIR optimization.
#[test]
fn test_static_receiver_grok_direct_dynamic_array_plain_no_ir_opt() {
    without_ir_opt(test_static_receiver_grok_direct_dynamic_array_plain);
}

/// Dynamic property array updates diagnose each float key once, like literal properties.
#[test]
fn test_static_receiver_dynamic_float_key_warns_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public array $items = [1, 2]; public array $nested = [[1, 2], [3, 4]]; }
class C { public static ?Bag $o = null; }
function name(): string { return 'items'; }
function nestedName(): string { return 'nested'; }
set_error_handler(function(int $level, string $message): bool {
    if ($level === E_DEPRECATED) { echo 'W'; }
    return true;
});
C::$o = new Bag(); C::$o->{name()}[1.5] += 1; echo json_encode(C::$o->items), '|';
C::$o = new Bag(); $name = 'items'; C::$o->$name[1.5] += 1; echo json_encode(C::$o->items), '|';
C::$o = new Bag(); echo C::$o->{name()}[1.5]++, ':', json_encode(C::$o->items), '|';
C::$o = new Bag(); echo ++C::$o->{name()}[1.5], ':', json_encode(C::$o->items), '|';
C::$o = new Bag(); C::$o->{nestedName()}[1.5][0] += 1; echo json_encode(C::$o->nested);
restore_error_handler(); C::$o = null;
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "W[1,3]|W[1,3]|W2:[1,3]|W3:[1,3]|W[[1,2],[4,4]]",
        "{}",
        out.stderr,
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dynamic float-key diagnosis remains single-shot without EIR optimization.
#[test]
fn test_static_receiver_dynamic_float_key_warns_once_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_float_key_warns_once);
}

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

/// Nested appends through a runtime-name hop select the static receiver after the RHS.
#[test]
fn test_static_receiver_dynamic_nested_append_after_rhs() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Bag { public array $items = [[1]]; }
class C { public static ?Bag $o = null; }
function make(array $items): Bag {
    $bag = new Bag(); $bag->items = $items; return $bag;
}
function name(): string { echo 'N'; return 'items'; }
class Hook {
    public int $value { get { echo 'H'; C::$o = make([[7]]); return 9; } }
}
$hook = new Hook();
C::$o = make([[1]]);
C::$o->{name()}[0][] = $hook->value;
echo json_encode(C::$o->items);
C::$o = null;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "NH[[7,9]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A final runtime-name compound target fetches its effectful receiver only once.
#[test]
fn test_static_receiver_dynamic_final_compound_receiver_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Leaf { public int $v = 7; }
class Root {
    private Leaf $stored;
    public function __construct() { $this->stored = new Leaf(); }
    public Leaf $child { get { echo 'G'; return $this->stored; } }
}
class C { public static ?Root $o = null; }
function rhs(): int { echo 'R'; return 2; }
$name = $argc > 0 ? 'v' : 'missing';
C::$o = new Root();
C::$o->child->$name += rhs();
echo ':', C::$o->child->v;
C::$o = null;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "RG:G9", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
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

/// Dynamic-hop nested append timing is independent of EIR optimization.
#[test]
fn test_static_receiver_dynamic_nested_append_after_rhs_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_nested_append_after_rhs);
}

/// Final dynamic compound receiver reuse is independent of EIR optimization.
#[test]
fn test_static_receiver_dynamic_final_compound_receiver_once_no_ir_opt() {
    without_ir_opt(test_static_receiver_dynamic_final_compound_receiver_once);
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
