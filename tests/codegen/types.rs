use crate::support::*;

#[test]
fn test_backed_enum_value_and_from_identity() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
            case Green = 2;
            case Blue = 3;
        }
        echo Color::Red->value;
        echo PHP_EOL;
        $c = Color::from(2);
        echo $c === Color::Green;
        ",
    );
    assert_eq!(out, "1\n1");
}

#[test]
fn test_enum_try_from_and_cases() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
            case Green = 2;
        }
        $picked = Color::tryFrom(99) ?? Color::Red;
        echo $picked === Color::Red;
        echo PHP_EOL;
        $cases = Color::cases();
        echo count($cases);
        echo PHP_EOL;
        echo $cases[1] === Color::Green;
        ",
    );
    assert_eq!(out, "1\n2\n1");
}

#[test]
fn test_string_backed_enum_from_and_value() {
    let out = compile_and_run(
        "<?php
        enum Status: string {
            case Draft = \"draft\";
            case Live = \"live\";
        }
        echo Status::from(\"live\") === Status::Live;
        echo PHP_EOL;
        echo Status::Live->value;
        ",
    );
    assert_eq!(out, "1\nlive");
}

#[test]
fn test_pure_enum_cases_identity() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts;
            case Clubs;
        }
        $cases = Suit::cases();
        echo count($cases);
        echo PHP_EOL;
        echo $cases[0] === Suit::Hearts;
        ",
    );
    assert_eq!(out, "2\n1");
}

#[test]
fn test_enum_from_failure_is_fatal() {
    let err = compile_and_run_expect_failure(
        "<?php
        enum Color: int {
            case Red = 1;
        }
        Color::from(99);
        ",
    );
    assert!(err.contains("Fatal error: enum case not found"));
}

#[test]
fn test_example_enums_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/enums/main.php"));
    assert_eq!(out, "1\n2\n3");
}

#[test]
fn test_enum_try_from_is_null_on_missing_value() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
            case Green = 2;
        }
        echo is_null(Color::tryFrom(2)) ? \"null\" : \"found\";
        echo \"|\";
        echo is_null(Color::tryFrom(99)) ? \"null\" : \"found\";
        ",
    );
    assert_eq!(out, "found|null");
}

#[test]
fn test_enum_try_from_is_null_through_nullable_variable() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
        }
        $missing = Color::tryFrom(99);
        $present = Color::tryFrom(1);
        echo is_null($missing) ? \"null\" : \"found\";
        echo \"|\";
        echo is_null($present) ? \"null\" : \"found\";
        ",
    );
    assert_eq!(out, "null|found");
}

#[test]
fn test_nullable_enum_typed_local_accepts_try_from_result() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
        }
        ?Color $missing = Color::tryFrom(99);
        ?Color $present = Color::tryFrom(1);
        echo is_null($missing) ? \"null\" : \"found\";
        echo \"|\";
        echo $present === Color::Red ? \"red\" : \"other\";
        ",
    );
    assert_eq!(out, "null|red");
}

#[test]
fn test_example_union_types_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/union-types/main.php"));
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
fn test_named_arguments_reorder_function_call() {
    let out = compile_and_run(
        "<?php
        function describe($name, $age) {
            echo $name;
            echo \":\";
            echo $age;
        }
        describe(age: 30, name: \"Alice\");
        ",
    );
    assert_eq!(out, "Alice:30");
}

#[test]
fn test_named_arguments_use_defaults_for_missing_params() {
    let out = compile_and_run(
        "<?php
        function greet($name = \"world\", $suffix = \"!\") {
            echo $name . $suffix;
        }
        greet(suffix: \"?\");
        ",
    );
    assert_eq!(out, "world?");
}

#[test]
fn test_named_arguments_closure_call() {
    let out = compile_and_run(
        "<?php
        $f = function ($name, $age) {
            echo $name;
            echo \":\";
            echo $age;
        };
        $f(age: 30, name: \"Alice\");
        ",
    );
    assert_eq!(out, "Alice:30");
}

#[test]
fn test_named_arguments_first_class_callable_call() {
    let out = compile_and_run(
        "<?php
        function describe($name, $age) {
            echo $name;
            echo \":\";
            echo $age;
        }
        $f = describe(...);
        $f(age: 30, name: \"Alice\");
        ",
    );
    assert_eq!(out, "Alice:30");
}

#[test]
fn test_named_arguments_method_and_constructor_calls() {
    let out = compile_and_run(
        "<?php
        class User {
            public $name;
            public $age;

            public function __construct($name, $age = 18) {
                $this->name = $name;
                $this->age = $age;
            }

            public function describe($prefix, $suffix = \"!\") {
                echo $prefix . $this->name . \":\" . $this->age . $suffix;
            }
        }

        $user = new User(age: 30, name: \"Alice\");
        $user->describe(suffix: \"?\", prefix: \"user=\");
        ",
    );
    assert_eq!(out, "user=Alice:30?");
}

#[test]
fn test_named_arguments_static_method_call() {
    let out = compile_and_run(
        "<?php
        class Greeter {
            public static function hi($name, $punct = \"!\") {
                echo \"Hi \" . $name . $punct;
            }
        }
        Greeter::hi(punct: \"?\", name: \"Alice\");
        ",
    );
    assert_eq!(out, "Hi Alice?");
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

#[test]
fn test_example_functions_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/functions/main.php"));
    assert_eq!(
        out,
        "my_abs(-42) = 42\nmy_max(3, 7) = 7\nclamp(15, 0, 10) = 10\ngcd(48, 18) = 6\n2^10 = 1024\ndescribe(42) = integer:42\ndescribe(null) = NULL:null\nadd_ten() = 20\nprofile(age: 30, name: \"Ada\") = Ada:30\n",
    );
}
