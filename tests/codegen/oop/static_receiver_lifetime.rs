//! Purpose:
//! Regressions for instance writes through an owning static object slot.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - No local alias masks early destruction when RHS evaluation replaces the static owner.
//! - Nullable array receivers must mutate the declared slot and retire unwind pins.

use crate::support::compile_and_run_with_heap_debug;

/// Checks output and balanced lifetime state for one static-receiver write fixture.
fn verify(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Computed keys run before the RHS, but nested writes fetch its replacement static array.
#[test]
fn test_static_receiver_second_review_computed_nested_keys() {
    verify(r#"<?php
class C { public static array $items = [[1]]; }
function pickKey(): int { echo 'K'; return 0; }
function replace(): int { echo 'R'; C::$items = [[7]]; return 9; }
C::$items[pickKey()][0] = replace(); echo json_encode(C::$items), '|';
C::$items[0][pickKey()] = replace(); echo json_encode(C::$items), '|';
C::$items[0.0][0] = replace(); echo json_encode(C::$items), '|';
C::$items[strval(0)][0] = replace(); echo json_encode(C::$items);
"#, "KR[[9]]|KR[[9]]|R[[9]]|R[[9]]");
}

/// Nested compound writes read the replacement element after the RHS, with keys evaluated once.
#[test]
fn test_static_receiver_second_review_nested_compound_order() {
    verify(r#"<?php
class C { public static array $items = [[1]]; }
function pickKey(): int { echo 'K'; return 0; }
function replace(): int { echo 'R'; C::$items = [[7]]; return 9; }
C::$items[0][0] += replace(); echo json_encode(C::$items), '|';
C::$items[pickKey()][0] += replace(); echo json_encode(C::$items), '|';
C::$items[0][pickKey()] += replace(); echo json_encode(C::$items);
"#, "R[[16]]|KR[[16]]|KR[[16]]");
}

/// Fractional static parent keys are diagnosed after the RHS and once per compound access.
#[test]
fn test_static_receiver_second_review_nested_float_diagnosis() {
    verify(r#"<?php
class C { public static array $items = [[1], [2]]; }
function rhs(): int { echo 'R'; C::$items = [[7], [8]]; return 9; }
set_error_handler(function($level, $message) { echo 'W'; return true; });
C::$items[1.5][0] = rhs(); echo json_encode(C::$items), '|';
C::$items[1.5][0] += rhs(); echo json_encode(C::$items);
restore_error_handler();
"#, "RW[[7],[9]]|RW[[7],[17]]");
}

/// Null write chains fail at their first missing property without emitting read warnings.
#[test]
fn test_static_receiver_second_review_null_property_chain() {
    verify(r#"<?php
class Leaf { public int $v = 1; public string $text = ''; }
class O { public Leaf $child; }
class C { public static ?O $o = null; }
try { C::$o->child->v = (print 'rhs'); }
catch (Error $e) { echo ':', $e->getMessage(), '|'; }
try { C::$o->child->v += (print 'rhs'); }
catch (Error $e) { echo ':', $e->getMessage(), '|'; }
for ($i = 0; $i < 4; $i++) {
    try { C::$o->child->text = str_repeat('x', 24); }
    catch (Error $e) { echo 'c'; }
}
"#, "rhs:Attempt to modify property \"child\" on null|rhs:Attempt to modify property \"child\" on null|cccc");
}

/// A present chain retains the child lease and calls its get hook only once for a compound read.
#[test]
fn test_static_receiver_second_review_present_property_chain() {
    verify(r#"<?php
class Leaf { public int $v = 1; public function __destruct() { echo 'D', $this->v; } }
class O {
    public Leaf $storage;
    public Leaf $child { get { echo 'G'; return $this->storage; } }
    public function __construct() { $this->storage = new Leaf(); }
}
class C { public static ?O $o = null; }
function rhs(): int { echo 'R'; return 9; }
C::$o = new O();
C::$o->child->v += rhs();
echo ':', C::$o->storage->v;
"#, "RG:10D10");
}

/// Nullable children are checked as write-context parents without reading their later property.
#[test]
fn test_static_receiver_second_review_nullable_child() {
    verify(r#"<?php
class Leaf { public int $v = 1; }
class O { public ?Leaf $child = null; }
class C { public static O $o; }
C::$o = new O();
try { C::$o->child->v = (print 'rhs'); }
catch (Error $e) { echo ':', $e->getMessage(); }
"#, "rhs:Attempt to assign property \"v\" on null");
}

/// Captured owned String keys are retired when the RHS throws before any static traversal.
#[test]
fn test_static_receiver_second_review_throwing_nested_rhs() {
    verify(r#"<?php
class C { public static array $items = [[1]]; }
function fail(): int { throw new Error('rhs'); }
for ($i = 0; $i < 4; $i++) {
    try { C::$items[strval(0)][0] = fail(); }
    catch (Error $e) { echo 'a'; }
    try { C::$items[strval(0)][0] += fail(); }
    catch (Error $e) { echo 'b'; }
}
echo '|', json_encode(C::$items);
"#, "abababab|[[1]]");
}

/// Nullable local array writes evaluate computed keys and values before the null Error.
#[test]
fn test_static_receiver_followup_local_null_array_order() {
    verify(r#"<?php
class O { public array $items = []; }
function append(?O $object): void { $object->items[] = (print 'rhs'); }
function indexed(?O $object): void { $object->items[(print 'i')] = (print 'v'); }
try { append(null); } catch (Error $error) { echo ':', $error->getMessage(), '|'; }
try { indexed(null); } catch (Error $error) { echo ':', $error->getMessage(); }
"#, "rhs:Attempt to modify property \"items\" on null|iv:Attempt to modify property \"items\" on null");
}

/// Owned keys and values are retired when a nullable local array write is refused.
#[test]
fn test_static_receiver_followup_local_null_array_owners() {
    verify(r#"<?php
class O { public array $items = []; }
function write(?O $object): void { $object->items[str_repeat('k', 8)] = str_repeat('v', 8); }
for ($i = 0; $i < 8; $i++) {
    try { write(null); } catch (Error $error) { echo 'c'; }
}
"#, "cccccccc");
}

/// Both static and local runtime-name writes throw after evaluating the RHS.
#[test]
fn test_static_receiver_followup_dynamic_null_error() {
    verify(r#"<?php
class O { public int $v = 1; }
class C { public static ?O $o = null; }
function write(?O $object, string $name): void { $object->$name = (print 'rhs'); echo 'bad'; }
$name = 'v';
try { C::$o->$name = (print 'rhs'); echo 'bad'; }
catch (Error $error) { echo ':', $error->getMessage(), '|'; }
try { write(null, $name); } catch (Error $error) { echo ':', $error->getMessage(); }
"#, "rhs:Attempt to assign property \"v\" on null|rhs:Attempt to assign property \"v\" on null");
}

/// Dynamic-null guards unwind computed property names and fresh string values.
#[test]
fn test_static_receiver_followup_dynamic_null_owners() {
    verify(r#"<?php
class O { public string $value = ''; }
class C { public static ?O $o = null; }
function write(?O $object): void { $object->{str_repeat('v', 8)} = str_repeat('x', 16); }
for ($i = 0; $i < 8; $i++) {
    try { C::$o->{str_repeat('v', 8)} = str_repeat('x', 16); } catch (Error $error) { echo 's'; }
    try { write(null); } catch (Error $error) { echo 'l'; }
}
"#, "slslslslslslslsl");
}

/// A coalescing write releases the independent nullable static receiver acquire.
#[test]
fn test_static_receiver_followup_coalesce_nullable_pin() {
    verify(r#"<?php
class O { public mixed $v = null; public function __destruct() { echo 'D', $this->v ?? 0; } }
class C { public static ?O $o = null; }
function replace(): int { echo 'R'; C::$o = new O(); return 9; }
C::$o = new O();
C::$o->v ??= replace();
echo 'X', C::$o->v;
echo '|S', C::$o->v, '|END';
"#, "RD0X9|S9|ENDD9");
}

/// Tagged scalar property writes preserve the nullable receiver's final destructor as well.
#[test]
fn test_static_receiver_followup_coalesce_tagged_scalar_pin() {
    verify(r#"<?php
class O { public ?int $v = null; public function __destruct() { echo 'D'; } }
class C { public static ?O $o = null; }
function replace(): int { echo 'R'; C::$o = new O(); return 9; }
C::$o = new O();
C::$o->v ??= replace();
echo 'X', C::$o->v, '|END';
"#, "RDX9|ENDD");
}

/// A plain static array append fetches the slot only after an RHS replaces it.
#[test]
fn test_static_receiver_followup_direct_static_append() {
    verify(r#"<?php
class C { public static array $items = [1]; }
function replace(): int { echo 'R'; C::$items = [7]; return 9; }
C::$items[] = replace();
echo json_encode(C::$items);
"#, "R[7,9]");
}

/// An indirect array append reads the replacement receiver only after its RHS has run.
#[test]
fn test_static_receiver_review_nested_append_replacement() {
    verify(r#"<?php
class O { public array $items = [[1]]; public function __destruct() { echo 'D', json_encode($this->items); } }
class C { public static O $o; }
function replace(): int { C::$o = new O(); C::$o->items = [[7]]; return 9; }
C::$o = new O();
C::$o->items[0][] = replace();
echo 'S', json_encode(C::$o->items);
"#, "D[[1]]S[[7,9]]D[[7,9]]");
}

/// Computed dimensions precede the RHS while the static receiver itself remains delayed.
#[test]
fn test_static_receiver_review_nested_append_key_order() {
    verify(r#"<?php
class O { public array $items = [[1]]; }
class C { public static O $o; }
function pickKey(): int { echo 'key:'; return 0; }
function replace(): int { echo 'rhs:'; C::$o = new O(); C::$o->items = [[7]]; return 9; }
C::$o = new O();
C::$o->items[pickKey()][] = replace();
echo json_encode(C::$o->items);
"#, "key:rhs:[[7,9]]");
}

/// A child-property append traverses the replacement's entire object chain after its RHS.
#[test]
fn test_static_receiver_review_child_append_replacement() {
    verify(r#"<?php
class Leaf {
    public array $items = [1];
    public function __construct(public int $id) {}
    public function __destruct() { echo 'D', $this->id, json_encode($this->items); }
}
class Root {
    public Leaf $child;
    public function __construct(public int $id, int $childId) { $this->child = new Leaf($childId); }
    public function __destruct() { echo 'D', $this->id; }
}
class C { public static Root $o; }
function replace(): int { C::$o = new Root(3, 7); C::$o->child->items = [1, 7]; return 9; }
C::$o = new Root(1, 2);
C::$o->child->items[] = replace();
echo 'S', C::$o->id, ':', json_encode(C::$o->child->items), ':', C::$o->child->id;
"#, "D1D2[1]S3:[1,7,9]:7D3D7[1,7,9]");
}

/// A direct write on null throws a catchable Error after evaluating its RHS.
#[test]
fn test_static_receiver_review_direct_null_error() {
    verify(r#"<?php
class O { public int $v = 1; }
class C { public static ?O $o = null; }
function value(): int { echo 'rhs:'; return 9; }
try { C::$o->v = value(); echo 'bad'; }
catch (Error $e) { echo $e->getMessage(), '|'; }
echo 'after';
"#, "rhs:Attempt to assign property \"v\" on null|after");
}

/// A rejected null write unwinds its independently owned string RHS without retaining a leak.
#[test]
fn test_static_receiver_review_direct_null_string_owner() {
    verify(r#"<?php
class O { public string $text = ''; }
class C { public static ?O $o = null; }
function value(): string { return str_repeat('x', 24); }
for ($i = 0; $i < 8; $i++) {
    try { C::$o->text = value(); } catch (Error $e) { echo 'c'; }
}
echo '|after';
"#, "cccccccc|after");
}

/// A non-null static receiver is fetched after an RHS that replaces its sole owner, as in PHP.
#[test]
fn test_static_receiver_review_non_null_sole_owner() {
    verify(r#"<?php
class O { public int $v = 1; public function __destruct() { echo 'D', $this->v; } }
class C { public static O $o; }
function replace(): int { C::$o = new O(); C::$o->v = 7; return 9; }
C::$o = new O();
C::$o->v = replace();
echo C::$o->v;
"#, "D19D9");
}

/// Indexed writes target the replacement installed by the RHS, without accessing the freed object.
#[test]
fn test_static_receiver_review_array_sole_owner() {
    verify(r#"<?php
class O { public array $items = [1]; public function __destruct() { echo 'D', $this->items[0]; } }
class C { public static O $o; }
function replace(): int { C::$o = new O(); return 9; }
C::$o = new O();
C::$o->items[0] = replace();
echo '[', C::$o->items[0], ']';
"#, "D1[9]D9");
}

/// Appends also load the receiver after replacing RHS effects and retain its declared array slot.
#[test]
fn test_static_receiver_review_append_replacement() {
    verify(r#"<?php
class O { public array $items = [1]; public function __destruct() { echo 'D', $this->items[0]; } }
class C { public static ?O $o = null; }
function replace(): int { C::$o = new O(); return 9; }
C::$o = new O();
C::$o->items[] = replace();
foreach (C::$o->items as $item) { echo $item, ','; }
"#, "D11,9,D1");
}

/// Compound writes read the replacement slot after the RHS rather than updating a freed object.
#[test]
fn test_static_receiver_review_compound_replacement() {
    verify(r#"<?php
class O { public int $v = 1; public function __destruct() { echo 'D', $this->v; } }
class C { public static O $o; }
function replace(): int { C::$o = new O(); C::$o->v = 7; return 9; }
C::$o = new O();
C::$o->v += replace();
echo C::$o->v;
"#, "D116D16");
}

/// A computed receiver is evaluated eagerly and its independent return owner survives the RHS.
#[test]
fn test_static_receiver_review_computed_receiver_owner() {
    verify(r#"<?php
class O { public int $v = 1; public function __destruct() { echo 'D', $this->v; } }
class C { public static O $o; }
function receiver(): O { return C::$o; }
function replace(): int { C::$o = new O(); C::$o->v = 7; return 9; }
C::$o = new O();
receiver()->v = replace();
echo C::$o->v;
"#, "D97D7");
}

/// Runtime-name writes use the same delayed static receiver fetch as literal-name writes.
#[test]
fn test_static_receiver_review_dynamic_sole_owner() {
    verify(r#"<?php
class O { public int $v = 1; public function __destruct() { echo 'D', $this->v; } }
class C { public static O $o; }
function replace(): int { C::$o = new O(); return 9; }
C::$o = new O();
$name = 'v';
C::$o->$name = replace();
echo C::$o->v;
"#, "D19D9");
}

/// A runtime property name can select a consuming Mixed slot or stdClass storage without leaks.
#[test]
fn test_static_receiver_review_dynamic_mixed_value_ownership() {
    verify(r#"<?php
class O { public mixed $value = null; }
function write(O $object, string $name): void { $object->$name = str_repeat('x', 8); }
function dynamic(mixed $object, string $name): void { $object->$name = str_repeat('y', 8); }
$object = new O(); $dynamic = new stdClass();
for ($i = 0; $i < 12; $i++) { write($object, 'value'); dynamic($dynamic, 'value'); }
echo $object->value, ':', $dynamic->value;
"#, "xxxxxxxx:yyyyyyyy");
}

/// Boxed nullable receivers resolve their one object class for indexed and append writes.
#[test]
fn test_static_receiver_review_nullable_array_writes() {
    verify(r#"<?php
class O { public array $items = [1]; }
class C { public static ?O $o = null; }
C::$o = new O();
C::$o->items[0] = 9;
C::$o->items[] = 3;
foreach (C::$o->items as $item) { echo $item, ','; }
"#, "9,3,");
}

/// Throwing RHS expressions unwind the receiver lease for every write surface.
#[test]
fn test_static_receiver_review_throwing_rhs() {
    verify(r#"<?php
class O { public int $v = 1; public array $items = [1]; public function __destruct() { echo 'D', $this->v; } }
class C { public static O $o; }
function fail(): int { C::$o = new O(); throw new Exception('rhs'); }
$name = 'v';
C::$o = new O();
try { C::$o->v = fail(); } catch (Exception $e) { echo 'a'; }
try { C::$o->items[0] = fail(); } catch (Exception $e) { echo 'b'; }
try { C::$o->$name = fail(); } catch (Exception $e) { echo 'c'; }
"#, "D1aD1bD1cD1");
}

/// A null nullable receiver raises a catchable error instead of dropping a boxed write.
#[test]
fn test_static_receiver_review_nullable_null_errors() {
    verify(r#"<?php
class O { public array $items = [1]; }
class C { public static ?O $o = null; }
function value(): int { echo 'rhs:'; return 9; }
try { C::$o->items[0] = value(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { C::$o->items[] = value(); } catch (Error $e) { echo $e->getMessage(); }
"#, "rhs:Attempt to modify property \"items\" on null|rhs:Attempt to modify property \"items\" on null");
}
