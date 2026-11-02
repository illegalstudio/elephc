//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, iterable builtins and casts, including gettype iterable returns array, var dump iterable hash prints array shell, and var dump iterable indexed array prints array shell.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies boxed gradual arrays are checked and unboxed at an iterable parameter boundary.
#[test]
fn test_mixed_arrays_bind_to_iterable_parameter() {
    let out = compile_and_run(
        r#"<?php
function choose(bool $assoc): mixed {
    return $assoc ? ['a' => 3, 'b' => 4] : [1, 2];
}

function total(iterable $values): int {
    $sum = 0;
    foreach ($values as $value) {
        $sum += $value;
    }
    return $sum;
}

echo total(choose(false)), ':', total(choose(true));
"#,
    );
    assert_eq!(out, "3:7");
}

/// Verifies a boxed Traversable object is validated and unboxed at an iterable boundary.
#[test]
fn test_mixed_traversable_object_binds_to_iterable_parameter() {
    let out = compile_and_run(
        r#"<?php
function choose(): mixed {
    return new ArrayIterator([5, 6]);
}

function total(iterable $values): int {
    $sum = 0;
    foreach ($values as $value) {
        $sum += $value;
    }
    return $sum;
}

echo total(choose());
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies a non-iterable boxed gradual value raises a runtime type error at the boundary.
#[test]
fn test_mixed_scalar_rejected_by_iterable_parameter() {
    let err = compile_and_run_expect_failure(
        r#"<?php
function choose(): mixed {
    return 42;
}

function total(iterable $values): int {
    return count($values);
}

echo total(choose());
"#,
    );
    assert!(err.contains("Value must be of type iterable, int given"));
}

/// Verifies that `iterable` typed parameter returns "array" from `gettype()` for both hash and indexed arrays.
#[test]
fn test_gettype_iterable_returns_array() {
    let out = compile_and_run(
        "<?php
        function describe(iterable $items): string {
            return gettype($items);
        }
        echo describe(['a' => 1]);
        echo '|';
        echo describe([1, 2, 3]);
        ",
    );
    assert_eq!(out, "array|array");
}

/// `is_array()` and `is_object()` on an `iterable` answer from the VALUE, never from the
/// declaration.
///
/// PHP's `iterable` is `array|Traversable`, so the two predicates split it: `is_array($x)` is true
/// exactly when the value is an array, and `is_object($x)` exactly when it is a Traversable. Both
/// used to be answered statically as `false` for every `iterable`-typed value, with the comment
/// that an iterable "may hold a Traversable" — which made the ARRAY case wrong on the commonest
/// shape there is, a promoted constructor property `private iterable $items = []`:
/// `is_array($this->items)` said `false` while `\count($this->items)` (which has always dispatched
/// on the heap kind) answered `2`, so the two disagreed about one value and a `!is_array(…)` guard
/// took a branch php never takes.
#[test]
fn test_is_array_and_is_object_on_an_iterable_read_the_value() {
    let out = compile_and_run(
        r#"<?php
class Bag implements IteratorAggregate {
    public function __construct(private array $items) {}
    public function getIterator(): Traversable { return new ArrayIterator($this->items); }
}
class Holder {
    public function __construct(private iterable $items = []) {}
    public function probe(): string {
        return var_export(is_array($this->items), true) . '/' . var_export(is_object($this->items), true);
    }
}
function probeParam(iterable $x): string {
    return var_export(is_array($x), true) . '/' . var_export(is_object($x), true);
}
echo (new Holder())->probe(), ' ';
echo (new Holder([1, 2]))->probe(), ' ';
echo (new Holder(['k' => 'v']))->probe(), ' ';
echo (new Holder(new Bag([1])))->probe(), ' ';
echo probeParam([1, 2]), ' ';
echo probeParam(new Bag([1]));
"#,
    );
    assert_eq!(
        out,
        "true/false true/false true/false false/true true/false false/true"
    );
}

/// `gettype()` and `get_debug_type()` on an `iterable` read the value's heap kind for the same
/// reason the predicates above do.
///
/// Both named every `iterable` `array`, which is right for an array and wrong for the Traversable
/// half: php answers `object` / the class name. Measured before the fix, one value reported
/// `is_object=true` and `get_debug_type=array` at the same time.
#[test]
fn test_gettype_and_debug_type_on_an_iterable_read_the_value() {
    let out = compile_and_run(
        r#"<?php
class Bag implements IteratorAggregate {
    public function __construct(private array $items) {}
    public function getIterator(): Traversable { return new ArrayIterator($this->items); }
}
function name(iterable $x): string { return gettype($x) . '/' . get_debug_type($x); }
echo name([1, 2]), ' ';
echo name(['k' => 'v']), ' ';
echo name(new Bag([1]));
"#,
    );
    assert_eq!(out, "array/array array/array object/Bag");
}

/// Verifies `var_dump` on a hash (associative) `iterable` prints the array shell with correct count.
#[test]
fn test_var_dump_iterable_hash_prints_array_shell() {
    let out = compile_and_run(
        "<?php
        function dump(iterable $items): void {
            var_dump($items);
        }
        dump(['a' => 1, 'b' => 2]);
        ",
    );
    assert_eq!(out, "array(2) {\n}\n");
}

/// Verifies `var_dump` on an indexed `iterable` prints the array shell with the correct element count.
#[test]
fn test_var_dump_iterable_indexed_array_prints_array_shell() {
    let out = compile_and_run(
        "<?php
        function dump(iterable $items): void {
            var_dump($items);
        }
        dump([10, 20, 30]);
        ",
    );
    assert_eq!(out, "array(3) {\n}\n");
}

/// Verifies `echo` on an `iterable` parameter prints "Array" for both hash and indexed variants.
#[test]
fn test_echo_iterable_prints_array_literal() {
    let out = compile_and_run(
        "<?php
        function show(iterable $items): void {
            echo $items;
        }
        show(['a' => 1, 'b' => 2]);
        echo '|';
        show([10, 20, 30]);
        ",
    );
    assert_eq!(out, "Array|Array");
}

/// Verifies strict equality (`===`) on two `iterable` parameters reflects pointer identity:
/// same variable is equal, a copy with the same content is not equal.
#[test]
fn test_strict_eq_two_iterables_pointer_identity() {
    let out = compile_and_run(
        "<?php
        function same(iterable $a, iterable $b): bool {
            return $a === $b;
        }
        $h = ['a' => 1];
        echo same($h, $h) ? 'eq' : 'ne';
        echo '|';
        echo same($h, ['a' => 1]) ? 'eq' : 'ne';
        ",
    );
    assert_eq!(out, "eq|ne");
}

/// Verifies that casting an `iterable` to `string` produces "Array" regardless of indexed or hash content.
#[test]
fn test_iterable_string_cast_is_array_literal() {
    let out = compile_and_run(
        "<?php
        function as_str(iterable $items): string {
            return (string)$items;
        }
        echo as_str(['a' => 1]);
        echo '|';
        echo as_str([10, 20]);
        ",
    );
    assert_eq!(out, "Array|Array");
}

/// Verifies `(int)` and `(float)` casts on `iterable` follow PHP's array truthiness: empty array is 0/false,
/// non-empty indexed and associative arrays are 1/true.
#[test]
fn test_iterable_numeric_casts_follow_php_array_truthiness() {
    let out = compile_and_run(
        "<?php
        function as_int(iterable $items): int {
            return (int)$items;
        }
        function as_float(iterable $items): float {
            return (float)$items;
        }
        echo as_int([]);
        echo '|';
        echo as_int([10, 20]);
        echo '|';
        echo as_int(['a' => 1]);
        echo '|';
        echo as_float([]);
        echo '|';
        echo as_float([10, 20]);
        ",
    );
    assert_eq!(out, "0|1|1|0|1");
}

/// Verifies `is_iterable()` predicates at compile time for literal arrays, int, and `iterable` typed parameter.
#[test]
fn test_is_iterable_compile_time_predicates() {
    let out = compile_and_run(
        "<?php
        function check_indexed(): bool { return is_iterable([1, 2, 3]); }
        function check_hash(): bool { return is_iterable(['a' => 1]); }
        function check_int(): bool { return is_iterable(42); }
        function check_iter(iterable $v): bool { return is_iterable($v); }
        echo check_indexed() ? 'y' : 'n';
        echo check_hash() ? 'y' : 'n';
        echo check_int() ? 'y' : 'n';
        echo check_iter([1, 2]) ? 'y' : 'n';
        ",
    );
    assert_eq!(out, "yyny");
}

/// Verifies `is_iterable()` runtime dispatch for a `mixed` parameter: arrays return true, non-arrays return false.
#[test]
fn test_is_iterable_runtime_dispatch_for_mixed() {
    let out = compile_and_run(
        "<?php
        function check(mixed $v): bool {
            return is_iterable($v);
        }
        echo check(['a' => 1]) ? 'y' : 'n';
        echo check([10, 20]) ? 'y' : 'n';
        echo check(42) ? 'y' : 'n';
        echo check('hello') ? 'y' : 'n';
        echo check(null) ? 'y' : 'n';
        ",
    );
    assert_eq!(out, "yynnn");
}

/// Verifies `is_iterable()` returns true for both `Iterator` and `IteratorAggregate` objects when passed
/// to a `mixed` typed function or used directly.
#[test]
fn test_is_iterable_accepts_iterator_objects() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current;
    private int $end;
    public function __construct(int $start, int $end) {
        $this->current = $start;
        $this->end = $end;
    }
    public function rewind(): void {}
    public function valid(): bool { return $this->current < $this->end; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Values implements IteratorAggregate {
    public function getIterator(): Iterator { return new Range(0, 1); }
}
function check(mixed $value): bool {
    return is_iterable($value);
}
echo is_iterable(new Range(0, 1)) ? 'y' : 'n';
echo is_iterable(new Values()) ? 'y' : 'n';
echo check(new Range(0, 1)) ? 'y' : 'n';
echo check(new Values()) ? 'y' : 'n';
"#,
    );
    assert_eq!(out, "yyyy");
}

/// Verifies an `iterable` return value is boxed to `mixed` with a concrete array tag and is still
/// recognized as an iterable; `var_dump` prints the array structure.
#[test]
fn test_iterable_boxes_to_mixed_with_concrete_array_tag() {
    let out = compile_and_run(
        "<?php
        function box(iterable $items): mixed {
            return $items;
        }
        echo is_iterable(box([1, 2])) ? 'y' : 'n';
        echo '|';
        echo gettype(box(['a' => 1]));
        echo '|';
        var_dump(box([10, 20]));
        ",
    );
    // The boxed Mixed carries the concrete array pointer, so the var_dump walk
    // reads the array's real value_type stamp and prints the elements. It used to
    // print the empty `array(2) {\n}\n` shell (the walker assumed boxed cells).
    assert_eq!(out, "y|array|array(2) {\n  [0]=>\n  int(10)\n  [1]=>\n  int(20)\n}\n");
}

/// Verifies `empty()` on an `iterable` uses the underlying array length: empty array is "empty",
/// non-empty indexed and associative arrays are "not".
#[test]
fn test_empty_iterable_uses_underlying_array_length() {
    let out = compile_and_run(
        "<?php
        function describe(iterable $items): string {
            return empty($items) ? 'empty' : 'not';
        }
        echo describe([]);
        echo '|';
        echo describe([1]);
        echo '|';
        echo describe(['a' => 1]);
        ",
    );
    assert_eq!(out, "empty|not|not");
}

/// Verifies that `iterable` cleanup uses the uniform `__rt_decref_any` path (ARM64: `bl __rt_decref_any`,
/// x86_64: `call __rt_decref_any`) by inspecting the emitted assembly for both targets.
#[test]
fn test_iterable_cleanup_uses_uniform_decref_dispatch() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, _) = compile_source_to_asm_with_options(
        "<?php
        function hold(iterable $items): void {
            $copy = $items;
            echo 'ok';
        }
        hold([1, 2]);
        ",
        &dir,
        8_388_608,
        false,
        false,
    );
    match target().arch {
        Arch::AArch64 => assert!(user_asm.contains("bl __rt_decref_any"), "{user_asm}"),
        Arch::X86_64 => assert!(user_asm.contains("call __rt_decref_any"), "{user_asm}"),
    }

    let _ = fs::remove_dir_all(&dir);
}
