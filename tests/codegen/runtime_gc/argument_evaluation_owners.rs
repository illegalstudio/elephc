//! Purpose:
//! Exercises call argument evaluation owners through native unwind and success paths.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Direct, static, instance, named and spread calls protect earlier owned values.
//! - Reference places and aliasing return values keep their ordinary ownership contracts.

use crate::support::*;

/// Earlier by-value arguments retire cleanly when a later source argument throws.
#[test]
fn test_core_call_argument_evaluation_owners_balance_throw_and_alias_paths() {
    let source = r#"<?php
class ArgumentOwnerSource {
    public mixed $text = "argument-owner-payload";
}
class ArgumentOwnerTargets {
    public static function stat(int $later, string $text): void {}
    public function inst(int $later, string $text): void {}
}
class ArgumentOwnerNested { public function __construct(int $value) {} }
function argumentOwnerCallback(): int { return 1; }
function argumentOwnerDirect(int $later, string $text): void {}
function argumentOwnerByRef(string &$text, int $later): void {}
function argumentOwnerSpread(string $text, int $later): void {}
function argumentOwnerCallable(int $later, callable $callback): void {}
function argumentOwnerNestedSink(string $text, mixed $later): void {}
function argumentOwnerMakeSpread(string $text): array { return [$text]; }
function argumentOwnerAlias(string $text): string { return $text; }
function argumentOwnerDescriptor(int $value): int { return $value; }
function argumentOwnerThrow(): int { throw new RuntimeException("stop"); }

$source = new ArgumentOwnerSource();
$target = new ArgumentOwnerTargets();
$descriptor = argumentOwnerDescriptor(...);
for ($i = 0; $i < 3; $i++) {
    try { argumentOwnerDirect(text: $source->text, later: argumentOwnerThrow()); }
    catch (RuntimeException $error) { echo "D"; unset($error); }
    try { ArgumentOwnerTargets::stat(text: $source->text, later: argumentOwnerThrow()); }
    catch (RuntimeException $error) { echo "S"; unset($error); }
    try { $target->inst(text: $source->text, later: argumentOwnerThrow()); }
    catch (RuntimeException $error) { echo "I"; unset($error); }
    $callback = $i > 0 ? argumentOwnerCallback(...) : argumentOwnerCallback(...);
    try { argumentOwnerCallable(callback: $callback, later: argumentOwnerThrow()); }
    catch (RuntimeException $error) { echo "C"; unset($error); }
    unset($callback);

    $byRef = "reference-place";
    try { argumentOwnerByRef($byRef, argumentOwnerThrow()); }
    catch (RuntimeException $error) { echo "R"; unset($error); }
    echo $byRef === "reference-place" ? "V" : "X";
    unset($byRef);

    try {
        argumentOwnerSpread(
            ...argumentOwnerMakeSpread($source->text),
            later: argumentOwnerThrow(),
        );
    } catch (RuntimeException $error) { echo "P"; unset($error); }

    try { argumentOwnerNestedSink($source->text, new ArgumentOwnerNested(argumentOwnerThrow())); }
    catch (RuntimeException $error) { echo "N"; unset($error); }
    try { argumentOwnerNestedSink($source->text, $descriptor(argumentOwnerThrow())); }
    catch (RuntimeException $error) { echo "Q"; unset($error); }

    $alias = argumentOwnerAlias($source->text);
    echo $alias === "argument-owner-payload" ? "A" : "X";
    unset($alias);
}
unset($descriptor, $target, $source);
echo "done";
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(
        out.success,
        "stdout={:?}\nstderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(
        out.stdout,
        format!("{}done", "DSICRVPNQA".repeat(3)),
        "{}",
        out.stderr,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr,
    );
}

/// Sole boxed spreads keep positional and named keys while retiring temporary argument containers.
#[test]
fn test_descriptor_sole_boxed_spread_preserves_keys_and_owners() {
    let source = r#"<?php
function soleSpreadTarget(string $left, string $right): string { return $left . ":" . $right; }
function soleSpreadArguments(bool $named): array {
    if ($named) { return ["right" => "b", "left" => "a"]; }
    return ["a", "b"];
}
function invokeSoleSpread(callable $callback, bool $named): mixed {
    return $callback(...soleSpreadArguments($named));
}
$callback = soleSpreadTarget(...);
for ($i = 0; $i < 3; $i++) {
    echo invokeSoleSpread($callback, false), "|", invokeSoleSpread($callback, true), ";";
}
unset($callback);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a:b|a:b;".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A successful call retires a widened by-reference local's incidental value view exactly once.
#[test]
fn test_widened_string_reference_place_success_path_balances_heap() {
    let source = r#"<?php
function referenceOwnerSuccess(string &$text, int $later): void {}
function referenceOwnerLater(): int { return 1; }

$text = "reference-place";
referenceOwnerSuccess($text, referenceOwnerLater());
echo $text === "reference-place" ? "ok" : "bad";
unset($text);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(
        out.success,
        "stdout={:?}\nstderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(out.stdout, "ok", "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr,
    );
}

/// `substr()` results remain owned when source evaluation is pinned across later arguments.
#[test]
fn test_substr_evaluation_owner_slices_are_independent_and_balance_heap() {
    let source = r#"<?php
function sliceEvaluationLength(): int { return 3; }
function sliceEvaluationAt(string $source, int $offset): string {
    return substr($source, $offset, sliceEvaluationLength());
}

for ($i = 0; $i < 3; $i++) {
    $zero = sliceEvaluationAt("abcdef", 0);
    $nonzero = sliceEvaluationAt("abcdef", 2);
    $empty = sliceEvaluationAt("abcdef", 6);
    $returned = sliceEvaluationAt("abcdef", 1);
    echo $zero, "|", $nonzero, "|", $empty, "|", $returned, "\n";
    unset($zero, $nonzero, $empty, $returned);
}
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(
        out.success,
        "stdout={:?}\nstderr={}",
        out.stdout,
        out.stderr,
    );
    assert_eq!(out.stdout, "abc|cde||bcd\n".repeat(3), "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr,
    );
}
