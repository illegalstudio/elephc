//! Purpose:
//! Verifies weak Stringable allow-list binding without weakening strict callable checks.
//!
//! Called from:
//! - The string encoding codegen integration suite.
//!
//! Key details:
//! - Direct, first-class and opaque eval paths must invoke the same conversion once.
//! - Strict callers reject objects before conversion, and every fixture checks native cleanup.

use crate::support::*;

/// Checks Stringable binding output and retirement of conversion temporaries.
fn assert_stringable(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Weak direct and Mixed arguments convert their Stringable allow-list once.
#[test]
fn test_strip_tags_stringable_weak_direct() {
    assert_stringable(r#"<?php
class Allowed {
    public function __toString(): string { echo 'T'; return str_repeat('<p>', 1); }
}
function stripAllowed(mixed $allow): string { return strip_tags('<p>x</p>', $allow); }
try { echo strip_tags('<p>x</p>', new Allowed()), '|', stripAllowed(new Allowed()); }
catch (TypeError $error) { echo 'bad'; }
unset($error);
"#, "T<p>x</p>|T<p>x</p>");
}

/// Weak direct conversion is unchanged without EIR optimization.
#[test]
fn test_strip_tags_stringable_weak_direct_no_ir_opt() {
    without_ir_opt(test_strip_tags_stringable_weak_direct);
}

/// The first-class callable must not reject a weakly coercible object before conversion.
#[test]
fn test_strip_tags_stringable_weak_callable() {
    assert_stringable(r#"<?php
class Allowed {
    public function __toString(): string { echo 'T'; return str_repeat('<p>', 1); }
}
$callable = strip_tags(...);
try { echo $callable('<p>x</p>', new Allowed()); }
catch (TypeError $error) { echo 'bad'; }
unset($error, $callable);
"#, "T<p>x</p>");
}

/// Callable object conversion also retains its source without EIR optimization.
#[test]
fn test_strip_tags_stringable_weak_callable_no_ir_opt() {
    without_ir_opt(test_strip_tags_stringable_weak_callable);
}

/// Runtime-selected eval must convert eval-declared Stringable allow-lists too.
#[test]
fn test_strip_tags_stringable_weak_eval() {
    assert_stringable(r#"<?php
$source = $argc > 0 ? '
class Allowed {
    public function __toString(): string { echo "T"; return str_repeat("<p>", 1); }
}
try { echo strip_tags("<p>x</p>", new Allowed()); }
catch (TypeError $error) { echo "bad"; }
unset($error);
' : '';
eval($source);
"#, "T<p>x</p>");
}

/// Opaque eval object conversion is independent of EIR optimization.
#[test]
fn test_strip_tags_stringable_weak_eval_no_ir_opt() {
    without_ir_opt(test_strip_tags_stringable_weak_eval);
}

/// Strict callable guards reject both scalar and object allow-lists before __toString runs.
#[test]
fn test_strip_tags_stringable_strict_callable_control() {
    assert_stringable(r#"<?php
declare(strict_types=1);
class Allowed {
    public function __toString(): string { echo 'bad'; return '<p>'; }
}
$callable = strip_tags(...);
try { echo $callable('<p>x</p>', 1); echo 'bad'; }
catch (TypeError $error) { echo 'caught|'; }
try { echo $callable('<p>x</p>', new Allowed()); echo 'bad'; }
catch (TypeError $error) { echo 'caught'; }
unset($error, $callable);
"#, "caught|caught");
}

/// Weak union conversion still rejects non-Stringable class objects and closures.
#[test]
fn test_strip_tags_stringable_rejected_objects() {
    assert_stringable(r#"<?php
class NotAllowed {}
function rejectAllowed(mixed $allow): void {
    try { strip_tags('<p>x</p>', $allow); echo 'bad'; }
    catch (TypeError $error) {
        echo str_starts_with($error->getMessage(), 'strip_tags(): Argument #2') ? 'T' : 'bad';
    }
}
rejectAllowed(new NotAllowed());
rejectAllowed(static fn(): int => 1);
"#, "TT");
}

/// An exception from __toString retires both the argument and conversion roots.
#[test]
fn test_strip_tags_stringable_throwing_conversion() {
    assert_stringable(r#"<?php
class Allowed {
    public function __toString(): string { throw new RuntimeException('tag'); }
}
function stripAllowed(mixed $allow): string { return strip_tags(str_repeat('<p>x</p>', 2), $allow); }
try { echo stripAllowed(new Allowed()); } catch (RuntimeException $error) { echo $error->getMessage(); }
unset($error);
$callable = strip_tags(...);
try { echo $callable(str_repeat('<p>x</p>', 2), new Allowed()); }
catch (RuntimeException $error) { echo '|', $error->getMessage(); }
unset($error, $callable);
"#, "tag|tag");
}

/// Conversion exceptions keep the same cleanup with EIR optimization disabled.
#[test]
fn test_strip_tags_stringable_throwing_conversion_no_ir_opt() {
    without_ir_opt(test_strip_tags_stringable_throwing_conversion);
}

/// A callable parameter exercises the descriptor wrapper rather than a tracked direct builtin.
#[test]
fn test_strip_tags_stringable_opaque_callable() {
    assert_stringable(r#"<?php
class Allowed { public function __toString(): string { echo 'T'; return str_repeat('<p>', 1); } }
function invokeAllowed(callable $f, mixed $allow): string { return $f('<p>x</p>', $allow); }
echo invokeAllowed(strip_tags(...), new Allowed());
"#, "T<p>x</p>");
}

/// Opaque callable arguments retain the same owners without EIR optimization.
#[test]
fn test_strip_tags_stringable_opaque_callable_no_ir_opt() {
    without_ir_opt(test_strip_tags_stringable_opaque_callable);
}

/// Physical invocation source, not callable creation source, selects Stringable coercion.
#[test]
fn test_strip_tags_stringable_cross_file_profiles() {
    let out = compile_and_run_files(&[
        ("weak.php", r#"<?php
class Allowed { public function __toString(): string { echo 'T'; return '<p>'; } }
function weakFactory(): callable { return strip_tags(...); }
function weakInvoke(callable $f): string { return $f('<p>x</p>', new Allowed()); }
"#),
        ("strict.php", r#"<?php
declare(strict_types=1);
function strictFactory(): callable { return strip_tags(...); }
function strictInvoke(callable $f): string {
    try { return $f('<p>x</p>', new Allowed()); }
    catch (TypeError $error) { return 'caught'; }
}
function strictForward(callable $f): string {
    try { return call_user_func($f, '<p>x</p>', new Allowed()); }
    catch (TypeError $error) { return 'caught'; }
}
"#),
        ("main.php", r#"<?php
require 'weak.php'; require 'strict.php';
echo weakInvoke(strictFactory()), '|', strictInvoke(weakFactory()), '|', strictForward(weakFactory());
"#),
    ], "main.php");
    assert_eq!(out, "T<p>x</p>|caught|caught");
}

/// Cross-file callable profiles are also preserved without EIR optimization.
#[test]
fn test_strip_tags_stringable_cross_file_profiles_no_ir_opt() {
    without_ir_opt(test_strip_tags_stringable_cross_file_profiles);
}

/// Strict source profile transport must leave clone's lexical scope unchanged.
#[test]
fn test_strip_tags_stringable_strict_profile_clone_control() {
    let out = compile_and_run(r#"<?php
declare(strict_types=1);
class AllowedClone {
    protected function __clone(): void { echo 'hook'; }
    public function invoke(callable $f) { return $f($this); }
}
function outsideClone(callable $f, object $value): void {
    try { $copy = $f($value); echo 'bad'; }
    catch (Error $error) { echo '|caught'; }
}
$value = new AllowedClone();
$f = clone(...);
$copy = $value->invoke($f);
outsideClone($f, $value);
"#);
    assert_eq!(out, "hook|caught");
}
