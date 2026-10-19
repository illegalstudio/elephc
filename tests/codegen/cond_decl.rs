//! Purpose:
//! End-to-end coverage for conditional declarations: the
//! `if (! function_exists('f')) { function f() {} }` polyfill idiom (as in framework
//! `helpers.php` files), `use` imports inside `yield`, and the `UnitEnum`/`BackedEnum` interfaces.
//!
//! Called from:
//! - The native codegen suite (`tests/codegen/mod.rs`).
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.

use crate::support::*;

/// Verifies a guarded user function is declared, and that `function_exists` follows program order.
#[test]
fn test_function_exists_guard_declares_user_function() {
    let out = compile_and_run(r#"<?php
var_dump(function_exists('g_late'));
if (!function_exists('g_late')) { function g_late() { return 'late'; } }
var_dump(function_exists('g_late'));
echo g_late(), "\n";
function probe() { return function_exists('g_late') ? 'y' : 'n'; }
echo probe(), "\n";
"#);
    assert_eq!(out, "bool(false)\nbool(true)\nlate\ny\n");
}

/// Verifies an existing builtin or unconditional user function keeps the branch from declaring.
#[test]
fn test_function_exists_guard_skips_existing_functions() {
    let out = compile_and_run(r#"<?php
if (!function_exists('strlen')) { echo "no\n"; }
function plain() { return 'plain'; }
if (!function_exists('plain')) { function plain2() { return 'x'; } }
var_dump(function_exists('plain2'));
echo plain(), "\n";
if (function_exists('nowhere')) { echo "no\n"; } else { echo "absent\n"; }
"#);
    assert_eq!(out, "bool(false)\nplain\nabsent\n");
}

/// Verifies the leading backslash, case-insensitive names and an invalid `\\` needle.
#[test]
fn test_function_exists_guard_name_normalization() {
    let out = compile_and_run(r#"<?php
if (!function_exists('\Mixed_Case')) { function mixed_case() { return 'mc'; } }
echo MIXED_CASE(), "\n";
var_dump(function_exists('\\\\mixed_case'), function_exists('\MIXED_case'));
"#);
    assert_eq!(out, "mc\nbool(false)\nbool(true)\n");
}

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
        return match (true) {
            $value instanceof BackedEnum => $value->value,
            $value instanceof UnitEnum => $value->name,
            default => $value,
        };
    }
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "ab\nHOne\nbool(true)\n");
}

/// Verifies a guard whose declaration is not a clean top-level polyfill stays a clear error.
#[test]
fn test_function_exists_guard_inside_loop_is_not_hoisted() {
    let err = compile_cli_file_with_flags_expect_failure(
        "<?php\nforeach ([1] as $_) { if (!function_exists('lp')) { function lp() { return 1; } } }\necho lp();\n",
        &[],
    );
    assert!(err.contains("Undefined function: lp"), "{err}");
}

/// Verifies `use` imports apply inside `yield`, `yield $k =>`, `yield from`, arrow fns and closures
/// in generators, `clone`, `new $c(...)` arguments, `$o->{$k} = ...` and assignment expressions.
#[test]
fn test_use_imports_resolve_inside_yield_and_composite_expressions() {
    let out = compile_and_run(r#"<?php
// `use` imports were not applied inside yield (doctrine/inflector), clone, new $c(...),
// assignment expressions or $o->{$k} = ...
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
$o = new \stdClass; $k = 'p'; $o->{$k} = new P('dynprop'); echo $o->p->s, "\n";
if ($x = new P('assign')) { echo $x->s, "\n"; }
echo ($y = new Alias('assign-expr'))->s, "\n";
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
dynprop
assign
assign-expr
"#);
}

/// Verifies `UnitEnum`/`BackedEnum` as types: `instanceof`, parameter types (also nullable, in
/// unions and through a derived interface), `->name`/`->value` dispatched on the runtime enum,
/// and `cases()`/`from()`/`tryFrom()` called through the interface.
#[test]
fn test_unit_and_backed_enum_interfaces_as_types() {
    let out = compile_and_run(r#"<?php
// UnitEnum/BackedEnum as types: instanceof, type hints, ->name/->value and cases/from/tryFrom through the interface.
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
function kinds(UnitEnum $e): string { return count($e::cases()) . ($e instanceof BackedEnum ? $e::from($e->value)->name . ($e::tryFrom('zz') === null ? '-' : '+') : ''); }
echo kinds(S::A), ' ', kinds(U::C), "\n";
function raw(UnitEnum $e) { return $e->value; }
var_dump(raw(T::B));
"#);
    assert_eq!(out, r#"ABC a2
a2snull Q=q Cnone
bool(true)
bool(false)
bool(true)
bool(true)
1A- 1
int(2)
"#);
}

/// Verifies an include-discovered function keeps its runtime load-order answer next to a polyfill.
#[test]
fn test_function_exists_guard_leaves_include_variants_to_runtime() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() { include 'lib.php'; }
if (!function_exists('poly_cd')) { function poly_cd() { return 'poly'; } }
echo function_exists('late_cd') ? 'yes' : 'no', '|', poly_cd(), '|';
load_lib();
echo function_exists('late_cd') ? late_cd() : 'no';
"#,
            ),
            ("lib.php", "<?php function late_cd() { return 'late'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "no|poly|late");
}

/// Verifies top-level `eval()` keeps literal probes at runtime, where eval-declared names exist.
#[test]
fn test_function_exists_probe_after_top_level_eval_stays_runtime() {
    let out = compile_and_run(r#"<?php
echo function_exists('ev_cd') ? 'y' : 'n';
eval('function ev_cd() { return "e"; }');
echo function_exists('ev_cd') ? ev_cd() : 'n', "\n";
"#);
    assert_eq!(out, "ne\n");
}
