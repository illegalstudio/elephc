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
fn test_foreach_over_iterable_indexed_runtime_error() {
    let stderr = compile_and_run_expect_failure(
        "<?php
        function dump(iterable $items): void {
            foreach ($items as $v) {
                echo $v;
            }
        }
        dump([10, 20, 30]);
        ",
    );
    assert!(
        stderr.contains("foreach over iterable with non-hash kind"),
        "expected non-hash iterable runtime error, got: {}",
        stderr
    );
}

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
