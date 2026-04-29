use super::*;

#[test]
fn test_example_union_types_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/union-types/main.php"));
    assert_eq!(out, "41:string:ready");
}

#[test]
fn test_typed_array_parameter() {
    let out = compile_and_run(
        "<?php
        function total(array $values) {
            echo count($values);
        }
        total([1, 2, 3]);
        ",
    );
    assert_eq!(out, "3");
}

#[test]
fn test_typed_callable_parameter() {
    let out = compile_and_run(
        "<?php
        function apply(callable $fn) {
            echo $fn(1);
        }
        function plus_one($x) {
            return $x + 1;
        }
        apply(plus_one(...));
        ",
    );
    assert_eq!(out, "2");
}

#[test]
fn test_typed_by_ref_parameter() {
    let out = compile_and_run(
        "<?php
        function bump(int &$x) {
            $x = $x + 1;
        }
        $value = 4;
        bump($value);
        echo $value;
        ",
    );
    assert_eq!(out, "5");
}

#[test]
fn test_typed_method_parameter() {
    let out = compile_and_run(
        "<?php
        class Box {
            public function size(array $items) {
                echo count($items);
            }
        }
        $box = new Box();
        $box->size([1, 2]);
        ",
    );
    assert_eq!(out, "2");
}

#[test]
fn test_typed_constructor_parameter() {
    let out = compile_and_run(
        "<?php
        class User {
            public $id;
            public function __construct(int $id) {
                $this->id = $id;
            }
        }
        $user = new User(42);
        echo $user->id;
        ",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_typed_default_parameter_uses_default() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo add_ten();
        ",
    );
    assert_eq!(out, "20");
}

#[test]
fn test_typed_default_parameter_override() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo add_ten(5);
        ",
    );
    assert_eq!(out, "15");
}

#[test]
fn test_typed_closure_default_parameter() {
    let out = compile_and_run(
        "<?php
        $f = function (int $value = 10) {
            return $value + 1;
        };
        echo $f();
        echo \"|\";
        echo $f(4);
        ",
    );
    assert_eq!(out, "11|5");
}

#[test]
fn test_typed_first_class_callable_default_parameter() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        $f = add_ten(...);
        echo $f();
        echo \"|\";
        echo $f(7);
        ",
    );
    assert_eq!(out, "20|17");
}

#[test]
fn test_typed_call_user_func_default_parameter() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo call_user_func(add_ten(...));
        echo \"|\";
        echo call_user_func(\"add_ten\", 5);
        ",
    );
    assert_eq!(out, "20|15");
}

#[test]
fn test_typed_call_user_func_array_default_parameter() {
    let out = compile_and_run(
        "<?php
        function add_ten(int $value = 10): int {
            return $value + 10;
        }
        echo call_user_func_array(add_ten(...), []);
        echo \"|\";
        echo call_user_func_array(\"add_ten\", [5]);
        ",
    );
    assert_eq!(out, "20|15");
}

#[test]
fn test_typed_closure_parameter() {
    let out = compile_and_run(
        "<?php
        $f = function (int $x) {
            echo $x + 1;
        };
        $f(41);
        ",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_typed_function_return_value() {
    let out = compile_and_run(
        "<?php
        function label(): string {
            return \"ok\";
        }
        echo label();
        ",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_nullable_typed_parameter_accepts_null_and_int() {
    let out = compile_and_run(
        "<?php
        function show(?int $value): string {
            return is_null($value) ? \"null\" : (string) $value;
        }
        echo show(null);
        echo \"|\";
        echo show(7);
        ",
    );
    assert_eq!(out, "null|7");
}

#[test]
fn test_union_typed_parameter_accepts_multiple_types() {
    let out = compile_and_run(
        "<?php
        function show(int|string $value): string {
            return gettype($value) . \":\" . $value;
        }
        echo show(1);
        echo \"|\";
        echo show(\"ok\");
        ",
    );
    assert_eq!(out, "integer:1|string:ok");
}

#[test]
fn test_nullable_return_type_boxes_results() {
    let out = compile_and_run(
        "<?php
        function maybe(bool $flag): ?int {
            if ($flag) {
                return 7;
            }
            return null;
        }
        echo is_null(maybe(false)) ? \"null\" : \"value\";
        echo \"|\";
        echo maybe(true);
        ",
    );
    assert_eq!(out, "null|7");
}

#[test]
fn test_union_return_type_boxes_results() {
    let out = compile_and_run(
        "<?php
        function choose(bool $flag): int|string {
            if ($flag) {
                return 7;
            }
            return \"ok\";
        }
        echo gettype(choose(true));
        echo \"|\";
        echo choose(false);
        ",
    );
    assert_eq!(out, "integer|ok");
}

#[test]
fn test_mixed_parameter_and_return_type() {
    let out = compile_and_run(
        "<?php
        function id(mixed $value): mixed {
            return $value;
        }
        echo gettype(id(\"ok\"));
        echo \"|\";
        echo id(7);
        ",
    );
    assert_eq!(out, "string|7");
}

#[test]
fn test_call_user_func_array_with_nullable_callback_param() {
    let out = compile_and_run(
        "<?php
        function show(?int $value): string {
            return is_null($value) ? \"null\" : (string) $value;
        }
        echo call_user_func_array(show(...), [null]);
        echo \"|\";
        echo call_user_func_array(show(...), [7]);
        ",
    );
    assert_eq!(out, "null|7");
}

#[test]
fn test_nullable_by_ref_parameter_accepts_boxed_typed_local() {
    let out = compile_and_run(
        "<?php
        function clear(?int &$value): void {
            $value = null;
        }
        ?int $value = 7;
        clear($value);
        echo is_null($value) ? \"null\" : \"value\";
        ",
    );
    assert_eq!(out, "null");
}
