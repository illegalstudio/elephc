//! Purpose:
//! Runtime ownership for `IteratorAggregate::getIterator()` results produced
//! inside `Op::IterStart`. The Mixed owner must keep those iterators alive
//! through rewind/key/current/next and release them on every exit.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures run under `--heap-debug` and require `leak summary: clean`.
//! - Destructor letters record whether the iterator dies before the aggregate
//!   on unwind from rewind, current, key, and next.
//! - A temporary aggregate is destroyed once its iterator exists and is rewound, before the
//!   first iteration, as php's `FE_RESET` frees its operand; only a `rewind()` that throws
//!   destroys the iterator first.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

const PRELUDE: &str = r#"
class RangeIter implements Iterator {
    public static int $d = 0;
    private int $i = 0;
    private int $n;
    public function __construct(int $n) { $this->n = $n; }
    public function __destruct() { echo "I"; self::$d++; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->n; }
}
class FreshAgg implements IteratorAggregate {
    public static int $d = 0;
    public function __destruct() { echo "A"; self::$d++; }
    public function getIterator(): Iterator { return new RangeIter(2); }
}
class ExistingAgg implements IteratorAggregate {
    public static int $d = 0;
    private RangeIter $it;
    public function __construct() { $this->it = new RangeIter(2); }
    public function __destruct() { echo "A"; self::$d++; }
    public function getIterator(): Iterator { return $this->it; }
}
"#;

/// A temporary aggregate that returns a fresh Iterator must not leak.
///
/// A temporary aggregate dies as soon as the loop has its iterator (A first): php's `FE_RESET`
/// frees its operand once `getIterator()` returned and the iterator was rewound, so the aggregate
/// never outlives the first iteration. Measured with `php -n` 8.5.
#[test]
fn test_foreach_temporary_aggregate_fresh_iterator_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
foreach (new FreshAgg() as $v) {{ echo $v; }}
echo "\n";
"#
    ));
    assert_clean(out, "A01I\n");
}

/// A local aggregate that returns an existing Iterator must not leak.
#[test]
fn test_foreach_local_aggregate_existing_iterator_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
$a = new ExistingAgg();
foreach ($a as $v) {{ echo $v; }}
unset($a);
echo "\n";
"#
    ));
    assert_clean(out, "01AI\n");
}

/// Normal completion, innermost break, break 2, and return all leave a clean heap.
///
/// A temporary aggregate dies as soon as the loop has its iterator (A first): php's `FE_RESET`
/// frees its operand once `getIterator()` returned and the iterator was rewound, so the aggregate
/// never outlives the first iteration. Measured with `php -n` 8.5.
#[test]
fn test_foreach_aggregate_break_return_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
foreach (new FreshAgg() as $v) {{ echo $v; break; }}
foreach ([1] as $outer) {{
    foreach (new FreshAgg() as $v) {{ echo $v; break 2; }}
}}
function first(): int {{
    foreach (new FreshAgg() as $v) {{ return $v; }}
    return 9;
}}
echo first();
echo "\n";
"#
    ));
    assert_clean(out, "A0IA0IAI0\n");
}

/// Same-frame throws from rewind/current/key/next run iterator destructors and stay clean.
///
/// A `rewind()` throw happens while the loop is still taking its iterator, so php destroys the
/// iterator and then the aggregate (`IAr`); every later throw finds the aggregate already gone
/// (`AIc`, `AIk`, `A0In`). Measured with `php -n` 8.5.
#[test]
fn test_foreach_aggregate_method_throws_destructor_timing_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
class BoomRewind extends RangeIter {{
    public function rewind(): void {{ throw new RuntimeException("r"); }}
}}
class BoomCurrent extends RangeIter {{
    public function current(): mixed {{ throw new RuntimeException("c"); }}
}}
class BoomKey extends RangeIter {{
    public function key(): mixed {{ throw new RuntimeException("k"); }}
}}
class BoomNext extends RangeIter {{
    public function next(): void {{ throw new RuntimeException("n"); }}
}}
class AggRewind implements IteratorAggregate {{
    public function __destruct() {{ echo "A"; }}
    public function getIterator(): Iterator {{ return new BoomRewind(1); }}
}}
class AggCurrent implements IteratorAggregate {{
    public function __destruct() {{ echo "A"; }}
    public function getIterator(): Iterator {{ return new BoomCurrent(1); }}
}}
class AggKey implements IteratorAggregate {{
    public function __destruct() {{ echo "A"; }}
    public function getIterator(): Iterator {{ return new BoomKey(1); }}
}}
class AggNext implements IteratorAggregate {{
    public function __destruct() {{ echo "A"; }}
    public function getIterator(): Iterator {{ return new BoomNext(2); }}
}}
try {{ foreach (new AggRewind() as $v) {{ echo $v; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }}
try {{ foreach (new AggCurrent() as $v) {{ echo $v; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }}
try {{ foreach (new AggKey() as $k => $v) {{ echo $k; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }}
try {{ foreach (new AggNext() as $v) {{ echo $v; }} }} catch (RuntimeException $e) {{ echo $e->getMessage(); }}
echo "\n";
"#
    ));
    assert_clean(out, "IArAIcAIkA0In\n");
}

/// A tracked callable array unpacking a temporary aggregate destroys it before the callee runs.
///
/// PHP's SEND_UNPACK walks the aggregate's `getIterator()` result, destroys that iterator (I),
/// then frees the temporary aggregate (A), and only afterwards invokes the callee and lets echo
/// print its result (1). The descriptor walk retires the iterator owner and the pinned source at
/// its exit, before `CallableDescriptorInvoke`, which is exactly that order.
#[test]
fn test_descriptor_unpack_aggregate_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
class UnpackAdder {{
    public function add(int $first, int $second): int {{ return $first * 10 + $second; }}
}}
$callback = [new UnpackAdder(), "add"];
echo $callback(...new FreshAgg());
echo "\n";
"#
    ));
    assert_clean(out, "IA1\n");
}

/// A Traversable parameter must dispatch direct Iterator and IteratorAggregate values at runtime.
#[test]
fn test_foreach_traversable_parameter_dispatch_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
function walk(Traversable $items): void {{
    foreach ($items as $value) {{ echo $value; }}
}}
$direct = new RangeIter(2);
walk($direct);
unset($direct);
$aggregate = new FreshAgg();
walk($aggregate);
unset($aggregate);
echo "\n";
"#
    ));
    assert_clean(out, "01I01IA\n");
}

/// An explicit throw caught inside foreach must not retire the live iterator or source.
///
/// A temporary aggregate dies as soon as the loop has its iterator (A first): php's `FE_RESET`
/// frees its operand once `getIterator()` returned and the iterator was rewound, so the aggregate
/// never outlives the first iteration. Measured with `php -n` 8.5.
#[test]
fn test_foreach_inner_catch_preserves_aggregate_owner_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
foreach (new FreshAgg() as $value) {{
    try {{
        throw new RuntimeException("x");
    }} catch (RuntimeException $exception) {{
        echo $value;
        continue;
    }}
}}
echo "\n";
"#
    ));
    assert_clean(out, "A01I\n");
}

/// A getIterator throw must unwind the temporary aggregate even before the loop body exists.
#[test]
fn test_foreach_get_iterator_throw_releases_source_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class ThrowingAggregate implements IteratorAggregate {
    public function __destruct() { echo "A"; }
    public function getIterator(): Traversable { throw new RuntimeException("x"); }
}
try {
    foreach (new ThrowingAggregate() as $value) { echo $value; }
} catch (RuntimeException $exception) {
    echo $exception->getMessage();
}
echo "\n";
"#,
    );
    assert_clean(out, "Ax\n");
}

/// Rooting the aggregate source must not hide a by-reference array local from COW writeback.
#[test]
fn test_foreach_by_reference_arrays_publish_mutations_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$indexed = [1, 2];
foreach ($indexed as &$value) { $value++; }
unset($value);
echo implode(",", $indexed), ":";

$assoc = ["a" => 1, "b" => 2];
foreach ($assoc as &$value) { $value++; }
unset($value);
echo implode(",", $assoc), ":";

$mixed = [1, "2"];
foreach ($mixed as &$value) { $value = (int) $value + 1; }
unset($value);
echo implode(",", $mixed), "\n";
"#,
    );
    assert_clean(out, "2,3:2,3:2,3\n");
}

/// A terminal first branch must not consume the handler/finally state used by its sibling.
///
/// Both branches have to run in one program: `return` is the branch that consumes a finally
/// frame, and `continue` is the sibling that must still find one. `walk(false)` catches twice,
/// so the finally runs after each catch body and the loop completes normally, retiring the
/// iterator. `walk(true)` returns out of the first iteration, so the finally runs once and the
/// iterator's destructor follows it on the return path. In both walks the temporary aggregate is
/// destroyed before the first iteration, as php's `FE_RESET` frees it once the iterator exists
/// (measured with `php -n` 8.5).
#[test]
fn test_foreach_try_branch_exit_preserves_sibling_handler_state_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
function walk(bool $leave): void {{
    foreach (new FreshAgg() as $value) {{
        try {{
            if ($leave) {{ return; }}
            throw new RuntimeException("x");
        }} catch (RuntimeException $exception) {{
            echo $value;
            continue;
        }} finally {{
            echo "f";
        }}
    }}
}}
walk(false);
walk(true);
echo "\n";
"#
    ));
    assert_clean(out, "A0f1fIAfI\n");
}
