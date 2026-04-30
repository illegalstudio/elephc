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
