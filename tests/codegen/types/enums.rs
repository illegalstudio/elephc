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

/// Regression: an enum used as a class property / promoted-constructor-param TYPE
/// must resolve. Previously failed with "Unknown type: Tag" because enum names were
/// not pre-declared before the class schema pass resolved member type annotations.
#[test]
fn test_enum_as_promoted_constructor_param_type() {
    let out = compile_and_run(
        "<?php
        enum Tag: string {
            case Div = 'div';
            case Span = 'span';
        }
        final class Element {
            public function __construct(private Tag $tag, private string $text) {}
            public function render(): string {
                return '<' . $this->tag->value . '>' . $this->text . '</' . $this->tag->value . '>';
            }
        }
        echo (new Element(Tag::Div, 'hi'))->render();
        ",
    );
    assert_eq!(out, "<div>hi</div>");
}

/// Verifies an unused function accepts an enum case as the default for an enum-typed parameter.
#[test]
fn test_unused_function_accepts_enum_case_parameter_default() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";
}
function unused_level(Level $level = Level::Low): string {
    return $level->value;
}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies method and promoted-constructor parameters materialize enum case defaults.
#[test]
fn test_method_and_promoted_constructor_accept_enum_case_defaults() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";
}
final class Config {
    public function __construct(public Level $level = Level::Low) {}
    public function value(Level $level = Level::High): string {
        return $level->value;
    }
}
$config = new Config();
echo $config->level->value;
echo ":";
echo $config->value();
"#,
    );
    assert_eq!(out, "low:high");
}

/// Verifies `self::` and `parent::` defaults resolve in their declaring method contexts.
#[test]
fn test_method_enum_case_defaults_resolve_relative_receivers() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";

    public function value(self $level = self::Low): string {
        return $level->value;
    }
}
class LevelDefaults {
    public const DEFAULT_LEVEL = Level::High;
}
class ChildDefaults extends LevelDefaults {
    public function value(Level $level = parent::DEFAULT_LEVEL): string {
        return $level->value;
    }
}
echo Level::High->value();
echo ":";
echo (new ChildDefaults())->value();
"#,
    );
    assert_eq!(out, "low:high");
}

/// Verifies closure signatures accept and materialize enum case parameter defaults.
#[test]
fn test_closure_accepts_enum_case_parameter_default() {
    let out = compile_and_run(
        r#"<?php
enum Level: string {
    case Low = "low";
    case High = "high";
}
$value = function (Level $level = Level::Low): string {
    return $level->value;
};
echo $value();
"#,
    );
    assert_eq!(out, "low");
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

/// Verifies enum case objects expose PHP's readonly `name` property directly and inside methods.
#[test]
fn test_enum_case_name_property_and_method() {
    let out = compile_and_run(
        "<?php
        enum Suit {
            case Hearts;
            case Clubs;
            public function label(): string {
                return $this->name;
            }
        }
        echo Suit::Hearts->name;
        echo '|';
        echo Suit::Clubs->label();
        ",
    );
    assert_eq!(out, "Hearts|Clubs");
}

/// Verifies enum methods can be imported from traits and run with `$this` bound to the case.
#[test]
fn test_enum_uses_trait_method() {
    let out = compile_and_run(
        "<?php
        trait HasEnumLabel {
            public function label(): string {
                return $this->name;
            }
        }
        enum Suit {
            use HasEnumLabel;
            case Hearts;
            case Clubs;
        }
        echo Suit::Hearts->label();
        echo '|';
        echo Suit::Clubs->label();
        ",
    );
    assert_eq!(out, "Hearts|Clubs");
}

/// Verifies enum trait adaptations support `insteadof` conflict resolution and aliases.
#[test]
fn test_enum_trait_insteadof_and_alias() {
    let out = compile_and_run(
        "<?php
        trait PrimaryEnumLabel {
            public function label(): string {
                return 'P:' . $this->name;
            }
        }
        trait SecondaryEnumLabel {
            public function label(): string {
                return 'S:' . $this->name;
            }
        }
        enum Mode {
            use PrimaryEnumLabel, SecondaryEnumLabel {
                PrimaryEnumLabel::label insteadof SecondaryEnumLabel;
                SecondaryEnumLabel::label as secondaryLabel;
            }
            case Active;
        }
        echo Mode::Active->label();
        echo '|';
        echo Mode::Active->secondaryLabel();
        ",
    );
    assert_eq!(out, "P:Active|S:Active");
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
    assert_eq!(
        out,
        "1\n2\n3\nRed=1 Green=2 Blue=3 \nDefault=default Match=match MATCH=upper-match \nDESC"
    );
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
            public function label(): string { return $this->name . ':' . ($this->value * 2); }
        }
        echo Power::High->label();
        ",
    );
    assert_eq!(out, "High:20");
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

/// Regression for #349: an int-backed enum's `from()` accepts a PHP numeric string
/// (e.g. `"1"`), coercing it to the integer backing value and returning the matching
/// case — instead of rejecting the string argument at compile time.
#[test]
fn test_backed_int_enum_from_numeric_string() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        echo Level::from(\"1\")->name;
        echo Level::from(\"2\")->name;
        ",
    );
    assert_eq!(out, "LowHigh");
}

/// Regression for #349: PHP's coercive int parameter rules accept signed,
/// whitespace-padded, decimal-float, and exponent-form numeric strings for
/// int-backed enum `from()`, with float strings truncated toward zero.
#[test]
fn test_backed_int_enum_from_numeric_string_coercion_forms() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Zero = 0; case One = 1; case Thousand = 1000; }
        echo Level::from(\" 1 \")->name, \"|\";
        echo Level::from(\"+1\")->name, \"|\";
        echo Level::from(\"1.0\")->name, \"|\";
        echo Level::from(\"1.5\")->name, \"|\";
        echo Level::from(\"1e3\")->name, \"|\";
        echo Level::from(\".5\")->name;
        ",
    );
    assert_eq!(out, "One|One|One|One|Thousand|Zero");
}

/// Regression for #349: `tryFrom()` on an int-backed enum coerces a numeric string and
/// returns the matching case, or `null` when the coerced value matches no case.
#[test]
fn test_backed_int_enum_tryfrom_numeric_string() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        echo Level::tryFrom(\"1\")->name;
        $miss = Level::tryFrom(\"3\");
        echo $miss === null ? \"null\" : $miss->name;
        ",
    );
    assert_eq!(out, "Lownull");
}

/// Regression for #349: a numeric string that coerces to an int with no matching case
/// throws a `ValueError` with PHP's backing-value message (like an integer argument would).
#[test]
fn test_backed_int_enum_from_unmatched_numeric_string_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::from(\"3\");
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(out, "ValueError:3 is not a valid backing value for enum Level");
}

/// Regression for #349: negative numeric strings still coerce successfully before
/// enum lookup, so a missing negative backing value raises `ValueError`, not `TypeError`.
#[test]
fn test_backed_int_enum_from_negative_numeric_string_throws_value_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::from(\"-1\");
        } catch (ValueError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "ValueError:-1 is not a valid backing value for enum Level"
    );
}

/// Regression for #349: a non-numeric string passed to an int-backed enum's `from()`
/// throws a `TypeError` at runtime with PHP's exact argument-type message, matching
/// PHP's coercive-typing behavior instead of being accepted or rejected at compile time.
#[test]
fn test_backed_int_enum_from_non_numeric_string_throws_type_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::from(\"x\");
        } catch (TypeError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "TypeError:Level::from(): Argument #1 ($value) must be of type int, string given"
    );
}

/// Regression for #349: enum int-parameter coercion must reject libc `strtod`
/// extensions (`0x` hex floats, `INF`/`INFINITY`, and `NAN`) as `TypeError`, even
/// though the general numeric-string probe used by loose comparison accepts them.
#[test]
fn test_backed_int_enum_from_rejects_strtod_only_numeric_forms() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function mark(string $value) {
            try {
                Level::from($value);
                echo \"ok\";
            } catch (TypeError $e) {
                echo \"T\";
            } catch (ValueError $e) {
                echo \"V\";
            }
            echo \"|\";
        }
        mark(\"0x1\");
        mark(\"0X1\");
        mark(\"INF\");
        mark(\"inf\");
        mark(\"+inf\");
        mark(\"Infinity\");
        mark(\"NAN\");
        mark(\"nan\");
        mark(\"1abc\");
        mark(\"\");
        ",
    );
    assert_eq!(out, "T|T|T|T|T|T|T|T|T|T|");
}

/// Regression for #349: repeatedly coercing a heap-owned numeric string through an
/// int-backed enum `from()` in a loop and storing the reassigned result must keep the
/// case singleton's refcount balanced (it is a persistent, program-lifetime object).
/// Before the fix the returned singleton was under-retained and freed after a few
/// iterations, corrupting the heap free-list and crashing. Exercises the ownership path,
/// not just single-shot coercion.
#[test]
fn test_backed_int_enum_from_numeric_string_in_loop_keeps_singleton_alive() {
    let out = compile_and_run(
        "<?php
        enum L: int { case A = 1; case B = 2; }
        $acc = 0;
        for ($n = 0; $n < 50; $n++) {
            $t = str_repeat(\"2\", 1);
            $c = L::from($t);
            $acc += $c->value;
        }
        echo $acc;
        ",
    );
    assert_eq!(out, "100");
}

/// Regression for #349: `tryFrom()` also throws a `TypeError` (not `null`) for a
/// non-numeric string argument, matching PHP.
#[test]
fn test_backed_int_enum_tryfrom_non_numeric_string_throws_type_error() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::tryFrom(\"x\");
        } catch (TypeError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "TypeError:Level::tryFrom(): Argument #1 ($value) must be of type int, string given"
    );
}

/// Regression for #349: `tryFrom()` shares the same int-parameter coercion probe as
/// `from()`, so strtod-only forms such as `NAN` throw `TypeError` instead of returning
/// `null` or entering enum backing-value lookup.
#[test]
fn test_backed_int_enum_tryfrom_rejects_strtod_only_numeric_forms() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        try {
            Level::tryFrom(\"NAN\");
        } catch (TypeError $e) {
            echo get_class($e), \":\", $e->getMessage();
        }
        ",
    );
    assert_eq!(
        out,
        "TypeError:Level::tryFrom(): Argument #1 ($value) must be of type int, string given"
    );
}

/// Regression for #449: an int-backed enum's `from()` accepts a `Mixed` argument — here the
/// value of a `foreach` over a string array — coercing each element to the backing value.
/// Before the fix this failed to compile with "backing input PHP type Mixed".
#[test]
fn test_backed_int_enum_from_mixed_foreach_value() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        foreach ([\"1\", \"2\", \"1\"] as $v) {
            echo Level::from($v)->name;
        }
        ",
    );
    assert_eq!(out, "LowHighLow");
}

/// Regression for #449: an untyped (`Mixed`) parameter passed to `from()` coerces on its
/// runtime tag — a numeric string and an integer both resolve to the matching case.
#[test]
fn test_backed_int_enum_from_mixed_untyped_param() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function pick($x) { return Level::from($x)->name; }
        echo pick(\"1\"), pick(2), pick(\"2\");
        ",
    );
    assert_eq!(out, "LowHighHigh");
}

/// Verifies keyword-named enum cases remain distinct by exact case, resolve only through their
/// declared spelling, and expose that spelling through the PHP `->name` property.
#[test]
fn test_keyword_named_enum_cases_preserve_case_and_name() {
    let out = compile_and_run(
        "<?php
        enum KeywordCase: string {
            case Default = 'default';
            case DEFAULT = 'upper-default';
            case Match = 'match';
            case MATCH = 'upper-match';
            case Print = 'print';
        }
        foreach (KeywordCase::cases() as $case) {
            echo $case->name, '=', $case->value, ';';
        }
        ",
    );
    assert_eq!(
        out,
        "Default=default;DEFAULT=upper-default;Match=match;MATCH=upper-match;Print=print;"
    );
}

/// Regression for #449: a heterogeneous (`Mixed`) array mixing coercible and non-coercible
/// values dispatches per element — integers and numeric strings resolve, a non-numeric
/// string and an array each throw `TypeError` with PHP's runtime-type message.
#[test]
fn test_backed_int_enum_from_mixed_per_element_dispatch() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function pick($x) {
            try {
                return Level::from($x)->name;
            } catch (TypeError $e) {
                return $e->getMessage();
            }
        }
        foreach ([1, \"2\", \"x\", [9]] as $v) {
            echo pick($v), \"|\";
        }
        ",
    );
    assert_eq!(
        out,
        "Low|High|Level::from(): Argument #1 ($value) must be of type int, string given|Level::from(): Argument #1 ($value) must be of type int, array given|"
    );
}

/// Regression for #449: `tryFrom()` on a `Mixed` argument coerces and returns the matching
/// case, or `null` when the coerced value matches no case.
#[test]
fn test_backed_int_enum_tryfrom_mixed() {
    let out = compile_and_run(
        "<?php
        enum Level: int { case Low = 1; case High = 2; }
        function pick($x) { $r = Level::tryFrom($x); return $r === null ? \"null\" : $r->name; }
        echo pick(\"2\"), pick(\"9\"), pick(1);
        ",
    );
    assert_eq!(out, "HighnullLow");
}
