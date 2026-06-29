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
/// user-declared enum output, the `->name`/`->value` case introspection loop, and the builtin
/// `SortDirection` helper result.
#[test]
fn test_example_enums_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/enums/main.php"));
    assert_eq!(out, "1\n2\n3\nRed=1 Green=2 Blue=3 \nDESC");
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

/// Verifies that a pure enum can declare an instance method that uses `$this` for identity.
#[test]
fn test_enum_instance_method() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts;
            case Spades;
            public function label(): string {
                return $this === Suit::Hearts ? \"H\" : \"S\";
            }
        }
        echo Suit::Hearts->label() . Suit::Spades->label();
        ",
    );
    assert_eq!(out, "HS");
}

/// Verifies that an enum instance method can `match` on `$this` (the canonical enum-method form).
#[test]
fn test_enum_method_match_on_this() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts; case Diamonds; case Clubs; case Spades;
            public function color(): string {
                return match($this) {
                    Suit::Hearts, Suit::Diamonds => \"red\",
                    Suit::Clubs, Suit::Spades => \"black\",
                };
            }
        }
        echo Suit::Hearts->color() . \"-\" . Suit::Spades->color();
        ",
    );
    assert_eq!(out, "red-black");
}

/// Verifies that a backed enum method can read `$this->value`.
#[test]
fn test_enum_method_reads_backing_value() {
    let out = compile_and_run(
        "<?php
        enum Power: int {
            case Low = 1;
            case High = 10;
            public function doubled(): int { return $this->value * 2; }
        }
        echo Power::High->doubled();
        ",
    );
    assert_eq!(out, "20");
}

/// Verifies that a static enum method (a factory) dispatches and returns a case.
#[test]
fn test_enum_static_method() {
    let out = compile_and_run(
        "<?php
        enum Color {
            case Red;
            case Green;
            public static function fallback(): self { return Color::Red; }
        }
        echo Color::fallback() === Color::Red ? \"ok\" : \"no\";
        ",
    );
    assert_eq!(out, "ok");
}

/// Verifies that an enum can implement an interface and be used through it.
#[test]
fn test_enum_implements_interface() {
    let out = compile_and_run(
        "<?php
        interface HasLabel { public function label(): string; }
        enum Suit implements HasLabel {
            case Hearts;
            public function label(): string { return \"hearts\"; }
        }
        function describe(HasLabel $h): string { return $h->label(); }
        echo describe(Suit::Hearts);
        ",
    );
    assert_eq!(out, "hearts");
}

/// Verifies that an enum instance method can read `$this->name` (and `$this->value`), dispatching
/// on the case singleton. Previously `$this->name` inside a method was unsupported.
#[test]
fn test_enum_method_reads_this_name() {
    let out = compile_and_run(
        "<?php
        enum Suit: string {
            case Hearts = \"h\";
            case Spades = \"s\";
            public function describe(): string {
                return $this->name . \"=\" . $this->value;
            }
        }
        echo Suit::Hearts->describe();
        echo PHP_EOL;
        echo Suit::Spades->describe();
        ",
    );
    assert_eq!(out, "Hearts=h\nSpades=s");
}

/// Verifies that an enum method can reference a class constant via `self::`.
#[test]
fn test_enum_method_uses_self_constant() {
    let out = compile_and_run(
        "<?php
        enum Scale {
            case One;
            const FACTOR = 5;
            public function compute(): int { return self::FACTOR * 3; }
        }
        echo Scale::One->compute();
        ",
    );
    assert_eq!(out, "15");
}

/// Compiles and runs the checked-in `examples/enum-methods/main.php` fixture, covering instance
/// methods (`match($this)`, `$this->value`, `self::CONST`), a static factory, and `implements`.
#[test]
fn test_example_enum_methods_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/enum-methods/main.php"));
    assert_eq!(out, "red/black\ndiamonds\nblack\n52\nclubs\n");
}

/// Verifies a pure (unit) enum case exposes the read-only `->name` property holding the
/// case identifier (issue #330). This is the minimal reproduction from the bug report.
#[test]
fn test_pure_enum_name_property() {
    let out = compile_and_run(
        "<?php
        enum E { case A; }
        echo E::A->name;
        ",
    );
    assert_eq!(out, "A");
}

/// Verifies a backed enum case exposes both `->name` (case identifier) and `->value`
/// (backing value), and that the two are distinct properties.
#[test]
fn test_backed_enum_name_and_value() {
    let out = compile_and_run(
        "<?php
        enum Code: int {
            case Ok = 1;
            case Err = 2;
        }
        echo Code::Err->name;
        echo PHP_EOL;
        echo Code::Err->value;
        ",
    );
    assert_eq!(out, "Err\n2");
}

/// Verifies a string-backed enum case `->name` returns the case identifier, not the
/// backing string value (`Status::Live->name` is `"Live"`, not `"live"`).
#[test]
fn test_string_backed_enum_name_distinct_from_value() {
    let out = compile_and_run(
        "<?php
        enum Status: string {
            case Draft = \"draft\";
            case Live = \"live\";
        }
        echo Status::Live->name;
        echo PHP_EOL;
        echo Status::Live->value;
        ",
    );
    assert_eq!(out, "Live\nlive");
}

/// Verifies `->name` reads correctly when the case singleton is aliased through a local
/// variable and when retrieved from the `cases()` array, exercising the string property
/// through assignment and array storage of the shared singleton.
#[test]
fn test_enum_name_through_variable_and_cases() {
    let out = compile_and_run(
        "<?php
        enum Suit { case Hearts; case Clubs; }
        $x = Suit::Clubs;
        echo $x->name;
        echo PHP_EOL;
        $cases = Suit::cases();
        echo $cases[0]->name;
        echo $cases[1]->name;
        ",
    );
    assert_eq!(out, "Clubs\nHeartsClubs");
}

/// Verifies `->name` works inside string interpolation alongside `->value`, matching PHP's
/// `"{$case->name}"` behavior.
#[test]
fn test_enum_name_in_interpolation() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 9; }
        $l = Level::High;
        echo \"name={$l->name} value={$l->value}\";
        ",
    );
    assert_eq!(out, "name=High value=9");
}
