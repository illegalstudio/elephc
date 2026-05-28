//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, iterable builtins and casts, including gettype iterable returns array, var dump iterable hash prints array shell, and var dump iterable indexed array prints array shell.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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
    assert_eq!(out, "y|array|array(2) {\n}\n");
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
