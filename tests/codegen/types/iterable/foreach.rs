//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types, iterable foreach, including iterable as parameter and return type, foreach over iterable hash emits keys and values, and foreach over iterable indexed emits keys and values.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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

