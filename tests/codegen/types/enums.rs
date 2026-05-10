//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types enums, including backed enum value and from identity, enum try from and cases, and backed enum from and value.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

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
    let out = compile_and_run(include_str!("../../../examples/enums/main.php"));
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
