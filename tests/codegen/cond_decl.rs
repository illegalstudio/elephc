//! Purpose:
//! End-to-end coverage for the conditional-declaration follow-ups that are not optimizer
//! folds: Laravel's `helpers.php` polyfills across includes together with enum helpers, `use`
//! imports inside `yield` and composite expressions, and the `UnitEnum`/`BackedEnum` interfaces.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.
//! - The compile-time `function_exists()` polyfill folding cases live in
//!   `tests/codegen/optimizer/function_exists_guards.rs`.

use crate::support::*;

/// Verifies Laravel's helpers.php pattern across includes: first included declaration wins,

/// calls from earlier-declared functions resolve, and a namespaced guard name works.
#[test]
fn test_function_exists_guard_across_included_helpers() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require __DIR__ . '/support/helpers.php';
require __DIR__ . '/foundation/helpers.php';
require __DIR__ . '/support/functions.php';
enum Suit: string { case Hearts = 'H'; }
enum Pure { case One; }
function use_helpers() { return tapx('a') . only_b(); }
echo use_helpers(), "\n";
echo Illuminate\Support\enum_valuex(Suit::Hearts), Illuminate\Support\enum_valuex(Pure::One), "\n";
var_dump(function_exists('\illuminate\support\ENUM_VALUEX'));
"#,
            ),
            (
                "support/helpers.php",
                "<?php\nif (! function_exists('tapx')) {\n    /** doc */\n    function tapx($v) { return $v; }\n}\n",
            ),
            (
                "foundation/helpers.php",
                "<?php\nif (! function_exists('tapx')) { function tapx($v) { return 'B'; } }\nif (! function_exists('only_b')) { function only_b() { return 'b'; } }\n",
            ),
            (
                "support/functions.php",
                r#"<?php
namespace Illuminate\Support;
use BackedEnum;
use UnitEnum;
if (! function_exists('Illuminate\Support\enum_valuex')) {
    function enum_valuex($value) {
        if ($value instanceof BackedEnum) { return $value->value; }
        if ($value instanceof UnitEnum) { return $value->name; }
        return $value;
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ab\nHOne\nbool(true)\n");
}

/// Control (also passes on main): a guard whose declaration is not a clean top-level polyfill

/// Verifies `use` imports apply inside `yield`, `yield $k =>`, `yield from`, arrow fns and closures

/// in generators, `clone`, `new $c(...)` arguments, `$o->{$k} = ...` and assignment expressions.
#[test]
fn test_use_imports_resolve_inside_yield_and_composite_expressions() {
    let out = compile_and_run(r#"<?php
// `use` imports were not applied inside yield (doctrine/inflector), yield from, clone and
// new $c(...).
namespace A\R { class P { public function __construct(public $s) {} } }
namespace A\R\E {
use A\R\P;
use A\R\P as Alias;
class I {
    public static function g(): iterable { yield new P('x'); yield 'k' => new Alias('y'); }
    public static function h(): \Generator { yield from [new P('from')]; $gen = self::k(); yield from $gen; }
    public static function k(): \Generator { yield 'kk' => new Alias('k'); }
    public static function c(): \Generator { $f = fn ($v) => new P($v); yield $f('arrow'); $g = function () { yield new P('inner'); }; foreach ($g() as $p) { yield $p; } }
}
foreach (I::g() as $k => $p) echo $k, '=', $p->s, "\n";
foreach (I::h() as $p) echo $p->s, "\n";
foreach (I::c() as $p) echo $p->s, "\n";
echo (clone new P('clone'))->s, "\n";
$c = 'A\R\P'; echo (new $c(Alias::class))->s, "\n";
}
"#);
    assert_eq!(out, r#"0=x
k=y
from
k
arrow
inner
clone
A\R\P
"#);
}

/// Verifies `UnitEnum`/`BackedEnum` as types: `instanceof`, parameter types (also nullable, in

/// unions and through a derived interface), `->name`/`->value` dispatched on the runtime enum,

/// and an untyped read of `->value` through `UnitEnum`.
#[test]
fn test_unit_and_backed_enum_interfaces_as_types() {
    let out = compile_and_run(r#"<?php
// UnitEnum/BackedEnum as types: instanceof, type hints and ->name/->value through the interface.
namespace League\Uri;
use BackedEnum;
use UnitEnum;
enum S: string { case A = 'a'; }
enum T: int { case B = 2; }
enum U { case C; }
interface HasLabel extends BackedEnum {}
enum L: string implements HasLabel { case Q = 'q'; }
function n(UnitEnum $e): string { return $e->name; }
function v(BackedEnum $e): string { return (string) $e->value; }
function host(BackedEnum|string|null $h): string {
    return match (true) { null === $h => 'null', $h instanceof BackedEnum => (string) $h->value, default => (string) $h };
}
function lbl(HasLabel $l): string { return $l->name . '=' . $l->value; }
function maybe(?UnitEnum $e): string { return $e?->name ?? 'none'; }
echo n(S::A), n(T::B), n(U::C), ' ', v(S::A), v(T::B), "\n";
echo host(S::A), host(T::B), host('s'), host(null), ' ', lbl(L::Q), ' ', maybe(U::C), maybe(null), "\n";
var_dump(S::A instanceof BackedEnum, U::C instanceof BackedEnum, U::C instanceof UnitEnum, L::Q instanceof HasLabel);
function raw(UnitEnum $e) { return $e->value; }
var_dump(raw(T::B));
"#);
    assert_eq!(out, r#"ABC a2
a2snull Q=q Cnone
bool(true)
bool(false)
bool(true)
bool(true)
int(2)
"#);
}

/// Reading `->value` after `instanceof BackedEnum` on a `mixed` value, in a namespace that

/// imports the interface (issue #832), dispatches on the runtime enum.
#[test]
fn test_backed_enum_value_after_instanceof_narrowing() {
    let out = compile_and_run(r#"<?php
namespace Demo;

use BackedEnum;

enum Suit: string { case Hearts = 'H'; }
enum Size: int { case Large = 3; }

function extract(mixed $value): mixed
{
    if ($value instanceof BackedEnum) {
        return $value->value;
    }

    return null;
}
var_dump(extract(Suit::Hearts), extract(Size::Large), extract('x'));
"#);
    assert_eq!(out, "string(1) \"H\"\nint(3)\nNULL\n");
}
