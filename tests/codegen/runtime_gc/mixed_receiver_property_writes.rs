//! Purpose:
//! Ownership tests for issue #1094: a property write through a `mixed` receiver now reaches a
//! declared slot instead of being discarded, so the value it stores is one the object owns and the
//! previous occupant is one it must release.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`.
//! - The loop count is high enough that a per-iteration leak of one block is unmistakable.
//! - Before the fix these programs were trivially clean because the store never happened; the
//!   point of the tests is that they stay clean now that it does.
//! - Receivers that cannot hold the property (a non-object, a Closure, a readonly class, an enum)
//!   raise PHP's Error and retire the value they were handed; the enum fixture tolerates only
//!   the enum case singleton as a live block.

use crate::support::*;

/// Erased writes reject null typed slots while preserving allowed arrays and object contents.
#[test]
fn test_erased_nullable_property_dimension_guards_bundle() {
    let source = r#"<?php
class ErasedNullableObject { public ?ArrayObject $items = null; }
class ErasedNullableText { public ?string $items = null; }
class ErasedNullableFloat { public ?float $items = null; }
class ErasedNullableInt { public ?int $items = null; }
class ErasedNullableOther {}
class ErasedNullablePlain { public ?ErasedNullableOther $items = null; }
class ErasedNullableArray { public ?array $items = null; }
class ErasedNullableUnion { public ArrayObject|array|null $items = null; }
class ErasedNullableReadonly {
    public readonly ?ArrayObject $items;
    public function __construct(?ArrayObject $items) { $this->items = $items; }
}
class ErasedNullablePrivate {
    public private(set) ?ArrayObject $items;
    public function __construct(?ArrayObject $items) { $this->items = $items; }
}
function ErasedNullableWrite(mixed $box): void { $box->items[0] = 'x'; }
function ErasedNullableReject(mixed $box): void {
    try { ErasedNullableWrite($box); }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), ':', $box->items === null ? 'null' : 'changed', '|'; }
}
ErasedNullableReject(new ErasedNullableObject());
ErasedNullableReject(new ErasedNullableText());
ErasedNullableReject(new ErasedNullableFloat());
ErasedNullableReject(new ErasedNullableInt());
ErasedNullableReject(new ErasedNullablePlain());
ErasedNullableReject(new ErasedNullableReadonly(null));
ErasedNullableReject(new ErasedNullablePrivate(null));
$object = new ErasedNullableObject(); $object->items = new ArrayObject([1]);
ErasedNullableWrite($object); echo $object->items[0], '|';
$text = new ErasedNullableText(); $text->items = 'abc';
ErasedNullableWrite($text); echo $text->items, '|';
$readonly = new ErasedNullableReadonly(new ArrayObject([1]));
$private = new ErasedNullablePrivate(new ArrayObject([1]));
ErasedNullableWrite($readonly); ErasedNullableWrite($private);
echo $readonly->items[0], ':', $private->items[0], '|';
$array = new ErasedNullableArray(); $union = new ErasedNullableUnion();
ErasedNullableWrite($array); ErasedNullableWrite($union);
echo json_encode($array->items), ':', json_encode($union->items);
unset($object, $text, $readonly, $private, $array, $union);
"#;
    let renamed = source.replace("ErasedNullable", "EvalErasedNullable")
        + "\n$native = new ErasedNullableObject(); EvalErasedNullableReject($native); unset($native);";
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = concat!(
        "TypeError:Cannot auto-initialize an array inside property ErasedNullableObject::$items of type ?ArrayObject:null|",
        "TypeError:Cannot auto-initialize an array inside property ErasedNullableText::$items of type ?string:null|",
        "TypeError:Cannot auto-initialize an array inside property ErasedNullableFloat::$items of type ?float:null|",
        "TypeError:Cannot auto-initialize an array inside property ErasedNullableInt::$items of type ?int:null|",
        "TypeError:Cannot auto-initialize an array inside property ErasedNullablePlain::$items of type ?ErasedNullableOther:null|",
        "Error:Cannot indirectly modify readonly property ErasedNullableReadonly::$items:null|",
        "Error:Cannot indirectly modify private(set) property ErasedNullablePrivate::$items from global scope:null|",
        "x|xbc|x:x|[\"x\"]:[\"x\"]",
    );
    let eval_expected = expected.replace("ErasedNullable", "EvalErasedNullable")
        + "TypeError:Cannot auto-initialize an array inside property ErasedNullableObject::$items of type ?ArrayObject:null|";
    assert_bundle_with(&[
        BundleCase { name: "nullable_property_aot", source, expected },
        BundleCase { name: "nullable_property_eval", source: &eval, expected: &eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}

/// A throwing RHS retires the acquired nullable receiver through the same-frame unwind chain.
#[test]
fn test_static_property_object_receiver_throwing_rhs_releases_pin() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReviewThrowObject { public int $v = 1; }
class ReviewThrowHolder { public static ?ReviewThrowObject $o = null; }
function replaceThenThrowReviewReceiver(): int {
    ReviewThrowHolder::$o = new ReviewThrowObject();
    throw new Exception("stop");
}
$total = 0;
for ($i = 0; $i < 12; $i++) {
    $original = new ReviewThrowObject();
    ReviewThrowHolder::$o = $original;
    try { ReviewThrowHolder::$o->v = replaceThenThrowReviewReceiver(); }
    catch (Exception $error) { $total += $original->v; }
    unset($original, $error);
}
echo $total;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "12", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A string property overwritten through a `mixed` receiver: the incoming block is retained and the
/// previous one released, once each, every iteration.
#[test]
fn test_mixed_receiver_string_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public string $s = "seed"; }
function w(mixed $o, string $v): void { $o->s = $v; }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, "value" . $i); }
echo $t->s, "\n";
"#,
    );
    assert_clean(out, "value39\n");
}

/// An array property overwritten the same way, so the released previous occupant owns elements of
/// its own.
#[test]
fn test_mixed_receiver_array_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [0]; }
function w(mixed $o, int $v): void { $o->items = [$v, $v + 1]; }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, $i); }
echo count($t->items), ":", $t->items[0], "\n";
"#,
    );
    assert_clean(out, "2:39\n");
}

/// The stdClass payload keeps its own path; pinned here so the fallback's ownership is covered by
/// the same sweep.
#[test]
fn test_mixed_receiver_stdclass_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function w(mixed $o, string $v): void { $o->s = $v; }
$t = new stdClass();
$t->s = "seed";
for ($i = 0; $i < 40; $i++) { w($t, "value" . $i); }
echo $t->s, "\n";
"#,
    );
    assert_clean(out, "value39\n");
}

/// Two classes declaring the same property, alternating through the same call site, so both arms
/// of the class-id chain run and both release their own previous occupant.
#[test]
fn test_mixed_receiver_write_dispatch_is_heap_clean_for_both_classes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public string $s = "t"; }
class U { public string $s = "u"; }
function w(mixed $o, string $v): void { $o->s = $v; }
$t = new T();
$u = new U();
for ($i = 0; $i < 40; $i++) { w($t, "t" . $i); w($u, "u" . $i); }
echo $t->s, ":", $u->s, "\n";
"#,
    );
    assert_clean(out, "t39:u39\n");
}

/// A BORROWED source: the property retains it, the caller keeps its own reference, and neither is
/// released twice. This is the shape an earlier cut of the fix turned into a use-after-free.
#[test]
fn test_mixed_receiver_write_of_a_borrowed_source_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [0]; }
function w(mixed $o, array $a): void { $o->items = $a; }
$t = new T();
for ($i = 0; $i < 40; $i++) { $a = [$i, $i + 1]; w($t, $a); echo count($a); }
echo "|", count($t->items), "\n";
"#,
    );
    assert!(out.stdout.ends_with("|2\n"), "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An OWNING temporary: the release lowering now emits must fire exactly once, or the temporary
/// leaks once per write — three blocks for an array literal and its two boxed elements.
#[test]
fn test_mixed_receiver_write_of_an_owning_temporary_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [0]; }
function w(mixed $o, int $v): void { $o->items = [$v, $v + 1]; }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, $i); }
echo count($t->items), ":", $t->items[0], "\n";
"#,
    );
    assert_clean(out, "2:39\n");
}

/// A same-type write through a `mixed` receiver into a refined UNTYPED scalar property is accepted
/// (#1319); the slot copies the payload out of the box, so the owned source box must retire each
/// iteration.
#[test]
fn test_mixed_receiver_refined_untyped_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public $n = 1; public $s = "seed"; }
function w(mixed $o, int $n, string $s): void { $o->n = $n; $o->s = $s; }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, $i, "value" . $i); }
echo $t->n, ":", $t->s, "\n";
"#,
    );
    assert_clean(out, "39:value39\n");
}

/// Bool and float slots narrowed from a boxed value through a `mixed` receiver stay balanced.
#[test]
fn test_mixed_receiver_bool_float_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public bool $z = false; public float $f = 0.0; }
function w(mixed $o): void { for ($i = 0; $i < 40; $i++) { $o->z = !$o->z; $o->f += 1.5; } }
$t = new T();
w($t);
echo $t->z ? "T" : "F", ":", $t->f, "\n";
"#,
    );
    assert_clean(out, "F:60\n");
}

/// PHP source shared by the literal-name and runtime-name receiver refusal fixtures.
///
/// `@NAME@` is replaced by the property expression each fixture writes through.
const RECEIVER_REFUSAL_SOURCE: &str = r#"<?php
readonly class RefusedReadonly {}
class RefusedPlain {}
function refused_write($obj, string $text): void {
    try {
        $name = "p" . $text;
        $obj->@NAME@ = "v" . $text;
        echo "stored ", $obj->@NAME@, "\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
for ($i = 0; $i < 3; $i++) {
    refused_write(new RefusedPlain(), "a");
    refused_write(new RefusedReadonly(), "b");
    refused_write(function () {}, "d");
    refused_write(5, "e");
    refused_write(true, "f");
    refused_write(false, "g");
    refused_write(null, "h");
    refused_write([1], "i");
    refused_write("s", "j");
    refused_write(1.5, "k");
    refused_write(new stdClass(), "l");
}
"#;

/// Expected output of one round of [`RECEIVER_REFUSAL_SOURCE`], with `@NAME@` standing for the
/// property name PHP reports.
const RECEIVER_REFUSAL_ROUND: &str = "stored va
Error: Cannot create dynamic property RefusedReadonly::$@b@
Error: Cannot create dynamic property Closure::$@d@
Error: Attempt to assign property \"@e@\" on int
Error: Attempt to assign property \"@f@\" on true
Error: Attempt to assign property \"@g@\" on false
Error: Attempt to assign property \"@h@\" on null
Error: Attempt to assign property \"@i@\" on array
Error: Attempt to assign property \"@j@\" on string
Error: Attempt to assign property \"@k@\" on float
stored vl
";

/// Builds the expected output, naming each refused property the way the fixture spells it.
fn receiver_refusal_expected(name: impl Fn(&str) -> String) -> String {
    let mut round = RECEIVER_REFUSAL_ROUND.to_string();
    for suffix in ["b", "d", "e", "f", "g", "h", "i", "j", "k"] {
        round = round.replace(&format!("@{suffix}@"), &name(suffix));
    }
    round.repeat(3)
}

/// A literal-name write through an untyped receiver raises PHP's Error for a readonly class, a
/// Closure and every non-object, instead of dropping the write and leaking the value cell.
#[test]
fn test_mixed_receiver_literal_write_refuses_receivers_without_property_storage() {
    let out = compile_and_run_with_heap_debug(&RECEIVER_REFUSAL_SOURCE.replace("@NAME@", "label"));
    assert_clean(out, &receiver_refusal_expected(|_| "label".to_string()));
}

/// The runtime-name form reports the evaluated name in each Error.
#[test]
fn test_mixed_receiver_runtime_name_write_refuses_receivers_without_property_storage() {
    let out = compile_and_run_with_heap_debug(&RECEIVER_REFUSAL_SOURCE.replace("@NAME@", "$name"));
    assert_clean(out, &receiver_refusal_expected(|suffix| format!("p{suffix}")));
}

/// An intermediate property hop on a non-object or a Closure raises PHP's modification Error.
#[test]
fn test_mixed_receiver_write_hop_refuses_non_objects_and_closures() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class HopOuter { public $inner; }
function hop_write($obj): void {
    try {
        $obj->inner->x = 1;
        echo "stored\n";
    } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
$outer = new HopOuter();
$outer->inner = new stdClass();
hop_write($outer);
hop_write(7);
hop_write(function () {});
"#,
    );
    assert_clean(
        out,
        "stored\nError: Attempt to modify property \"inner\" on int\n\
         Error: Cannot create dynamic property Closure::$inner\n",
    );
}

/// An enum refuses literal and runtime-name writes. The enum case singleton is the one block that
/// stays live, so a leaked value cell per write would show up as extra live blocks.
#[test]
fn test_mixed_receiver_write_refuses_an_enum_receiver() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
enum RefusedEnum { case One; }
class RefusedEnumPlain {}
function enum_write($obj, string $text): void {
    try {
        $obj->label = "v" . $text;
        echo "stored\n";
    } catch (Error $e) {
        echo $e->getMessage(), "\n";
    }
    try {
        $name = "p" . $text;
        $obj->$name = "v" . $text;
        echo "stored\n";
    } catch (Error $e) {
        echo $e->getMessage(), "\n";
    }
}
for ($i = 0; $i < 20; $i++) {
    enum_write(new RefusedEnumPlain(), "a");
    enum_write(RefusedEnum::One, "b");
}
"#,
    );
    let round = "stored\nstored\nCannot create dynamic property RefusedEnum::$label\n\
                 Cannot create dynamic property RefusedEnum::$pb\n";
    assert_eq!(out.stdout, round.repeat(20), "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: live_blocks=1 "),
        "expected only the enum singleton to stay live, got: {}",
        out.stderr
    );
}

/// The runtime Mixed setter releases the value cell when it cannot store the write.
///
/// `ArrayObject` keeps a compiler-owned layout with no per-instance property hash, so this backend
/// still drops the dynamic property PHP would create there. The dropped write must not leak the
/// boxed value it was handed (storing the property is issue #2552).
#[test]
fn test_mixed_receiver_dropped_write_releases_the_value_cell() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class DroppedPlain {}
function dropped_write($obj, string $text): void { $obj->label = $text . "!"; }
for ($i = 0; $i < 20; $i++) {
    dropped_write(new DroppedPlain(), "a" . $i);
    dropped_write(new ArrayObject([]), "b" . $i);
}
echo "done\n";
"#,
    );
    assert_clean(out, "done\n");
}
