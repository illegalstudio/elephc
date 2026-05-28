//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, iterable foreach, including iterable as parameter and return type, foreach over iterable hash emits keys and values, and foreach over iterable indexed emits keys and values.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `iterable` can be used as a parameter and return type; an array passed through an
/// `identity(iterable $values): iterable` function is returned and `is_iterable()` confirms it.
#[test]
fn test_iterable_as_parameter_and_return_type() {
    let out = compile_and_run(
        "<?php
        function identity(iterable $values): iterable {
            return $values;
        }
        echo is_null(identity([1, 2])) ? 'null' : 'ok';
        ",
    );
    assert_eq!(out, "ok");
}

/// Verifies `foreach` over a hash (associative array) via a `iterable` parameter emits correct
/// string keys and values; `dump(['a' => 1, 'b' => 2, 'c' => 3])` outputs "a=1;b=2;c=3;".
#[test]
fn test_foreach_over_iterable_hash_emits_keys_and_values() {
    let out = compile_and_run(
        "<?php
        function dump(iterable $items): void {
            foreach ($items as $k => $v) {
                echo $k;
                echo '=';
                echo $v;
                echo ';';
            }
        }
        dump(['a' => 1, 'b' => 2, 'c' => 3]);
        ",
    );
    assert_eq!(out, "a=1;b=2;c=3;");
}

/// Verifies `foreach` over an indexed array via a `iterable` parameter emits correct integer keys
/// and values; `dump([10, 20, 30])` outputs "0=10;1=20;2=30;".
#[test]
fn test_foreach_over_iterable_indexed_emits_keys_and_values() {
    let out = compile_and_run(
        "<?php
        function dump(iterable $items): void {
            foreach ($items as $k => $v) {
                echo $k;
                echo '=';
                echo $v;
                echo ';';
            }
        }
        dump([10, 20, 30]);
        ",
    );
    assert_eq!(out, "0=10;1=20;2=30;");
}

/// Verifies `foreach` over an indexed string array via a `iterable` parameter uses runtime slot-width
/// integer keys; `dump(['red', 'blue'])` outputs "0:red;1:blue;".
#[test]
fn test_foreach_over_iterable_indexed_strings_uses_runtime_slot_width() {
    let out = compile_and_run(
        "<?php
        function dump(iterable $items): void {
            foreach ($items as $k => $v) {
                echo $k;
                echo ':';
                echo $v;
                echo ';';
            }
        }
        dump(['red', 'blue']);
        ",
    );
    assert_eq!(out, "0:red;1:blue;");
}

/// Verifies `foreach` over an untyped parameter receiving a `mixed` array works correctly;
/// `passthrough([1, 2, 3])` is passed to `dump($items)` and each value is echoed.
#[test]
fn test_foreach_over_untyped_parameter_with_mixed_runtime_array() {
    let out = compile_and_run(
        "<?php
        function passthrough(mixed $value): mixed {
            return $value;
        }
        function dump($items): void {
            foreach ($items as $value) {
                echo $value;
                echo ';';
            }
        }
        dump(passthrough([1, 2, 3]));
        ",
    );
    assert_eq!(out, "1;2;3;");
}

/// Verifies `foreach` over a `mixed` parameter containing an associative array with mixed types
/// outputs correct keys and string/integer values.
#[test]
fn test_foreach_over_mixed_parameter_assoc_array() {
    let out = compile_and_run(
        "<?php
        function dump(mixed $items): void {
            foreach ($items as $key => $value) {
                echo $key;
                echo '=';
                echo $value;
                echo ';';
            }
        }
        dump(['a' => 1, 'b' => 'two']);
        ",
    );
    assert_eq!(out, "a=1;b=two;");
}

/// Verifies `foreach` over a `mixed` result from `json_decode` with `true` (assoc) produces correct
/// integer keys and values; outputs "0=10;1=20;".
#[test]
fn test_foreach_over_mixed_json_decode_indexed_array() {
    let out = compile_and_run(
        r#"<?php
        $items = json_decode("[10, 20]", true);
        foreach ($items as $key => $value) {
            echo $key;
            echo '=';
            echo $value;
            echo ';';
        }
        "#,
    );
    assert_eq!(out, "0=10;1=20;");
}

/// Verifies `foreach` over a union-typed `array|bool` parameter dispatches to the array branch at runtime;
/// `dump(choose(true))` outputs "0:4;1:5;" where `choose` returns `[4, 5]`.
#[test]
fn test_foreach_over_union_parameter_array_runtime_value() {
    let out = compile_and_run(
        "<?php
        function choose(bool $flag): array|bool {
            if ($flag) {
                return [4, 5];
            }
            return false;
        }
        function dump(array|bool $items): void {
            foreach ($items as $key => $value) {
                echo $key;
                echo ':';
                echo $value;
                echo ';';
            }
        }
        dump(choose(true));
        ",
    );
    assert_eq!(out, "0:4;1:5;");
}

/// Verifies by-ref `foreach` over a `mixed` indexed array mutates the source array;
/// `[1, 2, 3]` becomes `[7, 7, 7]` after the by-ref loop.
#[test]
fn test_foreach_by_ref_over_mixed_indexed_array_updates_source() {
    let out = compile_and_run(
        "<?php
        function rewrite(mixed $items): void {
            foreach ($items as &$value) {
                $value = 7;
            }
            foreach ($items as $value) {
                echo $value;
                echo ';';
            }
        }
        rewrite([1, 2, 3]);
        ",
    );
    assert_eq!(out, "7;7;7;");
}

/// Verifies by-ref `foreach` over a `mixed` associative array mutates source values;
/// `['a' => 1, 'b' => 2]` becomes `['a' => 9, 'b' => 9]` after the loop.
#[test]
fn test_foreach_by_ref_over_mixed_assoc_array_updates_source() {
    let out = compile_and_run(
        "<?php
        function rewrite(mixed $items): void {
            foreach ($items as $key => &$value) {
                $value = 9;
            }
            foreach ($items as $key => $value) {
                echo $key;
                echo '=';
                echo $value;
                echo ';';
            }
        }
        rewrite(['a' => 1, 'b' => 2]);
        ",
    );
    assert_eq!(out, "a=9;b=9;");
}

/// Verifies that `unset($inner)` after a nested by-ref `foreach` resets its lifetime so the outer
/// `$v *= 2` mutation applies to the correct variable; outputs "42,4,6,".
#[test]
fn test_nested_by_ref_foreach_unset_inner_lifetime_reset() {
    let out = compile_and_run(
        "<?php
        $a = [1, 2, 3];
        foreach ($a as &$v) {
            foreach ($a as &$inner) {
                $inner += 10;
                break;
            }
            unset($inner);
            $v *= 2;
        }
        unset($v);
        foreach ($a as $x) {
            echo $x . ',';
        }
        ",
    );
    assert_eq!(out, "42,4,6,");
}

/// Verifies by-ref `foreach` on `mixed` with copy-on-write semantics: mutating `$b` via by-ref foreach
/// does not affect `$a`, but mutating `$c` does (COW split); outputs "a,b|a,b|a!,b!".
#[test]
fn test_mixed_by_ref_foreach_cow_split_preserves_aliases() {
    let out = compile_and_run(
        "<?php
        function mutate(mixed $x): mixed {
            foreach ($x as &$v) {
                $v .= '!';
            }
            unset($v);
            return $x;
        }
        $a = ['a', 'b'];
        $b = $a;
        $c = mutate($b);
        echo implode(',', $a) . '|' . implode(',', $b) . '|' . implode(',', $c);
        ",
    );
    assert_eq!(out, "a,b|a,b|a!,b!");
}

/// Verifies by-ref `foreach` over a `json_decode` assoc payload nested in a `mixed` container
/// mutates the source data; outputs "101|102" after adding 100 to each `n` field.
#[test]
fn test_by_ref_foreach_nested_json_decode_assoc_payloads() {
    let out = compile_and_run(
        r#"<?php
        $data = json_decode('{"rows":[{"n":1},{"n":2}]}', true);
        foreach ($data["rows"] as &$row) {
            $row["n"] = $row["n"] + 100;
        }
        unset($row);
        echo $data["rows"][0]["n"] . "|" . $data["rows"][1]["n"];
        "#,
    );
    assert_eq!(out, "101|102");
}

/// Verifies that a fatal error during `foreach` over a non-iterable `mixed` preserves prior side effects;
/// "S" is echoed before the fatal error, confirming `side()` ran before the foreach check.
#[test]
fn test_mixed_foreach_fatal_preserves_prior_side_effects() {
    let out = compile_and_run_capture(
        "<?php
        function side(): mixed {
            echo 'S';
            return 42;
        }
        $x = side();
        foreach ($x as $v) {
            echo $v;
        }
        ",
    );
    assert!(!out.success, "program unexpectedly succeeded");
    assert_eq!(out.stdout, "S");
    assert!(
        out.stderr
            .contains("Fatal error: foreach over iterable with unsupported kind"),
        "{}",
        out.stderr
    );
}

/// Verifies `foreach` over an `iterable`-typed `Iterator` implementation emits the correct keys and
/// values; `dump(new Range(2, 5))` outputs "0=2;1=3;2=4;" (key starts at 0 regardless of `current()`).
#[test]
fn test_foreach_over_iterable_iterator_object() {
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
    public function key(): int { return $this->current - 2; }
    public function next(): void { $this->current = $this->current + 1; }
}
function dump(iterable $items): void {
    foreach ($items as $k => $v) {
        echo $k;
        echo '=';
        echo $v;
        echo ';';
    }
}
dump(new Range(2, 5));
"#,
    );
    assert_eq!(out, "0=2;1=3;2=4;");
}

/// Verifies `foreach` over an `iterable`-typed `IteratorAggregate` implementation correctly delegates
/// to its `getIterator()` result; outputs "012".
#[test]
fn test_foreach_over_iterable_iterator_aggregate_object() {
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
    public function getIterator(): Iterator { return new Range(0, 3); }
}
function dump(iterable $items): void {
    foreach ($items as $v) {
        echo $v;
    }
}
dump(new Values());
"#,
    );
    assert_eq!(out, "012");
}

/// Verifies `getIterator()` on an `IteratorAggregate` is called exactly once per foreach iteration;
/// "G0011" confirms `getIterator()` echoes "G" once and the loop runs twice with keys "00" and "11".
#[test]
fn test_iterator_aggregate_get_iterator_side_effect_runs_once() {
    let out = compile_and_run(
        r#"<?php
class It implements Iterator {
    private int $i = 0;
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): mixed { return $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
class Bag implements IteratorAggregate {
    public function getIterator(): Iterator {
        echo "G";
        return new It();
    }
}
foreach (new Bag() as $k => $v) {
    echo $k . $v;
}
"#,
    );
    assert_eq!(out, "G0011");
}

/// Verifies `foreach` over a `iterable`-typed `Iterator` object can reuse the receiver variable name;
/// `consume(new Range(0, 3))` with `$items` as both the iterable and loop variable outputs "012".
#[test]
fn test_foreach_over_iterable_iterator_can_reuse_receiver_variable() {
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
function consume(iterable $items): void {
    foreach ($items as $items) {
        echo $items;
    }
}
consume(new Range(0, 3));
"#,
    );
    assert_eq!(out, "012");
}

/// Verifies `foreach` over an empty `iterable`-typed `Iterator` preserves the existing value of the
/// receiver variable; `consume(new EmptyIteratorImpl())` echoes "old".
#[test]
fn test_foreach_over_empty_iterable_iterator_preserves_existing_value_variable() {
    let out = compile_and_run(
        r#"<?php
class EmptyIteratorImpl implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return false; }
    public function current(): int { return 1; }
    public function key(): int { return 2; }
    public function next(): void {}
}
function consume(iterable $items): void {
    $value = 'old';
    foreach ($items as $value) {
    }
    echo $value;
}
consume(new EmptyIteratorImpl());
"#,
    );
    assert_eq!(out, "old");
}

/// Verifies `foreach` over an indexed `iterable` can reuse the receiver variable name as the value variable;
/// `consume([10, 20, 30])` with `as $items` outputs "10;20;30;".
#[test]
fn test_foreach_over_iterable_indexed_can_reuse_receiver_variable() {
    let out = compile_and_run(
        "<?php
        function consume(iterable $items): void {
            foreach ($items as $items) {
                echo $items;
                echo ';';
            }
        }
        consume([10, 20, 30]);
        ",
    );
    assert_eq!(out, "10;20;30;");
}

/// Verifies `foreach` over an associative `iterable` can reuse the receiver variable as the key;
/// `consume(['a' => 1, 'b' => 2])` with `as $items => $value` outputs "a=1;b=2;".
#[test]
fn test_foreach_over_iterable_assoc_key_can_reuse_receiver_variable() {
    let out = compile_and_run(
        "<?php
        function consume(iterable $items): void {
            foreach ($items as $items => $value) {
                echo $items;
                echo '=';
                echo $value;
                echo ';';
            }
        }
        consume(['a' => 1, 'b' => 2]);
        ",
    );
    assert_eq!(out, "a=1;b=2;");
}

/// Verifies the loop key from `foreach` over an `iterable` remains `mixed` and retains the last key
/// value after the loop; `last_key(['a' => 1])` outputs "a" and `last_key([10, 20])` outputs "1".
#[test]
fn test_iterable_foreach_key_remains_mixed_after_runtime_branch() {
    let out = compile_and_run(
        "<?php
        function last_key(iterable $items): void {
            foreach ($items as $k => $v) {
            }
            echo $k;
        }
        last_key(['a' => 1]);
        echo '|';
        last_key([10, 20]);
        ",
    );
    assert_eq!(out, "a|1");
}

/// Verifies that a `mixed` iterable containing an inner indexed array (boxed via `iterable` return)
/// preserves `is_iterable() == true` inside the outer foreach; the inner array is not flattened.
#[test]
fn test_iterable_value_in_indexed_array_stays_boxed() {
    let out = compile_and_run(
        "<?php
        function id(iterable $items): iterable {
            return $items;
        }
        function show(iterable $items): void {
            foreach ($items as $value) {
                echo is_iterable($value) ? gettype($value) : 'no';
                echo ':';
                var_dump($value);
            }
        }
        show([id([1, 2])]);
        ",
    );
    assert_eq!(out, "array:array(2) {\n}\n");
}

/// Verifies that a `mixed` iterable containing an inner associative array preserves
/// `is_iterable() == true` inside the outer foreach; the inner array is not flattened.
#[test]
fn test_iterable_value_in_assoc_array_stays_boxed() {
    let out = compile_and_run(
        "<?php
        function id(iterable $items): iterable {
            return $items;
        }
        $items = ['inner' => id([1, 2])];
        foreach ($items as $value) {
            echo is_iterable($value) ? gettype($value) : 'no';
            echo ':';
            var_dump($value);
        }
        ",
    );
    assert_eq!(out, "array:array(2) {\n}\n");
}

/// Verifies an inner `iterable` array appended to a plain array stays boxed and `is_iterable()` is
/// true for it inside the outer foreach.
#[test]
fn test_iterable_value_appended_to_array_stays_boxed() {
    let out = compile_and_run(
        "<?php
        function id(iterable $items): iterable {
            return $items;
        }
        $items = [];
        $items[] = id(['a' => 1]);
        foreach ($items as $value) {
            echo is_iterable($value) ? gettype($value) : 'no';
            echo ':';
            var_dump($value);
        }
        ",
    );
    assert_eq!(out, "array:array(1) {\n}\n");
}

/// Verifies a `mixed` variadic parameter receiving an `iterable` value preserves it as a boxed array
/// inside the runtime variadic array; `collect(id([1, 2]))` outputs "[[1,2]]" via `json_encode`.
#[test]
fn test_iterable_variadic_arg_stays_boxed_in_runtime_array() {
    let out = compile_and_run(
        "<?php
        function id(iterable $items): iterable {
            return $items;
        }
        function collect(...$items): void {
            echo json_encode($items);
        }
        collect(id([1, 2]));
        ",
    );
    assert_eq!(out, "[[1,2]]");
}
