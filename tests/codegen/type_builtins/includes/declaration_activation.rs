//! Purpose:
//! Integration tests for declaration activation: a function, class, interface, trait, enum or
//! constant compiled into the binary is visible to PHP code only once the source that declares
//! it has executed (or been early-bound), independently of source discovery and inclusion.
//!
//! Called from:
//! - `cargo test` through Rust's test harness (via `type_builtins/includes/mod.rs`).
//!
//! Key details:
//! - `if ($argc < 0) { require ... }` makes the compiler discover and compile a file whose code
//!   never runs, so its declarations must stay inactive at run time.
//! - A file that throws on its first line still early-binds what php early-binds (plain
//!   functions, classes, interfaces and traits without unresolved parents) but not enums,
//!   constants, conditional declarations or declarations in nested includes.
//! - Expected output matches `php -n` on the same files.

use super::*;

/// Verifies creating a first-class callable `unusedCallableProbe(...)` for a function whose file
/// was compiled but never required throws `Error`, even though the callable is never invoked.
/// PHP outputs "caught|done".
#[test]
fn test_runtime_function_first_class_unused_creation_keeps_error() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
if ($argc < 0) { require __DIR__.'/functions.php'; }
try { unusedCallableProbe(...); echo 'unexpected'; }
catch (Error $e) { echo 'caught'; }
echo '|done';
"#),
        ("functions.php", "<?php function unusedCallableProbe() { return 42; }"),
    ], "main.php");
    assert_eq!(out, "caught|done");
}

/// Verifies a first-class callable resolves to the variant from the file actually required when
/// two files declare the same function name: only `right.php` runs, so the callable returns 22.
#[test]
fn test_runtime_function_first_class_creation_selects_active_variant() {
    let out = compile_cli_files_and_run(&[
        ("main.php", "<?php if ($argc < 0) { require __DIR__.'/left.php'; } else { require __DIR__.'/right.php'; } $callback = selectedCallableProbe(...); echo $callback();"),
        ("left.php", "<?php function selectedCallableProbe() { return 11; }"),
        ("right.php", "<?php function selectedCallableProbe() { return 22; }"),
    ], "main.php");
    assert_eq!(out, "22");
}

/// Verifies first-class callable creation follows activation over time: it throws before the
/// declaring file runs and succeeds after `require_once` activates the function. PHP outputs
/// "caught|42".
#[test]
fn test_runtime_function_first_class_creation_tracks_activation() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
if ($argc < 0) { require __DIR__.'/functions.php'; }
try { $callback = firstClassBoundaryProbe(...); echo 'created'; }
catch (Error $e) { echo 'caught|'; }
require_once __DIR__.'/functions.php';
$callback = firstClassBoundaryProbe(...);
echo $callback();
"#),
        ("functions.php", "<?php function firstClassBoundaryProbe() { return 42; }"),
    ], "main.php");
    assert_eq!(out, "caught|42");
}

/// Verifies the activation guard emitted around a call to an included function does not clobber
/// a live `abs(...)` callable held in a local: after `callableBoundaryProbe()` the callback
/// still computes `abs(-42)`, printing "42".
#[test]
fn test_runtime_function_binding_guard_preserves_existing_callback() {
    let out = compile_cli_files_and_run(&[
        ("main.php", "<?php require __DIR__.'/functions.php'; $callback = abs(...); callableBoundaryProbe(); echo $callback(-42);"),
        ("functions.php", "<?php function callableBoundaryProbe() { return 0; }"),
    ], "main.php");
    assert_eq!(out, "42");
}

/// Verifies early binding of interfaces in a file that throws before its declarations: a plain
/// interface is bound, while one extending a parent and one inside an `if` are not. PHP outputs
/// "1:0:0".
#[test]
fn test_interface_early_binding_distinguishes_parent_linking() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
interface ActivationParent {}
try { require __DIR__.'/declarations.php'; } catch (Throwable $e) {}
echo (int) interface_exists('ActivationPlain', false), ':';
echo (int) interface_exists('ActivationChild', false), ':';
echo (int) interface_exists('ActivationConditional', false);
"#),
        ("declarations.php", r#"<?php
throw new Exception('stop');
interface ActivationPlain {}
interface ActivationChild extends ActivationParent {}
if ($argc > 0) { interface ActivationConditional {} }
"#),
    ], "main.php");
    assert_eq!(out, "1:0:0");
}

/// Verifies two conditional declarations of one function with different signatures keep their
/// own signatures: the taken `int $value` branch is called with 41 and returns 42, rather than
/// the string-concatenating alternative.
#[test]
fn test_conditional_function_alternatives_preserve_selected_signature() {
    let source = r#"<?php
if ($argc > 0) {
    function signatureBindingProbe(int $value) { return $value + 1; }
} else {
    function signatureBindingProbe(string $value, string $suffix = '!') { return $value.$suffix; }
}
echo signatureBindingProbe(41);
"#;
    let out = compile_cli_files_and_run(&[("main.php", source)], "main.php");
    assert_eq!(out, "42");
}

/// Verifies same-named functions in the `if` and `else` branches compile to distinct bodies:
/// with either condition `function_exists()` is false before the branch runs, and the call
/// reaches the body of the branch taken ("0:left" or "0:right").
#[test]
fn test_conditional_function_alternatives_keep_distinct_implementations() {
    for (condition, expected) in [("$argc > 0", "0:left"), ("$argc < 0", "0:right")] {
        let source = r#"<?php
echo (int) function_exists('branchBindingProbe'), ':';
if (CONDITION) {
    function branchBindingProbe() { return 'left'; }
} else {
    function branchBindingProbe() { return 'right'; }
}
echo branchBindingProbe();
"#.replace("CONDITION", condition);
        let out = compile_cli_files_and_run(&[("main.php", &source)], "main.php");
        assert_eq!(out, expected, "condition: {condition}");
    }
}

/// Verifies the activation guard also applies to calls made from a method body and from a
/// closure: both throw `Error` before the declaring file is required and return 42 after.
/// PHP outputs "method|closure|42:42".
#[test]
fn test_runtime_function_binding_guard_in_method_and_closure() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
class BindingCaller {
    public function invoke() { return scopedBindingProbe(); }
}
$closure = static function () { return scopedBindingProbe(); };
$caller = new BindingCaller();
try { $caller->invoke(); } catch (Error $e) { echo 'method|'; }
try { $closure(); } catch (Error $e) { echo 'closure|'; }
require __DIR__.'/functions.php';
echo $caller->invoke(), ':', $closure();
"#),
        ("functions.php", "<?php function scopedBindingProbe() { return 42; }"),
    ], "main.php");
    assert_eq!(out, "method|closure|42:42");
}

/// Verifies calling a compiled but inactive function throws "undefined function" before its
/// arguments are evaluated, as php does: neither "argument-ran" nor "body-ran" is printed, only
/// "caught|done".
#[test]
fn test_inactive_compiled_function_call_throws_before_arguments() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
if ($argc < 0) { require __DIR__.'/functions.php'; }
try { inactiveCallProbe(print 'argument-ran'); }
catch (Error $e) { echo 'caught'; }
echo '|done';
"#),
        ("functions.php", "<?php function inactiveCallProbe($value) { echo 'body-ran'; }"),
    ], "main.php");
    assert_eq!(out, "caught|done");
}

/// Verifies a function declared inside a taken `if` is absent to `function_exists()` before the
/// branch executes and both reported and callable after it. PHP outputs "0:1:42".
#[test]
fn test_conditional_function_activation_matches_callable_availability() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
echo (int) function_exists('conditionalBindingProbe'), ':';
if ($argc > 0) {
    function conditionalBindingProbe() { return 42; }
}
echo (int) function_exists('conditionalBindingProbe'), ':', conditionalBindingProbe();
"#),
    ], "main.php");
    assert_eq!(out, "0:1:42");
}

/// Verifies a top-level function in a required file is early-bound when the file is entered, so
/// it exists even though the file throws on its first statement. PHP outputs "1".
#[test]
fn test_file_function_early_binding_before_throw() {
    let out = compile_cli_files_and_run(&[
        ("main.php", "<?php try { require __DIR__.'/definitions.php'; } catch (Throwable $e) {} echo (int) function_exists('fileEarlyFunction');"),
        ("definitions.php", "<?php throw new Exception('stop'); function fileEarlyFunction() { return 1; }"),
    ], "main.php");
    assert_eq!(out, "1");
}

/// Verifies early binding covers every braced namespace block of the entered file: code in
/// `namespace First` already sees `Second\fileEarlyFunction` before it throws. PHP outputs "1".
#[test]
fn test_file_function_early_binding_across_namespaces() {
    let out = compile_cli_files_and_run(&[
        ("main.php", "<?php try { require __DIR__.'/definitions.php'; } catch (Throwable $e) {}"),
        ("definitions.php", "<?php namespace First { echo (int) function_exists('Second\\\\fileEarlyFunction'); throw new \\Exception('stop'); } namespace Second { function fileEarlyFunction() { return 1; } }"),
    ], "main.php");
    assert_eq!(out, "1");
}

/// Verifies early binding of an entered file stops at its own declarations: the parent's
/// function is bound before the throw, but the function of a `require` that never executes is
/// not. PHP outputs "1:0".
#[test]
fn test_file_function_early_binding_does_not_enter_child_include() {
    let out = compile_cli_files_and_run(&[
        ("main.php", "<?php try { require __DIR__.'/parent.php'; } catch (Throwable $e) {} echo (int) function_exists('parentEarlyFunction'), ':', (int) function_exists('childEarlyFunction');"),
        ("parent.php", "<?php throw new Exception('stop'); require __DIR__.'/child.php'; function parentEarlyFunction() { return 1; }"),
        ("child.php", "<?php function childEarlyFunction() { return 2; }"),
    ], "main.php");
    assert_eq!(out, "1:0");
}

/// Verifies `interface_exists()` with a name only known at run time answers false for an
/// interface compiled from a file that was never required. PHP outputs "0".
#[test]
fn test_unentered_interface_runtime_name_is_not_active() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
if ($argc < 0) { require_once __DIR__.'/definitions.php'; }
$name = $argv[1] ?? 'RuntimeBoundaryInterface';
echo (int) interface_exists($name, false);
"#),
        ("definitions.php", "<?php interface RuntimeBoundaryInterface {}"),
    ], "main.php");
    assert_eq!(out, "0");
}

/// Verifies `require_once` of a file activates each kind of class-like declaration it holds:
/// `class_exists`, `interface_exists`, `trait_exists` and `enum_exists` all answer true,
/// "1:1:1:1".
#[test]
fn test_entered_source_activates_its_classlike_declarations() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
require_once __DIR__.'/definitions.php';
echo (int) class_exists('LoadedBoundaryClass', false), ':';
echo (int) interface_exists('LoadedBoundaryInterface', false), ':';
echo (int) trait_exists('LoadedBoundaryTrait', false), ':';
echo (int) enum_exists('LoadedBoundaryEnum', false);
"#),
        ("definitions.php", "<?php class LoadedBoundaryClass {} interface LoadedBoundaryInterface {} trait LoadedBoundaryTrait {} enum LoadedBoundaryEnum { case One; }"),
    ], "main.php");
    assert_eq!(out, "1:1:1:1");
}

/// Verifies which declaration kinds php early-binds in a file that throws first: the class,
/// interface, trait and function exist, while the enum and the `const` (bound only when
/// executed) do not. PHP outputs "1:1:1:0:1:0".
#[test]
fn test_file_early_binding_distinguishes_declaration_kinds() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
try { require __DIR__.'/definitions.php'; } catch (Throwable $e) {}
echo (int) class_exists('EarlyBoundaryClass', false), ':';
echo (int) interface_exists('EarlyBoundaryInterface', false), ':';
echo (int) trait_exists('EarlyBoundaryTrait', false), ':';
echo (int) enum_exists('LateBoundaryEnum', false), ':';
echo (int) function_exists('earlyBoundaryFunction'), ':';
echo (int) defined('LATE_BOUNDARY_CONSTANT');
"#),
        ("definitions.php", r#"<?php
throw new Exception('stop');
class EarlyBoundaryClass {}
interface EarlyBoundaryInterface {}
trait EarlyBoundaryTrait {}
enum LateBoundaryEnum { case One; }
function earlyBoundaryFunction() { return 1; }
const LATE_BOUNDARY_CONSTANT = 1;
"#),
    ], "main.php");
    assert_eq!(out, "1:1:1:0:1:0");
}

/// Verifies a file compiled from an unreachable `require_once` activates none of its class,
/// interface, trait or enum declarations. PHP outputs "0:0:0:0".
#[test]
fn test_unentered_source_does_not_activate_classlike_declarations() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
if ($argc < 0) { require_once __DIR__.'/definitions.php'; }
echo (int) class_exists('BoundaryClass', false), ':';
echo (int) interface_exists('BoundaryInterface', false), ':';
echo (int) trait_exists('BoundaryTrait', false), ':';
echo (int) enum_exists('BoundaryEnum', false);
"#),
        ("definitions.php", "<?php class BoundaryClass {} interface BoundaryInterface {} trait BoundaryTrait {} enum BoundaryEnum { case One; }"),
    ], "main.php");
    assert_eq!(out, "0:0:0:0");
}

/// Verifies entering a file does not activate a function declared inside an `if` whose
/// condition is false there. PHP outputs "0".
#[test]
fn test_entered_source_does_not_activate_unexecuted_conditional_function() {
    let out = compile_cli_files_and_run(&[
        ("main.php", "<?php require_once __DIR__.'/definitions.php'; echo (int) function_exists('boundaryConditionalFunction');"),
        ("definitions.php", "<?php if ($argc < 0) { function boundaryConditionalFunction() { return 1; } }"),
    ], "main.php");
    assert_eq!(out, "0");
}

/// Verifies entering a file does not define a constant whose `define()` sits in an `if` that
/// does not run. PHP outputs "0".
#[test]
fn test_entered_source_does_not_activate_unexecuted_define() {
    let out = compile_cli_files_and_run(&[
        ("main.php", "<?php require_once __DIR__.'/definitions.php'; echo (int) defined('BOUNDARY_CONDITIONAL_CONSTANT');"),
        ("definitions.php", "<?php if ($argc < 0) { define('BOUNDARY_CONDITIONAL_CONSTANT', 1); }"),
    ], "main.php");
    assert_eq!(out, "0");
}

/// Verifies `interface_exists()` sees an interface declared by the ENTRY program itself.
///
/// Only the resolver's include stripping and the autoload pass emitted the activation event the
/// overlay cell needs, and the entry file's own declarations travel neither path: every interface
/// written in the program being compiled answered FALSE, before and after its own declaration.
/// PHP early-binds an interface that extends nothing, so all four answers here are `true` —
/// value-checked against `php -n`.
#[test]
fn test_an_entry_program_interface_is_visible_to_interface_exists() {
    let out = compile_and_run(
        r#"<?php
echo (int) interface_exists('EntryMarker'), ':';
interface EntryMarker {}
echo (int) interface_exists('EntryMarker'), ':';
class EntryWidget implements EntryMarker {}
echo (int) interface_exists('EntryMarker'), ':';
echo (int) class_exists('EntryWidget');
"#,
    );
    assert_eq!(out, "1:1:1:1");
}

/// A class declared under `if (!class_exists(...))` in an INCLUDED file used to be stripped with
/// the file's unconditional declarations and lost outright (Carbon's `DatePeriodBase`).
#[test]
fn test_included_guarded_class_declaration_is_kept() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
namespace Lib;
require __DIR__.'/base.php';
class Period extends PeriodBase {}
echo (new Period())->flavor(), ':', (int) class_exists(PeriodBase::class, false);
"#),
        ("base.php", r#"<?php
namespace Lib;
if (!class_exists(PeriodBase::class, false)) {
    class PeriodBase { public function flavor(): string { return 'base'; } }
}
"#),
    ], "main.php");
    assert_eq!(out, "base:1");
}

/// `require COND ? 'a.php' : 'b.php';` with static branches joins the closed world like an `if`,
/// and a version guard over `PHP_VERSION` (string against float) or `PHP_VERSION_ID` is decided
/// at compile time, so only the live branch's class declaration remains.
#[test]
fn test_ternary_require_version_guard_selects_one_guarded_class() {
    let out = compile_cli_files_and_run(&[
        ("main.php", r#"<?php
namespace Lib;
require PHP_VERSION < 8.2
    ? __DIR__.'/lazy/Old.php'
    : __DIR__.'/lazy/New.php';
require \PHP_VERSION_ID < 80200 ? __DIR__.'/lazy/OldB.php' : __DIR__.'/lazy/NewB.php';
class Period extends PeriodBase {}
echo (new Period())->flavor(), ':', (new OtherBase())->flavor();
"#),
        ("lazy/Old.php", "<?php namespace Lib; if (!class_exists(PeriodBase::class, false)) { class PeriodBase { public function flavor(): string { return 'old'; } } }"),
        ("lazy/New.php", "<?php namespace Lib; if (!class_exists(PeriodBase::class, false)) { class PeriodBase { public function flavor(): string { return 'new'; } } }"),
        ("lazy/OldB.php", "<?php namespace Lib; if (!class_exists(OtherBase::class, false)) { class OtherBase { public function flavor(): string { return 'oldb'; } } }"),
        ("lazy/NewB.php", "<?php namespace Lib; if (!class_exists(OtherBase::class, false)) { class OtherBase { public function flavor(): string { return 'newb'; } } }"),
    ], "main.php");
    assert_eq!(out, "new:newb");
}
