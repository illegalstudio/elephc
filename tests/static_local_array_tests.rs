//! Purpose:
//! End-to-end tests for a SILENT MISCOMPILE of arrays held in a function `static`.
//!
//! `array_shift()` / `array_pop()` on a `static $q = [];` returned `NULL` where php returns the
//! element, for any element a PREVIOUS call had pushed. No crash and no warning — the array even
//! shrank by one, so the call site saw "my queue is empty" rather than a compiler bug.
//!
//! Two independent root causes, both about storage that OUTLIVES the call:
//!
//! - THE TYPE. `static $q = [];` runs its initializer on the first call only, but the
//!   declaration re-types the name on EVERY call, so the slot kept `array<never>` — the type of
//!   an array that is provably empty. `array<never>` element slots are ZERO WIDTH and codegen
//!   acts on that: `array_shift`'s "preserve the removed payload" arm is `mov x11, #0`, i.e. a
//!   literal null, and its compaction loop walks the buffer at 8-byte stride while
//!   `__rt_array_push_str` had re-stamped the runtime header to 16-byte string slots on the
//!   first push. A `static` seeded by a NON-empty literal was correct, which is the cut that
//!   isolates the cause. Fixed in `LoweringContext::required_static_local_storage_type`, which
//!   gives an empty-array static `array<mixed>` — boxed ELEMENT slots — instead. Not a boxed
//!   SLOT: that puts a Mixed cell in the `.comm` symbol, and a `mixed &$param` argument is then
//!   handed that cell as its reference and replaces it, so `sort($staticQueue)` goes from a stale
//!   answer to a DESTROYED one. Measured both ways before choosing.
//!   Gated on `append_vivify::locals_written_with_non_integer_keys`, because `[]` infers as
//!   `Array(never)` even for a static every write makes a hash, and pinning INDEXED storage on
//!   one of those would have changed how IT is broken.
//!
//! - THE PLACE. A `static` lives in a `.comm` symbol, not a frame slot, and every backend
//!   resolver that names "somewhere a relocated container can be published back to" matched only
//!   `Op::LoadLocal`/`Op::LoadRefCell`. `ReceiverPlace::resolve` therefore answered `Opaque` for
//!   a `static` receiver, and `Opaque`'s write-back is `Ok(())` — a SILENT drop. So once an
//!   append grew the array past its initial capacity, `__rt_array_grow`'s realloc moved it and
//!   the symbol kept the freed pointer: `static $q = ['s0'];` plus nine appends read back as one
//!   element. Fixed by `ReceiverPlace::StaticLocal` and
//!   `FunctionContext::writeback_static_local_array_source`.
//!
//! Called from:
//! - `cargo test --test static_local_array_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here was taken from reference PHP on this machine, not from reasoning
//!   about what php ought to do.
//! - THE CONTROLS ARE THE POINT. A test that only pinned the failing case would be satisfied by
//!   a "fix" that made every `static` behave like a `global`, or that persisted every extracted
//!   value and leaked. The four controls — a non-empty initializer, an indexed read of the same
//!   element, push-and-shift inside ONE call, and the identical shape in a `global` — all PASSED
//!   before the fix and must keep passing, so they are what holds the fix in place.
//! - The neighbour tests cover the rest of the by-reference array family on the same slot, since
//!   they share the receiver-place resolver: `array_unshift` was REFUSED at compile time by the
//!   loud half of the same hole, and `array_splice`/the internal pointer family/`array_slice`/
//!   `foreach` were each wrong in their own way.
//! - STILL BROKEN, and named in the tests that stop short of them rather than pinned at a wrong
//!   value: a `static` passed to a by-REFERENCE PARAMETER is not written back (which is what
//!   `sort`/`rsort`/`usort` reduce to), `unset($q[0])` on a static empties it, and a
//!   STRING-KEYED static loses writes and reads garbage. All three are unchanged by this fix,
//!   byte for byte, and all three predate it.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, run it, and assert stdout — the same harness style as
//!   `array_result_type_tests` / `shutdown_function_tests`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` to a plain executable, runs it, and returns its stdout.
fn compile_and_run(source: &str, stem: &str) -> String {
    let dir = make_test_dir(stem);
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed for {stem}:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let run = Command::new(dir.join(stem))
        .output()
        .expect("failed to run compiled binary");
    assert!(
        run.status.success(),
        "compiled binary {stem} exited non-zero ({:?}):\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8_lossy(&run.stdout).into_owned()
}

// ---------------------------------------------------------------------------------------------
// THE REPORTED DEFECT
// ---------------------------------------------------------------------------------------------

/// `array_shift()` on a `static` array returns the element an EARLIER call pushed.
///
/// This is the reported repro verbatim. Before the fix elephc printed `returned=NULL` twice
/// while the `before=`/`after=` counts were already correct, which is what made it look like a
/// pure return-value loss rather than the element-width confusion it is.
#[test]
fn array_shift_on_a_static_returns_the_element_a_previous_call_pushed() {
    let out = compile_and_run(
        r#"<?php
function probe(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'a'; $q[] = 'b'; return; }
    echo 'before=', count($q), ' ';
    $e = array_shift($q);
    echo 'returned=', var_export($e, true), ' after=', count($q), "\n";
}
probe(1); probe(2); probe(2);
"#,
        "static_shift_across_calls",
    );
    assert_eq!(
        out,
        "before=2 returned='a' after=1\nbefore=1 returned='b' after=0\n",
        "array_shift on a static array lost the element a previous call pushed"
    );
}

/// `array_pop()` has the identical shape and was wrong the identical way.
#[test]
fn array_pop_on_a_static_returns_the_element_a_previous_call_pushed() {
    let out = compile_and_run(
        r#"<?php
function probe(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'a'; $q[] = 'b'; return; }
    $e = array_pop($q);
    echo 'returned=', var_export($e, true), ' after=', count($q), "\n";
}
probe(1); probe(2); probe(2);
"#,
        "static_pop_across_calls",
    );
    assert_eq!(
        out,
        "returned='b' after=1\nreturned='a' after=0\n",
        "array_pop on a static array lost the element a previous call pushed"
    );
}

/// The element TYPE is irrelevant — every payload shape the indexed backend has was wrong.
///
/// The first report characterised this as "a static array whose ELEMENTS ARE ARRAYS" and called
/// a flat static array of scalars correct. It is not. Each row below returned `NULL` before the
/// fix, and the `float` row is the sharpest: its survivor printed as `4612811918334230528`, the
/// float's bit pattern read back through the wrong element width.
#[test]
fn every_element_type_survives_a_shift_out_of_a_static() {
    let out = compile_and_run(
        r#"<?php
function strings(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'aa'; $q[] = 'bb'; $q[] = 'cc'; return; }
    echo 'str=', var_export(array_shift($q), true), ' left=', json_encode($q), "\n";
}
function ints(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 11; $q[] = 22; return; }
    echo 'int=', var_export(array_shift($q), true), ' left=', json_encode($q), "\n";
}
function floats(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 1.5; $q[] = 2.5; return; }
    echo 'float=', var_export(array_shift($q), true), ' left=', json_encode($q), "\n";
}
function bools(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = true; $q[] = false; return; }
    echo 'bool=', var_export(array_shift($q), true), ' left=', json_encode($q), "\n";
}
function nested(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = ['a','b']; $q[] = ['c']; return; }
    echo 'nested=', json_encode(array_shift($q)), ' left=', json_encode($q), "\n";
}
strings(1); strings(2);
ints(1); ints(2);
floats(1); floats(2);
bools(1); bools(2);
nested(1); nested(2);
"#,
        "static_shift_element_types",
    );
    assert_eq!(
        out,
        concat!(
            "str='aa' left=[\"bb\",\"cc\"]\n",
            "int=11 left=[22]\n",
            "float=1.5 left=[2.5]\n",
            "bool=true left=[false]\n",
            "nested=[\"a\",\"b\"] left=[[\"c\"]]\n",
        ),
        "the element type of a static array changed which shift was wrong, but not whether"
    );
}

/// A `static` array must keep everything pushed into it once it outgrows its first allocation.
///
/// This is the PLACE half, and it is the larger of the two defects: nothing here shifts or pops
/// at all. `static $q = [];` plus nine appends read back as `count() === 0`, because
/// `__rt_array_grow`'s realloc moved the container and no resolver knew how to publish the new
/// pointer into the `.comm` symbol. The `int` row proves it is not a string-storage quirk, and
/// the NON-EMPTY-initializer row proves it is not the `array<never>` type either: that one is a
/// correctly typed `array<string>` slot and it lost nine of its ten elements.
#[test]
fn a_static_array_survives_growth_past_its_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
function grow_strings(int $op): void {
    static $q = [];
    if ($op === 1) { for ($i = 0; $i < 9; $i++) { $q[] = 'x' . $i; } return; }
    echo 'strings=', count($q), ' ', json_encode($q), "\n";
}
function grow_ints(int $op): void {
    static $q = [];
    if ($op === 1) { for ($i = 0; $i < 9; $i++) { $q[] = $i; } return; }
    echo 'ints=', count($q), ' ', json_encode($q), "\n";
}
function grow_seeded(int $op): void {
    static $q = ['s0'];
    if ($op === 1) { for ($i = 0; $i < 9; $i++) { $q[] = 'y' . $i; } return; }
    echo 'seeded=', count($q), ' ', json_encode($q), "\n";
}
function grow_same_call(): void {
    static $q = ['s0'];
    for ($i = 0; $i < 9; $i++) { $q[] = 'z' . $i; }
    echo 'same_call=', count($q), "\n";
}
grow_strings(1); grow_strings(2);
grow_ints(1); grow_ints(2);
grow_seeded(1); grow_seeded(2);
grow_same_call();
"#,
        "static_growth",
    );
    assert_eq!(
        out,
        concat!(
            "strings=9 [\"x0\",\"x1\",\"x2\",\"x3\",\"x4\",\"x5\",\"x6\",\"x7\",\"x8\"]\n",
            "ints=9 [0,1,2,3,4,5,6,7,8]\n",
            "seeded=10 [\"s0\",\"y0\",\"y1\",\"y2\",\"y3\",\"y4\",\"y5\",\"y6\",\"y7\",\"y8\"]\n",
            "same_call=10\n",
        ),
        "a static array lost its contents when growth relocated it"
    );
}

/// Drain a `static` queue to empty, the shape `__elephc_shutdown_function_state` wanted.
///
/// `while (count($q) > 0) { array_shift($q); }` is the natural way to write a callback queue, and
/// it is exactly what the shutdown-function prelude could not use. Also pins that a drained
/// static stays drained on the NEXT call, and that entries appended during the drain are reached.
#[test]
fn a_static_queue_drains_to_empty_and_stays_empty() {
    let out = compile_and_run(
        r#"<?php
function queue(int $op, string $item = ''): string {
    static $q = [];
    if ($op === 1) { $q[] = $item; return ''; }
    $out = '';
    while (count($q) > 0) { $out .= (string) array_shift($q); }
    return $out;
}
queue(1, 'a'); queue(1, 'b'); queue(1, 'c');
echo 'drain1=', queue(2), "\n";
echo 'drain2=', queue(2), "\n";
queue(1, 'd');
echo 'drain3=', queue(2), "\n";
"#,
        "static_queue_drain",
    );
    assert_eq!(
        out,
        "drain1=abc\ndrain2=\ndrain3=d\n",
        "a static queue did not drain in registration order"
    );
}

// ---------------------------------------------------------------------------------------------
// THE CONTROLS — all four PASSED before the fix, and prove it did not over-correct
// ---------------------------------------------------------------------------------------------

/// CONTROL: a `static` seeded by a NON-EMPTY literal was always correct, and still is.
///
/// This is the cut that located the cause: the only difference from the failing case is that the
/// initializer constrains the element type, so the slot never carried `array<never>`. If a fix
/// broke this, it broke the concrete-slot path while repairing the gradual one.
#[test]
fn control_static_seeded_by_a_non_empty_initializer_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
function seeded(): void {
    static $q = ['x', 'y'];
    echo 'before=', count($q), ' ';
    $e = array_shift($q);
    echo 'returned=', var_export($e, true), ' after=', count($q), "\n";
}
seeded();
seeded();

function seeded_drain(int $op): void {
    static $q = ['seed'];
    if ($op === 1) { array_shift($q); $q[] = 'a'; return; }
    echo 'drained=', var_export(array_shift($q), true), "\n";
}
seeded_drain(1); seeded_drain(2);
"#,
        "control_seeded_initializer",
    );
    assert_eq!(
        out,
        concat!(
            "before=2 returned='x' after=1\n",
            "before=1 returned='y' after=0\n",
            "drained='a'\n",
        ),
        "the non-empty-initializer control regressed"
    );
}

/// CONTROL: reading the same element by INDEX was always correct, and still is.
///
/// This is what proved the payload was really in the storage and lost on the way out, rather
/// than the builtin having been handed a copy of the array.
#[test]
fn control_indexed_read_of_the_same_element_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
function by_index(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = ['a', 'b']; $q[] = 'tail'; return; }
    echo 'index0=', count((array) $q[0]), ' index1=', var_export($q[1], true), "\n";
}
by_index(1); by_index(2); by_index(2);
"#,
        "control_indexed_read",
    );
    assert_eq!(
        out,
        "index0=2 index1='tail'\nindex0=2 index1='tail'\n",
        "the indexed-read control regressed"
    );
}

/// CONTROL: push and shift inside ONE call was always correct, and still is.
///
/// Within a call the checker's flow-sensitive widening retypes the binding after the push, so
/// the shift never saw `array<never>`. A fix that only widened the DECLARATION would leave this
/// untouched; a fix that disturbed flow typing would not.
#[test]
fn control_push_and_shift_within_one_call_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
function same_call(): void {
    static $q = [];
    $q[] = ['a', 'b'];
    echo 'shift_same_call=', count((array) array_shift($q)), ' left=', count($q), "\n";
}
same_call();
same_call();

function local_only(): void {
    $q = [];
    for ($i = 0; $i < 9; $i++) { $q[] = 'y' . $i; }
    echo 'local=', count($q), ' shift=', var_export(array_shift($q), true), "\n";
}
local_only();
"#,
        "control_same_call",
    );
    assert_eq!(
        out,
        concat!(
            "shift_same_call=2 left=0\n",
            "shift_same_call=2 left=0\n",
            "local=9 shift='y0'\n",
        ),
        "the same-call control regressed"
    );
}

/// CONTROL: the identical shape in a `global` was always correct, and still is.
///
/// The fix routes an empty-array `static` onto the gradual storage a `global` already uses, so
/// this control is now load-bearing in a second way: it pins the path the fix reuses.
#[test]
fn control_the_same_shape_in_a_global_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
$g = [];
function via_global(int $op): void {
    global $g;
    if ($op === 1) { $g[] = ['a', 'b']; return; }
    echo 'shift_global=', count((array) array_shift($g)), "\n";
}
via_global(1); via_global(2);

$h = [];
function sort_global(int $op): void {
    global $h;
    if ($op === 1) { $h[] = 'c'; $h[] = 'a'; $h[] = 'b'; return; }
    sort($h);
    echo 'sort_global=', json_encode($h), "\n";
}
sort_global(1); sort_global(2);
"#,
        "control_global",
    );
    assert_eq!(
        out,
        "shift_global=2\nsort_global=[\"a\",\"b\",\"c\"]\n",
        "the global control regressed"
    );
}

/// CONTROL: a `static` the body never MUTATES keeps its exact declared contents.
///
/// The fix widens an empty-array static's storage; it must not disturb a static that is only
/// ever read, nor a non-array one.
#[test]
fn control_read_only_and_scalar_statics_are_unchanged() {
    let out = compile_and_run(
        r#"<?php
function read_only(): void {
    static $table = ['a' => 1, 'b' => 2];
    echo 'table=', json_encode($table), ' a=', $table['a'], "\n";
}
read_only(); read_only();

function counter(): void {
    static $n = 0;
    $n++;
    echo 'n=', $n, "\n";
}
counter(); counter(); counter();

function empty_and_untouched(): void {
    static $q = [];
    echo 'empty=', count($q), ' json=', json_encode($q), "\n";
}
empty_and_untouched(); empty_and_untouched();
"#,
        "control_read_only",
    );
    assert_eq!(
        out,
        concat!(
            "table={\"a\":1,\"b\":2} a=1\n",
            "table={\"a\":1,\"b\":2} a=1\n",
            "n=1\nn=2\nn=3\n",
            "empty=0 json=[]\n",
            "empty=0 json=[]\n",
        ),
        "a read-only or scalar static regressed"
    );
}

// ---------------------------------------------------------------------------------------------
// THE NEIGHBOURS — the rest of the by-reference array family on the same slot
// ---------------------------------------------------------------------------------------------

/// The by-reference array builtins that the receiver-place fix repaired on a `static`.
///
/// All of these resolve their write-back through `ReceiverPlace`, which answered `Opaque` for a
/// `static`. Before the fix, measured against reference PHP: `array_unshift` was REFUSED at
/// compile time ("array_unshift for a by-reference receiver that is not a local variable slot"),
/// `array_push` printed `[]`, `array_splice` returned an empty cut AND emptied the array,
/// `current`/`next`/`end`/`reset` all returned `false`, `array_slice` returned `[null]`, and a
/// plain `foreach` produced no iterations.
///
/// WHAT IS STILL WRONG, and deliberately not asserted here rather than pinned at a wrong value:
/// `sort`/`rsort`/`usort` drop the reordering (they lower to a prelude function taking `&$array`,
/// and a `static` passed to a by-REFERENCE PARAMETER is still not written back — the contents
/// survive now, where before the static was emptied), `unset($q[0])` empties the array, and a
/// STRING-KEYED static (`static $m = []; $m['k'] = …;`) loses writes and reads garbage. That last
/// one is why the storage widening is gated on
/// `append_vivify::locals_written_with_non_integer_keys`: it is broken identically with and
/// without this fix, and widening it would have changed which way.
#[test]
fn the_repaired_by_reference_array_family_works_on_a_static() {
    let out = compile_and_run(
        r#"<?php
function n_unshift(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'b'; $q[] = 'c'; return; }
    array_unshift($q, 'a');
    echo 'unshift=', json_encode($q), "\n";
}
function n_push(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'a'; return; }
    array_push($q, 'b', 'c');
    echo 'push=', json_encode($q), "\n";
}
function n_splice(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'a'; $q[] = 'b'; $q[] = 'c'; return; }
    $cut = array_splice($q, 1, 1);
    echo 'splice=', json_encode($cut), ' left=', json_encode($q), "\n";
}
function n_pointer(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'a'; $q[] = 'b'; $q[] = 'c'; return; }
    echo 'current=', var_export(current($q), true);
    echo ' next=', var_export(next($q), true);
    echo ' end=', var_export(end($q), true);
    echo ' reset=', var_export(reset($q), true), "\n";
}
function n_slice(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'a'; $q[] = 'b'; return; }
    echo 'reverse=', json_encode(array_reverse($q)), ' slice=', json_encode(array_slice($q, 1)), "\n";
}
function n_foreach(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'a'; $q[] = 'b'; return; }
    $out = '';
    foreach ($q as $v) { $out .= $v; }
    echo 'foreach=', $out, "\n";
}
n_unshift(1); n_unshift(2);
n_push(1); n_push(2);
n_splice(1); n_splice(2);
n_pointer(1); n_pointer(2);
n_slice(1); n_slice(2);
n_foreach(1); n_foreach(2);
"#,
        "static_by_ref_family",
    );
    assert_eq!(
        out,
        concat!(
            "unshift=[\"a\",\"b\",\"c\"]\n",
            "push=[\"a\",\"b\",\"c\"]\n",
            "splice=[\"b\"] left=[\"a\",\"c\"]\n",
            "current='a' next='b' end='c' reset='a'\n",
            "reverse=[\"b\",\"a\"] slice=[\"b\"]\n",
            "foreach=ab\n",
        ),
        "a by-reference array builtin on a static slot regressed"
    );
}

/// A `sort()` on a `static` must at least not DESTROY it.
///
/// The reordering is still dropped — `sort($q)` lowers to `__elephc_sort_mixed(&$array)` and a
/// `static` handed to a by-reference parameter is not written back. That is a separate open
/// defect. What this pins is the part the fix does own: before it, the same call left the static
/// EMPTY, because the mutation relocated the container and the new pointer went nowhere. Data
/// loss and a dropped mutation are not the same severity, and this is the line between them.
#[test]
fn sort_on_a_static_no_longer_empties_it() {
    let out = compile_and_run(
        r#"<?php
function n_sort(int $op): void {
    static $q = [];
    if ($op === 1) { $q[] = 'c'; $q[] = 'a'; $q[] = 'b'; return; }
    sort($q);
    // Membership, not order: the ORDER is the part sort() still gets wrong on a static, and
    // pinning it would freeze a wrong answer into a test.
    echo 'count=', count($q),
        ' a=', (int) in_array('a', $q, true),
        ' b=', (int) in_array('b', $q, true),
        ' c=', (int) in_array('c', $q, true), "\n";
}
n_sort(1); n_sort(2);
"#,
        "static_sort_keeps_contents",
    );
    assert_eq!(
        out,
        "count=3 a=1 b=1 c=1\n",
        "sort() on a static lost its contents"
    );
}

// ---------------------------------------------------------------------------------------------
// THE OTHER SLOT SHAPES — never affected, pinned so a future fix does not break them
// ---------------------------------------------------------------------------------------------

/// A static class property, an instance property and a class constant were all already correct.
///
/// The repo's own memory records that one slot shape can carry several different refcount rules,
/// so these were measured rather than assumed. All seven rows below matched php BEFORE the fix
/// too — the defect is specific to the function-`static` slot — and they are pinned here because
/// the `static` fix reaches the shared array/hash write-back paths these also use.
#[test]
fn other_persistent_slot_shapes_are_unchanged() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public static array $sq = [];
    public static $untyped = [];
    public array $iq = [];
    const CQ = ['ca', 'cb'];
}
function static_prop(int $op): void {
    if ($op === 1) { Box::$sq[] = 'a'; Box::$sq[] = 'b'; return; }
    echo 'staticprop=', var_export(array_shift(Box::$sq), true), ' after=', json_encode(Box::$sq), "\n";
}
function static_prop_untyped(int $op): void {
    if ($op === 1) { Box::$untyped[] = 'a'; Box::$untyped[] = 'b'; return; }
    echo 'untyped=', var_export(array_shift(Box::$untyped), true), ' after=', json_encode(Box::$untyped), "\n";
}
function class_const(): void {
    $c = Box::CQ;
    echo 'classconst=', var_export(array_shift($c), true), ' after=', json_encode($c), "\n";
}
function inst_prop(): void {
    $b = new Box();
    $b->iq[] = 'a';
    $b->iq[] = 'b';
    echo 'instprop=', var_export(array_shift($b->iq), true), ' after=', json_encode($b->iq), "\n";
}
class Queue {
    public array $items = [];
    public function push(string $s): void { $this->items[] = $s; }
    public function drain(): string {
        $out = '';
        while (count($this->items) > 0) { $out .= (string) array_shift($this->items); }
        return $out;
    }
}
static_prop(1); static_prop(2);
static_prop_untyped(1); static_prop_untyped(2);
class_const(); class_const();
inst_prop();
$q = new Queue();
$q->push('a'); $q->push('b');
echo 'method_drain=', $q->drain(), "\n";
"#,
        "other_slot_shapes",
    );
    assert_eq!(
        out,
        concat!(
            "staticprop='a' after=[\"b\"]\n",
            "untyped='a' after=[\"b\"]\n",
            "classconst='ca' after=[\"cb\"]\n",
            "classconst='ca' after=[\"cb\"]\n",
            "instprop='a' after=[\"b\"]\n",
            "method_drain=ab\n",
        ),
        "a persistent slot shape other than a function static regressed"
    );
}
