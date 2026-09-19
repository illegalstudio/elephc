//! Purpose:
//! Regression coverage for issue #703: a union-typed local narrowed with `instanceof` and then
//! passed to a parameter that wants the object must arrive as the object, not as its box.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A `Box|bool` local lives in boxed Mixed storage, because the slot's type is flow-insensitive
//!   even where the checker has narrowed it. The callee's parameter is a concrete `Heap(Object)`,
//!   so something has to unbox between the two; when nothing did, the callee read the property
//!   off the box address and printed a pointer instead of the value.
//! - The failure was SILENT -- no diagnostic, no crash, a different wrong number on every run --
//!   which is why these fixtures assert the value rather than merely that the program compiles.
//! - `test_narrowed_union_argument_is_unboxed_at_the_call_site` pins the mechanism, so the shape
//!   cannot regress into "accidentally right" if the property ever moves to offset zero.
//! - One fixture runs again with the EIR optimizer off, because the boxes and copies it folds
//!   away change which value the call site has to unbox.

use super::*;

/// Issue repro: the callee used to receive the Mixed box and read `num_rows` off its address.
#[test]
fn test_instanceof_narrowed_union_reaches_a_typed_object_parameter() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $num_rows = 0; }
function get(): Box|bool { $b = new Box(); $b->num_rows = 2; return $b; }
function readnum(Box $b): int { return $b->num_rows; }
$r = get();
if (!($r instanceof Box)) { exit(1); }
echo readnum($r), "|", $r->num_rows;
"#,
    );
    assert_eq!(out, "2|2");
}

/// The four union flavours that box their local, each one narrowed in its own typed variable.
///
/// Each local has to keep its own declared type all the way to the call. Collecting the four
/// results into one array first would type the element `mixed`, and the four call sites would
/// collapse into a single `mixed`-to-object coercion repeated four times.
const EVERY_BOXED_UNION_FLAVOUR: &str = r#"<?php
class Box { public int $n = 0; }
class Other { public int $n = 0; }
function readnum(Box $b): int { return $b->n; }

function mkBool(): Box|bool { $b = new Box(); $b->n = 2; return $b; }
function mkNull(): ?Box { $b = new Box(); $b->n = 3; return $b; }
function mkWide(): Box|bool|int { $b = new Box(); $b->n = 4; return $b; }
function mkTwoClass(): Box|Other { $b = new Box(); $b->n = 5; return $b; }

$fromBool = mkBool();
if ($fromBool instanceof Box) { echo readnum($fromBool); }

$fromNull = mkNull();
if ($fromNull instanceof Box) { echo readnum($fromNull); }

$fromWide = mkWide();
if ($fromWide instanceof Box) { echo readnum($fromWide); }

$fromTwoClass = mkTwoClass();
if ($fromTwoClass instanceof Box) { echo readnum($fromTwoClass); }
"#;

/// Verifies every union flavour that boxes its local reaches the parameter as the object.
///
/// `?Box`, `Box|bool`, a three-member union and a two-class union all share one runtime
/// representation, and each one used to hand the callee a different heap address.
#[test]
fn test_every_boxed_union_flavour_reaches_the_parameter_as_an_object() {
    assert_eq!(compile_and_run(EVERY_BOXED_UNION_FLAVOUR), "2345");
}

/// Verifies the unbox survives with the EIR optimizer off.
///
/// Every other fixture here compiles with the optimizer on, which is the default. With it off
/// the boxes and copies it folds away stay in the instruction stream, so the call site the
/// backend has to unbox is a different one; the four values still have to arrive as objects.
#[test]
fn test_every_boxed_union_flavour_reaches_the_parameter_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run(EVERY_BOXED_UNION_FLAVOUR));
    assert_eq!(out, "2345");
}

/// Verifies the unbox happens for every call shape, not only a plain function call.
#[test]
fn test_narrowed_union_reaches_every_call_shape() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 0; }
class Reader {
    public function read(Box $b): int { return $b->n; }
    public static function sread(Box $b): int { return $b->n; }
}
function readnum(Box $b): int { return $b->n; }
function readTwo(Box $a, Box $b): int { return $a->n * 10 + $b->n; }
function mk(int $n): Box|bool { $b = new Box(); $b->n = $n; return $b; }

$v = mk(2);
if (!($v instanceof Box)) { exit(1); }
$reader = new Reader();
echo readnum($v), $reader->read($v), Reader::sread($v), readTwo($v, $v);
echo (function (Box $b): int { return $b->n; })($v);
"#,
    );
    assert_eq!(out, "222222");
}

/// Verifies the call site unboxes rather than passing the Mixed cell straight through.
///
/// The EIR still types the argument `Heap(Mixed)` and the parameter `Heap(Object)`, so the
/// unbox is the backend's job. Asserting the value alone would keep passing if the property
/// ever sat at offset zero, where a box address and an object address read the same.
#[test]
fn test_narrowed_union_argument_is_unboxed_at_the_call_site() {
    let dir = make_cli_test_dir("elephc_narrowed_object_argument_unbox");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Box { public int $n = 0; }
function readnum(Box $b): int { return $b->n; }
function mk(): Box|bool { $b = new Box(); $b->n = 2; return $b; }
$v = mk();
if ($v instanceof Box) { echo readnum($v); }
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    // The mnemonic and the symbol prefix vary INDEPENDENTLY across targets, so every
    // combination has to be tried rather than one spelling per target: linux-x86_64 emits
    // `call _fn_readnum`, underscore included, which a hand-written list of three missed.
    //
    // The leading indent matters on all of them: the directive `.globl _fn_readnum` contains
    // "bl _fn_readnum" as a substring, and only the indent keeps it from matching.
    let call = ["bl", "call"]
        .iter()
        .flat_map(|mnemonic| {
            ["_fn_readnum", "fn_readnum"]
                .iter()
                .map(move |symbol| format!("\n    {mnemonic} {symbol}"))
        })
        .find_map(|marker| {
            user_asm
                .split_once(marker.as_str())
                .map(|(before, _)| before.to_string())
        })
        .expect("the fixture must call readnum");
    assert!(
        call.contains("__rt_mixed_unbox"),
        "the narrowed union argument must be unboxed before the call: {}",
        &call[call.len().saturating_sub(700)..]
    );
}
