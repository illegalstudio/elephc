//! Purpose:
//! End-to-end regressions for issue #1113: `is_subclass_of()` and `is_a()` take a class NAME as
//! readily as an object, and answering `false` for the name form is a silent wrong answer.
//!
//! Called from:
//! - `cargo test --test codegen_tests class_relation_names` through Rust's test harness.
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
//! - Still `false`, and deliberately: a NON-LITERAL name. That needs a name-keyed table the
//!   emitted program can consult at runtime, which is the second half of #1113 and what
//!   `ReflectionAttribute::IS_INSTANCEOF` actually needs.

use crate::support::compile_and_run;

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
