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
#[test]
fn test_foreach_temporary_aggregate_fresh_iterator_heap_clean() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php {PRELUDE}
foreach (new FreshAgg() as $v) {{ echo $v; }}
echo "\n";
"#
    ));
    assert_clean(out, "01IA\n");
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
    assert_clean(out, "01IA\n");
}

/// Normal completion, innermost break, break 2, and return all leave a clean heap.
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
    assert_clean(out, "0IA0IA0IA\n");
}

/// Same-frame throws from rewind/current/key/next run iterator destructors and stay clean.
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
    assert_clean(out, "IArIAcIAk0IAn\n");
}

/// Descriptor unpack of an aggregate must own getIterator() and leave a clean heap.
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
    assert_clean(out, "1IA\n");
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
    assert_clean(out, "01IA\n");
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
