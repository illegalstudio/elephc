//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types enums, including backed enum value and from identity, enum try from and cases, and backed enum from and value.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Verifies backed enum with `int` underlying type: `->value` returns the integer case value
/// and `Color::from(2)` resolves to `Color::Green` by identity comparison.
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

/// Verifies `Color::tryFrom(99)` returns `null` for an unknown value (with null coalescing to `Color::Red`),
/// `Color::cases()` returns all cases, and case index `1` is `Color::Green` by identity.
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

/// Verifies string-backed enum: `Status::from("live")` resolves to `Status::Live` by identity,
/// and `Status::Live->value` returns the `"live"` string.
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

/// Verifies pure (unit) enum: `Suit::cases()` returns all cases and `Suit::Hearts === $cases[0]` by identity.
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

/// Verifies that `Color::from(99)` throws a catchable `ValueError` with PHP's
/// invalid backing-value message.
#[test]
fn test_enum_from_int_failure_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Color: int {
            case Red = 1;
        }
        try {
            Color::from(99);
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "ValueError:99 is not a valid backing value for enum Color"
    );
}

/// Verifies that a missing string-backed enum value is quoted in the catchable
/// `ValueError` message, matching PHP's backed-enum contract.
#[test]
fn test_enum_from_string_failure_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Status: string {
            case Draft = \"draft\";
        }
        try {
            Status::from(\"live\");
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "ValueError:\"live\" is not a valid backing value for enum Status"
    );
}

/// Compiles and runs the checked-in `examples/enums/main.php` fixture and asserts stdout includes
/// both user-declared enum output and the builtin `SortDirection` helper result.
#[test]
fn test_example_enums_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/enums/main.php"));
    assert_eq!(out, "1\n2\n3\nDESC");
}

/// Verifies `Color::tryFrom(2)` returns a non-null value and `Color::tryFrom(99)` returns `null`,
/// confirmed with `is_null()`.
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

/// Verifies `Color::tryFrom(99)` stored in an untyped variable is `null` and `Color::tryFrom(1)` is not null,
/// confirmed through separate variables echoed via `is_null()`.
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

/// Verifies `Color::tryFrom(99)` stored in a nullable typed local `?Color` is `null`
/// and `Color::tryFrom(1)` in `?Color` resolves to `Color::Red`.
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

/// Verifies namespaced enum case resolution: `RenderMode::Split` is accessible directly in
/// the namespace where it is declared, via `use` imports, and through a static method that
/// receives it as a parameter. Asserts both "local" and "import" paths work correctly.
#[test]
fn test_namespaced_enum_cases_resolve_inside_namespace_and_imports() {
    let out = compile_and_run(
        r#"<?php
namespace Showcases\Doom\App;

enum RenderMode {
    case Map2D;
    case World3D;
    case Split;
}

class Config {
    public static function defaultMode() {
        return RenderMode::Split;
    }
}

namespace Showcases\Doom\Render;

use Showcases\Doom\App\RenderMode;

class Renderer {
    public static function isSplit($mode): bool {
        return $mode === RenderMode::Split;
    }
}

namespace Showcases\Doom;

use Showcases\Doom\App\Config;
use Showcases\Doom\App\RenderMode;
use Showcases\Doom\Render\Renderer;

echo Config::defaultMode() === RenderMode::Split ? "local" : "bad";
echo "|";
echo Renderer::isSplit(RenderMode::Split) ? "import" : "bad";
"#,
    );
    assert_eq!(out, "local|import");
}

/// Verifies PHP 8.6's builtin `SortDirection` unit enum exposes both singleton
/// cases through direct access, `cases()`, `enum_exists()`, and class-like introspection.
#[test]
fn test_builtin_sort_direction_cases_and_introspection() {
    let out = compile_and_run(
        "<?php
        $cases = SortDirection::cases();
        echo count($cases);
        echo '|';
        echo $cases[0] === SortDirection::Ascending ? 'A' : 'bad';
        echo '|';
        echo $cases[1] === SortDirection::Descending ? 'D' : 'bad';
        echo '|';
        echo enum_exists('sortdirection', false) ? 'enum' : 'missing';
        echo '|';
        echo class_exists('SortDirection', false) ? 'class' : 'missing';
        ",
    );
    assert_eq!(out, "2|A|D|enum|class");
}

/// Verifies builtin `SortDirection` can be used in parameter and return type
/// declarations and can drive a `match` expression over enum case singletons.
#[test]
fn test_builtin_sort_direction_typed_function_return_and_match() {
    let out = compile_and_run(
        "<?php
        function default_direction(): SortDirection {
            return SortDirection::Ascending;
        }

        function sort_keyword(SortDirection $direction): string {
            return match ($direction) {
                SortDirection::Ascending => 'ASC',
                SortDirection::Descending => 'DESC',
            };
        }

        echo sort_keyword(default_direction());
        echo '|';
        echo sort_keyword(SortDirection::Descending);
        ",
    );
    assert_eq!(out, "ASC|DESC");
}

/// Verifies namespaced code can reference the global builtin `SortDirection`
/// through PHP class-like name rules: imports and fully-qualified names work.
#[test]
fn test_builtin_sort_direction_resolves_from_namespaced_code() {
    let out = compile_and_run(
        r#"<?php
namespace App;

use SortDirection;

function is_ascending(SortDirection $direction): bool {
    return $direction === \SortDirection::Ascending;
}

echo is_ascending(SortDirection::Ascending) ? "import" : "bad";
echo "|";
echo \SortDirection::Descending === SortDirection::Descending ? "fqcn" : "bad";
"#,
    );
    assert_eq!(out, "import|fqcn");
}

/// Verifies builtin `SortDirection` cases can be used as enum case constants
/// and then compared by singleton identity.
#[test]
fn test_builtin_sort_direction_case_constant() {
    let out = compile_and_run(
        "<?php
        const DEFAULT_DIRECTION = SortDirection::Descending;
        echo DEFAULT_DIRECTION === SortDirection::Descending ? 'ok' : 'bad';
        ",
    );
    assert_eq!(out, "ok");
}
