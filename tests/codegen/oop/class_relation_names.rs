//! Purpose:
//! End-to-end regressions for issue #1113: `is_subclass_of()` and `is_a()` take a class NAME as
//! readily as an object, and answering `false` for the name form is a silent wrong answer.
//!
//! Called from:
//! - `cargo test --test codegen_tests_oop class_relation_names` through Rust's test harness.
//!
//! Key details:
//! - `static_relation_holds` opened by requiring `PhpType::Object`, so a string first operand
//!   returned `false` before the parent and interface walks below it were ever reached. Those
//!   walks were already correct — they just need a class name, and do not care which operand
//!   shape it arrived in.
//! - PHP's `$allow_string` defaults differ between the two builtins: `is_subclass_of` takes
//!   names unless told otherwise, `is_a` only when told to. Measured, `is_a("D", "B")` is false
//!   and `is_a("D", "B", true)` is true.
//! - An INTERFACE name is reachable only through the string form — there is no instance of an
//!   interface to pass — and its parents live in `interface_infos`, not `class_infos`, so it
//!   needs its own walk.
//! - A RUNTIME `$allow_string` is not guessed at. For a literal name the answer is exactly
//!   `$allow_string && relation`, so when the relation holds the flag IS the answer. Substituting
//!   the builtin's default instead made `is_subclass_of("Derived", "Base", $false)` answer `y`.
//! - A runtime flag need not be a `bool`. The declared signature is `bool $allow_string`, but
//!   coercive mode admits any scalar and the argument lowering does not cast it, so an `int`,
//!   `string` or `float` reaches the lowering. Answering `false` for those made `$flag = 1;
//!   is_subclass_of("Derived", "Base", $flag)` report `n` where PHP reports `y`. The relation
//!   already holds at that point, so the answer is exactly the flag's PHP truthiness — which is
//!   why the shared `emit_value_truthiness` is reused rather than a nonzero test: `"0"` is false
//!   but `"0.0"` and `" "` are true, and a hand-rolled test gets that wrong.
//! - A name that resolves to nothing answers `false`, INCLUDING against itself: PHP looks the
//!   subject up before comparing, so `is_a("Ghost", "Ghost", true)` is false.
//! - A NON-LITERAL target (`is_a($name, $name, true)`) used to answer `false` for every
//!   subject: the static walk needs a literal target. The subject now selects the ancestor list
//!   of one compiled class or interface (by name, or by an object's runtime class id), and the
//!   target is compared with it at runtime.
//! - A class only an autoloader could load, and that the program never names, is not compiled,
//!   so a runtime name cannot reach it: the same AOT limit as `class_exists($name)`.

use crate::support::{compile_and_run, compile_and_run_files_per_argv, compile_and_run_per_argv};

/// Boxed subjects use runtime class and interface metadata without requiring an eval context.
#[test]
fn test_is_a_mixed_subject_literal_target_without_eval() {
    let out = compile_and_run(r#"<?php
interface MixedRelationMarker {}
class MixedRelationBase implements MixedRelationMarker {}
class MixedRelationChild extends MixedRelationBase {}
function mixedRelationProbe(mixed $value): void {
    echo (int)is_a($value, 'MixedRelationBase'), (int)is_a($value, 'MixedRelationMarker');
    echo (int)is_a($value, 'MixedRelationChild'), (int)is_a($value, 'MissingMixedRelation');
    echo (int)is_a($value, '\\mIxEdReLaTiOnBaSe'), '|';
}
mixedRelationProbe(new MixedRelationChild()); mixedRelationProbe(new MixedRelationBase());
mixedRelationProbe(new stdClass()); mixedRelationProbe('MixedRelationChild');
mixedRelationProbe(null); mixedRelationProbe(42);
"#);
    assert_eq!(out, "11101|11001|00000|00000|00000|00000|");
}

/// Verifies the six rows the issue was filed with, plus the two the object form already got
/// right, so a regression on either side is visible.
#[test]
fn test_class_name_subject_answers_like_php() {
    let out = compile_and_run(
        r#"<?php
class Base {}
class Derived extends Base {}
class Deeper extends Derived {}
interface I {}
class Impl implements I {}

$o = new Derived();
echo is_subclass_of($o, "Base") ? "y" : "n";
echo is_subclass_of("Derived", "Base") ? "y" : "n";
echo is_subclass_of("Deeper", "Base") ? "y" : "n";
echo is_subclass_of("Impl", "I") ? "y" : "n";
echo is_subclass_of("Base", "Base") ? "y" : "n";
echo is_subclass_of("Impl", "Base") ? "y" : "n";
echo is_a($o, "Base") ? "y" : "n";
echo is_a("Derived", "Base", true) ? "y" : "n";
"#,
    );

    assert_eq!(out, "yyyynnyy");
}

/// Verifies PHP's two different `$allow_string` defaults, and that an explicit flag overrides
/// each of them.
#[test]
fn test_allow_string_defaults_differ_between_the_two_builtins() {
    let out = compile_and_run(
        r#"<?php
class Base {}
class Derived extends Base {}

echo is_subclass_of("Derived", "Base") ? "y" : "n";
echo is_subclass_of("Derived", "Base", true) ? "y" : "n";
echo is_subclass_of("Derived", "Base", false) ? "y" : "n";
echo is_a("Derived", "Base") ? "y" : "n";
echo is_a("Derived", "Base", true) ? "y" : "n";
echo is_a("Derived", "Base", false) ? "y" : "n";
echo is_a("Base", "Base", true) ? "y" : "n";
"#,
    );

    assert_eq!(out, "yynnyny");
}

/// Verifies an interface name as the subject, which the object form can never ask about, and
/// which walks a different table.
#[test]
fn test_an_interface_name_is_a_valid_subject() {
    let out = compile_and_run(
        r#"<?php
interface I {}
interface J extends I {}
interface K extends J {}
interface Unrelated {}
class ImplJ implements J {}

echo is_subclass_of("J", "I") ? "y" : "n";
echo is_subclass_of("K", "I") ? "y" : "n";
echo is_subclass_of("I", "J") ? "y" : "n";
echo is_subclass_of("J", "Unrelated") ? "y" : "n";
echo is_subclass_of("ImplJ", "I") ? "y" : "n";
echo is_a("J", "I", true) ? "y" : "n";
echo is_a("I", "I", true) ? "y" : "n";
"#,
    );

    assert_eq!(out, "yynnyyy");
}

/// Verifies names are matched the way PHP matches them — case-insensitively, with a leading
/// separator ignored — and that an unknown name answers `false` rather than claiming a relation.
#[test]
fn test_names_fold_like_php_and_unknown_names_answer_false() {
    let out = compile_and_run(
        r#"<?php
namespace App;

class Base {}
class Derived extends Base {}

echo \is_subclass_of("app\\dErIvEd", "App\\Base") ? "y" : "n";
echo \is_subclass_of("App\\Derived", "app\\bAsE") ? "y" : "n";
echo \is_subclass_of("\\App\\Derived", "\\App\\Base") ? "y" : "n";
echo \is_subclass_of("App\\NoSuchClass", "App\\Base") ? "y" : "n";
echo \is_subclass_of("App\\Derived", "App\\NoSuchTarget") ? "y" : "n";
echo \is_subclass_of("", "App\\Base") ? "y" : "n";
"#,
    );

    assert_eq!(out, "yyynnn");
}

/// Verifies a RUNTIME `$allow_string` decides the answer rather than being replaced by the
/// builtin's compile-time default.
///
/// The flag governs only a string subject, so for a literal name the result is exactly
/// `$allow_string && relation`. Guessing the default instead answered `y` for a false flag —
/// a wrong answer this lowering did not produce before it learned to read names at all — and `n`
/// for a true one on `is_a`, which was wrong before and after.
#[test]
fn test_a_runtime_allow_string_flag_decides_the_answer() {
    let out = compile_and_run(
        r#"<?php
class Base {}
class Derived extends Base {}

function pick(int $i): bool { return $i > 100; }

$off = pick(0);
$on  = pick(200);

echo is_subclass_of("Derived", "Base", $off) ? "y" : "n";
echo is_subclass_of("Derived", "Base", $on) ? "y" : "n";
echo is_a("Derived", "Base", $off) ? "y" : "n";
echo is_a("Derived", "Base", $on) ? "y" : "n";
echo is_subclass_of(new Derived(), "Base", $off) ? "y" : "n";
echo is_subclass_of("Derived", "Missing", $on) ? "y" : "n";
"#,
    );

    assert_eq!(out, "nynyyn");
}

/// Verifies a name that resolves to nothing answers `false` even against itself.
///
/// PHP looks the subject up before comparing, so an undeclared name is not "equal to itself" for
/// `is_a`. The self-check has to run AFTER the name is known to exist, which is why the interface
/// branch gates on the interface table first.
#[test]
fn test_an_undeclared_name_answers_false_even_against_itself() {
    let out = compile_and_run(
        r#"<?php
interface I {}

echo is_a("Ghost", "Ghost", true) ? "y" : "n";
echo is_a("Ghost", "ghost", true) ? "y" : "n";
echo is_subclass_of("Ghost", "Ghost") ? "y" : "n";
echo is_a("Ghost", "I", true) ? "y" : "n";
echo is_a("I", "I", true) ? "y" : "n";
"#,
    );

    assert_eq!(out, "nnnny");
}

/// Verifies a runtime `$allow_string` that is not a `bool` is converted with PHP's truthiness
/// rather than answered `false`.
///
/// The declared parameter is `bool $allow_string`, but coercive mode admits any scalar and the
/// argument lowering does not cast it, so `int`, `string` and `float` flags all reach the
/// lowering. Each one answered `n` where PHP answers `y`.
#[test]
fn test_a_non_bool_runtime_allow_string_flag_uses_php_truthiness() {
    let out = compile_and_run(
        r#"<?php
class Base {}
class Derived extends Base {}
interface I {}
class Impl implements I {}

function pickInt(int $i): int { return $i > 100 ? 1 : 0; }
function pickStr(int $i, string $a): string { return $i > 100 ? $a : "x"; }
function pickFloat(int $i, float $a): float { return $i > 100 ? $a : 0.0; }

echo is_subclass_of("Derived", "Base", pickInt(200)) ? "y" : "n";
echo is_subclass_of("Derived", "Base", pickInt(0)) ? "y" : "n";
echo is_a("Derived", "Base", pickInt(200)) ? "y" : "n";
echo is_subclass_of("Impl", "I", pickInt(200)) ? "y" : "n";
echo is_subclass_of("Derived", "Base", pickFloat(200, 2.5)) ? "y" : "n";
echo is_subclass_of("Derived", "Base", pickFloat(200, 0.0)) ? "y" : "n";
"#,
    );

    assert_eq!(out, "ynyyyn");
}

/// Verifies the string truthiness corners PHP treats specially reach the right answer.
///
/// `"0"` is the trap: it is a non-empty string and still false, while `"0.0"` and `" "` are true.
/// Reusing the shared truthiness lowering is what gets this right; a nonzero test would report
/// `"0"` as true and `"0.0"` no differently.
#[test]
fn test_string_allow_string_flags_follow_php_string_truthiness() {
    let out = compile_and_run(
        r#"<?php
class Base {}
class Derived extends Base {}

function pick(int $i, string $a): string { return $i > 100 ? $a : "x"; }

echo is_subclass_of("Derived", "Base", pick(200, "0")) ? "y" : "n";
echo is_subclass_of("Derived", "Base", pick(200, "0.0")) ? "y" : "n";
echo is_subclass_of("Derived", "Base", pick(200, " ")) ? "y" : "n";
echo is_subclass_of("Derived", "Base", pick(200, "")) ? "y" : "n";
echo is_subclass_of("Derived", "Base", pick(200, "1")) ? "y" : "n";
"#,
    );

    assert_eq!(out, "nyyny");
}

/// A target class name known only at runtime: `is_a($name, $name, true)` was `false` for every
/// subject. Covers a name against itself, parents, interfaces through an interface parent,
/// builtin exceptions, case-insensitive and backslash-rooted names, unknown names, and the
/// `$allow_string` flag, in a namespace. Expected output measured with php 8.5.12.
#[test]
fn test_is_a_runtime_target_name() {
    let out = compile_and_run_per_argv(
        r#"<?php
namespace App;
interface Root {}
interface Leaf extends Root {}
class Base implements Leaf {}
class Child extends Base {}
class Failure extends \RuntimeException {}
function rel(string $sub, string $target, bool $flag): string {
    return (is_a($sub, $target, true) ? "y" : "n") . (is_subclass_of($sub, $target) ? "y" : "n")
        . (is_a($sub, $target, $flag) ? "y" : "n") . (is_subclass_of($sub, $target, $flag) ? "y" : "n");
}
$sub = $argv[1] ?? "App\\Child";
$target = $argv[2] ?? "App\\Base";
echo rel($sub, $sub, true), " ", rel($sub, $target, true), " ", rel($sub, $target, false), " ", rel($target, $sub, true), "\n";
foreach ([["app\\child", "\\APP\\ROOT"], ["\\App\\Leaf", "App\\Root"], ["App\\Root", "App\\Root"], ["Ghost", "Ghost"],
          ["App\\Failure", "Exception"], ["App\\Failure", "Throwable"], ["RuntimeException", "App\\Failure"]] as [$s, $t]) {
    echo rel($s, $t, true), " ";
}
echo "\n";
"#,
        &[&[], &["App\\Base", "App\\Child"], &["App\\Root", "App\\Leaf"], &["Nope", "App\\Base"]],
    );
    let catalog = "yyyy yyyy ynyn nnnn yyyy yyyy nnnn \n";
    assert_eq!(out, [
        format!("ynyn yyyy yynn nnnn\n{catalog}"),
        format!("ynyn nnnn nnnn yyyy\n{catalog}"),
        format!("ynyn nnnn nnnn yyyy\n{catalog}"),
        format!("nnnn nnnn nnnn nnnn\n{catalog}"),
    ]);
}

/// An object subject with a runtime target answers from the object's runtime class, not its
/// static type (`Square $o` holding a `Cube`), and ignores `$allow_string`. A Mixed subject is
/// matched as an object or as a class name by its runtime tag, and any other value is `false`.
/// Expected output measured with php 8.5.12.
#[test]
fn test_is_a_object_and_mixed_subject_runtime_target() {
    let out = compile_and_run_per_argv(
        r#"<?php
interface Shape {}
class Square implements Shape {}
class Cube extends Square {}
function probe(Square $o, string $target): string {
    return (is_a($o, $target) ? "y" : "n") . (is_subclass_of($o, $target) ? "y" : "n") . (is_a($o, $target, false) ? "y" : "n");
}
$target = $argv[1] ?? "Square";
echo probe(new Cube(), $target), " ", probe(new Square(), $target), " ", probe(new Cube(), "shape"), " ", probe(new Square(), "Nope"), "\n";
$values = [$argc > 9 ? 1 : new Cube(), "Cube", 7, null];
foreach ($values as $value) {
    echo is_a($value, $target, true) ? "y" : "n", is_subclass_of($value, $target) ? "y" : "n", is_a($value, $target) ? "y" : "n";
}
echo "\n";
"#,
        &[&[], &["Cube"], &["Shape"], &["stdClass"]],
    );
    assert_eq!(out, [
        "yyy yny yyy nnn\nyyyyynnnnnnn\n",
        "yny nnn yyy nnn\nynyynnnnnnnn\n",
        "yyy yyy yyy nnn\nyyyyynnnnnnn\n",
        "nnn nnn yyy nnn\nnnnnnnnnnnnn\n",
    ]);
}

/// Classes loaded through a PSR-4 autoloader relate to a runtime name once the program names
/// them. Expected output measured with php 8.5.12 and an equivalent `spl_autoload_register`.
#[test]
fn test_is_a_runtime_target_name_autoloaded_classes() {
    let out = compile_and_run_files_per_argv(
        &[
            ("composer.json", r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#),
            ("src/Shapes/Shape.php", "<?php\nnamespace App\\Shapes;\ninterface Shape {}\n"),
            ("src/Shapes/Square.php", "<?php\nnamespace App\\Shapes;\nclass Square implements Shape {}\n"),
            ("src/Shapes/Cube.php", "<?php\nnamespace App\\Shapes;\nclass Cube extends Square {}\n"),
            ("main.php", r#"<?php
$known = [App\Shapes\Cube::class];
$n = $argv[1] ?? "App\\Shapes\\Cube";
$t = $argv[2] ?? "App\\Shapes\\Shape";
var_dump(is_a($n, $n, true), is_a($n, $t, true), is_subclass_of($n, $t), is_subclass_of($t, $n));
"#),
        ],
        "main.php",
        &[&[], &["App\\Shapes\\Square", "App\\Shapes\\Cube"]],
    );
    assert_eq!(out, [
        "bool(true)\nbool(true)\nbool(true)\nbool(false)\n",
        "bool(true)\nbool(false)\nbool(false)\nbool(true)\n",
    ]);
}

/// With eval in the program, a compiled subject still answers from compiled metadata, and an
/// eval-declared class (by name, as an object, or boxed in a Mixed) asks the eval bridge, which
/// knows the eval class itself. Expected output measured with php 8.5.12.
#[test]
fn test_is_a_runtime_target_name_with_eval_classes() {
    let out = compile_and_run_per_argv(
        r#"<?php
class Foo {}
class Bar extends Foo {}
eval('class Dyn extends Bar {}');
$n = $argv[1] ?? 'Foo';
$o = $argc > 9 ? 1 : new Dyn();
echo is_a('Bar', $n, true) ? "y" : "n", is_a($argv[2] ?? 'Bar', $n, true) ? "y" : "n", is_subclass_of(new Bar(), $n) ? "y" : "n";
echo is_a('Dyn', $n, true) ? "y" : "n", is_subclass_of('Dyn', $n) ? "y" : "n", is_a(new Dyn(), $n) ? "y" : "n", is_a($o, $n) ? "y" : "n", is_subclass_of($o, $n) ? "y" : "n", "\n";
"#,
        &[&[], &["Dyn"], &["Bar"]],
    );
    assert_eq!(out, ["yyyyyyyy\n", "nnnynyyn\n", "yynyyyyy\n"]);
}

/// A closure is an instance of `Closure`, whether held as a callable or boxed in a Mixed, so a
/// runtime `Closure` target (any case, backslash-rooted or not) matches it. `Closure` is final
/// and implements nothing, so `is_subclass_of()` and every other target answer `false`.
/// Expected output measured with php 8.5.12.
#[test]
fn test_is_a_closure_subject_runtime_target() {
    let out = compile_and_run_per_argv(
        r#"<?php
function probe(mixed $v, string $t): string {
    return (is_a($v, $t) ? "y" : "n") . (is_subclass_of($v, $t) ? "y" : "n") . (is_a($v, $t, true) ? "y" : "n");
}
function typed(callable $f, string $t): string { return (is_a($f, $t) ? "y" : "n") . (is_subclass_of($f, $t) ? "y" : "n"); }
$t = $argv[1] ?? "Closure";
$c = function () { return 1; };
$a = fn() => 2;
echo probe($c, $t), " ", probe($a, $t), " ", probe(strlen(...), $t), " ", typed($c, $t), " ", is_a($c, $t) ? "y" : "n", "\n";
"#,
        &[&[], &["closure"], &["\\Closure"], &["Other"], &["stdClass"]],
    );
    let closure = "yny yny yny yn y\n";
    let other = "nnn nnn nnn nn n\n";
    assert_eq!(out, [closure, closure, closure, other, other]);
}

/// A runtime target holding a NUL byte names no class, even with eval in the program, whose
/// bridge reads its candidate list as NUL-separated names. A closure subject still matches
/// `Closure` there. Expected output measured with php 8.5.12.
#[test]
fn test_is_a_runtime_target_with_nul_byte_and_eval_classes() {
    let out = compile_and_run_per_argv(
        r#"<?php
class Foo {}
eval('class Dyn extends Foo {}');
$nul = chr($argc > 9 ? 1 : 0);
$t = $argv[1] ?? "Dyn";
$m = $argc > 9 ? 1 : fn() => 0;
echo is_a('Dyn', $t, true) ? "y" : "n", is_a('Dyn', "Ghost" . $nul . $t, true) ? "y" : "n", is_a('Dyn', $t . $nul, true) ? "y" : "n";
echo is_subclass_of('Dyn', $t . $nul . "Ghost") ? "y" : "n", is_a('Dyn', $nul . $t, true) ? "y" : "n";
echo " ", is_a($m, $argv[2] ?? "Closure") ? "y" : "n", "\n";
"#,
        &[&[], &["Foo", "closure"], &["Foo", "Foo"]],
    );
    assert_eq!(out, ["ynnnn y\n", "ynnnn y\n", "ynnnn n\n"]);
}

/// Only a Closure is an instance of `Closure`: a function name or a method array passed as a
/// `callable` is an implicit callable without object identity, so a runtime `Closure` target
/// does not match it, held as a callable or forwarded into a Mixed. A first-class callable
/// (`strlen(...)`, `$o->m(...)`) is a Closure and matches. Expected output measured with
/// php 8.5.12.
#[test]
fn test_is_a_implicit_callable_is_not_a_closure() {
    let out = compile_and_run_per_argv(
        r#"<?php
class Cls { public function m() {} public static function sm() {} }
function m(mixed $f, string $t): string { return (is_a($f, $t) ? "y" : "n") . (is_a($f, $t, true) ? "y" : "n"); }
function c(callable $f, string $t): string { return (is_a($f, $t) ? "y" : "n") . (is_a($f, $t, true) ? "y" : "n") . m($f, $t); }
$t = $argv[1] ?? "Closure";
$o = new Cls();
echo c('strlen', $t), " ", c([$o, 'm'], $t), " ", c('Cls::sm', $t), " ", c(['Cls', 'sm'], $t), " ";
echo c(strlen(...), $t), " ", c($o->m(...), $t), " ", c(fn() => 1, $t), "\n";
"#,
        &[&[], &["Cls"]],
    );
    assert_eq!(out, ["nnnn nnnn nnnn nnnn yyyy yyyy yyyy\n", "nnnn nnnn nnnn nnnn nnnn nnnn nnnn\n"]);
}
