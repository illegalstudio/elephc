//! Purpose:
//! Regressions for readonly initialization permissions and overwrite diagnostics.
//!
//! Called from:
//! - The native object-oriented codegen suite.
//!
//! Key details:
//! - AOT, eval declarations, and native objects accessed by eval share PHP error precedence.
//! - Heap-debug checks cover normal initialization and catchable write failures.

use crate::support::compile_and_run_with_heap_debug;

/// False union receivers throw before readonly initialization or setter permission checks.
#[test]
fn test_readonly_oct9_false_receiver_error_precedence() {
    for setter in ["", "public(set)"] {
        let source = format!(r#"<?php
class Box {{ public {setter} readonly int $id; }}
function receiver(bool $found): Box|false {{ if ($found) {{ return new Box(); }} return false; }}
function value(): int {{ echo 'rhs|'; return 9; }}
$box = receiver($argc > 100);
try {{ $box->id = value(); echo 'bad'; }}
catch (Error $error) {{ echo $error->getMessage(); }}
"#);
        verify(&source, "rhs|Attempt to assign property \"id\" on false");
    }
}

/// Rejecting a non-object receiver retires a fresh refcounted RHS before the catch.
#[test]
fn test_readonly_oct9_false_receiver_rhs_ownership() {
    verify(r#"<?php
class Box { public public(set) readonly array $id; }
function receiver(bool $found): Box|false { if ($found) { return new Box(); } return false; }
function value(): array { echo 'rhs|'; return [str_repeat('x', 24)]; }
$box = receiver($argc > 100);
try { $box->id = value(); echo 'bad'; }
catch (Error $error) { echo $error->getMessage(); }
"#, "rhs|Attempt to assign property \"id\" on false");
}

/// Checks observable output and balanced heap ownership for one readonly fixture.
fn verify(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Legacy untyped readonly extensions retain constructor initialization and reject external writes.
#[test]
fn test_readonly_initialization_legacy_untyped_constructor() {
    verify(r#"<?php
class LegacyBox {
    public readonly $id;
    public function __construct($id) {
        $this->id = $id;
    }
}
readonly class LegacyReadonly {
    public $id;
    public function __construct($id) { $this->id = $id; }
}
$box = new LegacyBox(7);
$readonly = new LegacyReadonly(42);
echo $box->id, ':', $readonly->id, '|';
try { $box->id = 8; } catch (Error $error) { echo $error->getMessage(); }
"#, "7:42|Cannot modify readonly property LegacyBox::$id");
}

/// Boxed factory unions probe the object's state before rejecting an overwrite or allowing initialization.
#[test]
fn test_readonly_initialization_union_receiver_state() {
    verify(r#"<?php
class Box {
    public readonly int $id;
    public function __construct(int $id) { $this->id = $id; }
}
class PublicBox { public public(set) readonly int $id; }
function box(bool $found): Box|false { if ($found) { return new Box(7); } return false; }
function publicBox(bool $found): PublicBox|false { if ($found) { return new PublicBox(); } return false; }
$box = box($argc > 0);
try { $box->id = 8; } catch (Error $error) { echo $error->getMessage(), ':', $box->id, '|'; }
$public = publicBox($argc > 0);
$public->id = 9;
try { $public->id = 8; } catch (Error $error) { echo $error->getMessage(), ':', $public->id; }
"#, "Cannot modify readonly property Box::$id:7|Cannot modify readonly property PublicBox::$id:9");
}

/// An implicit setter rejects first global writes only after evaluating their RHS.
#[test]
fn test_readonly_initialization_global_direct_and_coalesce() {
    verify(r#"<?php
class Box { public readonly int $id; }
function replacement(): int { echo 'R'; return 2; }
$box = new Box();
try { $box->id = replacement(); } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $box->id ??= replacement(); } catch (Error $error) { echo $error->getMessage(), '|'; }
echo isset($box->id) ? 'set' : 'empty';
"#, "RCannot modify protected(set) readonly property Box::$id from global scope|RCannot modify protected(set) readonly property Box::$id from global scope|empty");
}

/// Inherited protected setters permit child initialization once and name the declaring owner.
#[test]
fn test_readonly_initialization_child_constructor_once() {
    verify(r#"<?php
class Base { public readonly int $id; }
class Child extends Base {
    public function __construct() {
        $this->id = 7;
        try { $this->id = 8; } catch (Error $error) { echo $error->getMessage(), ':'; }
    }
}
$box = new Child();
echo $box->id, '|';
try { $box->id = 9; } catch (Error $error) { echo $error->getMessage(), ':', $box->id; }
"#, "Cannot modify readonly property Base::$id:7|Cannot modify readonly property Base::$id:7");
}

/// The declaring constructor follows the same one-shot rule as an inherited initializer.
#[test]
fn test_readonly_initialization_declaring_constructor_once() {
    verify(r#"<?php
class Box {
    public readonly int $id;
    public function __construct() {
        $this->id = 7;
        try { $this->id = 8; } catch (Error $error) { echo $error->getMessage(), ':'; }
    }
}
$box = new Box();
echo $box->id;
"#, "Cannot modify readonly property Box::$id:7");
}

/// Explicit public setters permit one global initialization without permitting an overwrite.
#[test]
fn test_readonly_initialization_public_set_once() {
    verify(r#"<?php
class Box { public public(set) readonly int $id; }
$box = new Box();
$box->id = 7;
try { $box->id = 8; } catch (Error $error) { echo $error->getMessage(), ':', $box->id; }
"#, "Cannot modify readonly property Box::$id:7");
}

/// Eval first writes report implicit and explicit protected readonly setter diagnostics.
#[test]
fn test_readonly_initialization_eval_uninitialized_setter_errors() {
    verify(r#"<?php
eval('class EvalBox {
    public readonly int $implicit;
    public protected(set) readonly int $explicit;
    public private(set) readonly int $private;
}
$box = new EvalBox();
try { $box->implicit = 2; } catch (Error $error) { echo $error->getMessage(), "|"; }
try { $box->explicit = 2; } catch (Error $error) { echo $error->getMessage(), "|"; }
try { $box->private = 2; } catch (Error $error) { echo $error->getMessage(); }');
"#, "Cannot modify protected(set) readonly property EvalBox::$implicit from global scope|Cannot modify protected(set) readonly property EvalBox::$explicit from global scope|Cannot modify private(set) property EvalBox::$private from global scope");
}

/// Eval inherited and public setters initialize once under their effective write permissions.
#[test]
fn test_readonly_initialization_eval_authorized_initializers() {
    verify(r#"<?php
eval('class EvalBase { public readonly int $id; }
class EvalChild extends EvalBase { public function __construct() { $this->id = 7; } }
class EvalPublic { public public(set) readonly int $id; }
$child = new EvalChild();
$public = new EvalPublic();
$public->id = 9;
echo $child->id, ":", $public->id, "|";
try { $child->id = 8; } catch (Error $error) { echo $error->getMessage(), "|"; }
try { $public->id = 8; } catch (Error $error) { echo $error->getMessage(); }');
"#, "7:9|Cannot modify readonly property EvalBase::$id|Cannot modify readonly property EvalPublic::$id");
}

/// Native objects accessed by eval reject overwrites before restricted setter access.
#[test]
fn test_readonly_initialization_native_eval_error_precedence() {
    verify(r#"<?php
class Box { public readonly int $id; public function __construct() { $this->id = 7; } }
$box = new Box();
eval('try { $box->id = 8; } catch (Error $error) { echo $error->getMessage(), ":", $box->id; }');
"#, "Cannot modify readonly property Box::$id:7");
}

/// Native uninitialized storage accessed by eval uses the same first-write setter diagnostic.
#[test]
fn test_readonly_initialization_native_eval_uninitialized_error() {
    verify(r#"<?php
class Box { public readonly int $id; }
$box = new Box();
eval('try { $box->id = 8; } catch (Error $error) { echo $error->getMessage(); }');
"#, "Cannot modify protected(set) readonly property Box::$id from global scope");
}

/// Eval subclasses initialize inherited native slots and retain native declaring-owner errors.
#[test]
fn test_readonly_initialization_eval_child_native_parent() {
    verify(r#"<?php
class NativeBase { public readonly int $id; }
eval('class EvalChild extends NativeBase {
    public function initialize() { $this->id = 7; }
}
$box = new EvalChild();
$box->initialize();
try { $box->id = 8; } catch (Error $error) { echo $error->getMessage(), ":", $box->id; }');
"#, "Cannot modify readonly property NativeBase::$id:7");
}
