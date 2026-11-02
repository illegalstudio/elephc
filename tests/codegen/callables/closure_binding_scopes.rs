//! Purpose:
//! Regression coverage for explicit closure scope after runtime callable selection.
//!
//! Called from:
//! - The callables codegen integration suite.
//!
//! Key details:
//! - Warning handlers may keep or replace a closure without losing explicit private visibility.
//! - Bound descriptors retain independent scope metadata and balanced capture ownership.

use crate::support::*;

/// Integral and fractional keys preserve an unchanged closure's requested private scope.
#[test]
fn test_dynamic_scoped_bind_after_unset_key_diagnostic() {
    for key in ["1.0", "1.5", "runtime_scope_key()"] {
        let source = r#"<?php
function expose_scope_reader(): void { global $peek; }
function runtime_scope_key(): mixed { return 1.5; }
class PrivateScopeVault { private string $code = "open"; }
$hash = ["keep" => 1];
set_error_handler(function (): bool { return true; });
$peek = function () { return $this->code; };
try { unset($hash[UNSET_KEY]); } catch (Error $error) {}
$bound = Closure::bind($peek, new PrivateScopeVault(), PrivateScopeVault::class);
echo $bound(), "\n";
restore_error_handler();
"#.replace("UNSET_KEY", key);
        let out = compile_and_run_with_heap_debug(&source);
        assert!(out.success, "{}\n{}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "open\n");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// An object scope uses its concrete subclass and is evaluated once in PHP argument order.
#[test]
fn test_dynamic_scoped_bind_uses_runtime_object_scope_class() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ObjectScopeBase { private string $value = "base"; }
class ObjectScopeChild extends ObjectScopeBase { private string $value = "child"; }
function bind_object_scope(callable $reader, ObjectScopeBase $object): callable {
    return Closure::bind($reader, $object, $object);
}
$reader = function () { return $this->value; };
$base = bind_object_scope($reader, new ObjectScopeBase());
$child = bind_object_scope($reader, new ObjectScopeChild());
class BoxedPublicScope { public string $value = "public"; }
function bind_boxed_scope(callable $reader, mixed $object, mixed $scope): callable {
    return Closure::bind($reader, $object, $scope);
}
$boxed = bind_boxed_scope($reader, new ObjectScopeChild(), new ObjectScopeChild());
$global = bind_boxed_scope($reader, new BoxedPublicScope(), null);
echo $base(), "|", $child(), "|", $boxed(), "|", $global(), "|";
try { bind_boxed_scope($reader, 42, null); }
catch (TypeError $error) { echo "invalid\n"; }
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "base|child|child|public|invalid\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A replacement closure gets its own private scope, while the original closure stays unchanged.
#[test]
fn test_dynamic_scoped_bind_uses_replaced_private_closure() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReplacedScopeVault {
    private string $code = "old";
    private string $replacement = "new";
}
set_error_handler(function (): bool {
    global $peek;
    $peek = function () { return $this->replacement; };
    return true;
});
$peek = function () { return $this->code; };
$original = $peek;
$hash = ["keep" => 1];
try { unset($hash[1.5]); } catch (Error $error) {}
$bound = Closure::bind($peek, new ReplacedScopeVault(), ReplacedScopeVault::class);
$old = Closure::bind($original, new ReplacedScopeVault(), ReplacedScopeVault::class);
echo $bound(), "|", $old(), "\n";
restore_error_handler();
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "new|old\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Rebinding a runtime-selected descriptor replaces scope without mutating earlier bound copies.
#[test]
fn test_dynamic_scoped_bind_rebinds_existing_scope_variants() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class FirstScopeBox { private string $value = "A"; }
class SecondScopeBox { private string $value = "B"; }
class PublicScopeBox { public string $value = "C"; }
function first_scope(callable $reader): callable {
    return Closure::bind($reader, new FirstScopeBox(), FirstScopeBox::class);
}
function second_scope(callable $reader): callable {
    return $reader->bindTo(new SecondScopeBox(), SecondScopeBox::class);
}
$reader = function () { return $this->value; };
$first = first_scope($reader);
$second = second_scope($first);
$again = first_scope($second);
$global = Closure::bind($second, new PublicScopeBox(), null);
$preserved = Closure::bind($second, new SecondScopeBox(), "static");
echo $first(), $second(), $again(), $global(), $preserved(), "\n";
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "ABACB\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Later argument callbacks may replace the source closure and throw without losing its lease.
#[test]
fn test_dynamic_scoped_bind_roots_selected_operand_across_receiver_callback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ArgumentScopeVault { private string $code = "kept"; }
function argument_scope_receiver(bool $fail): ArgumentScopeVault {
    global $peek;
    $peek = null;
    if ($fail) { throw new RuntimeException("receiver"); }
    return new ArgumentScopeVault();
}
for ($i = 0; $i < 10; $i++) {
    $peek = function () { return $this->code; };
    $bound = Closure::bind($peek, argument_scope_receiver(false), ArgumentScopeVault::class);
    echo $bound(), "|";
    $peek = function () { return $this->code; };
    try { Closure::bind($peek, argument_scope_receiver(true), ArgumentScopeVault::class); }
    catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
}
echo "done\n";
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "kept|receiver|".repeat(10) + "done\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Temporary argument cleanup matches PHP for static binding, receiver binding, and exceptions.
#[test]
fn test_dynamic_scoped_bind_preserves_argument_destructor_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class OrderedScopeVault {
    public function __construct(public string $name) {}
    private string $value = "value";
    public function reader(): callable { return function () { return $this->value; }; }
    public function __destruct() { echo $this->name, "|"; }
}
function ordered_reader(): callable { return (new OrderedScopeVault("old"))->reader(); }
function throwing_scope(): mixed { throw new RuntimeException("caught"); }
$bound = Closure::bind(ordered_reader(), new OrderedScopeVault("new"), new OrderedScopeVault("scope"));
echo "bound|", $bound(), "|";
unset($bound);
echo "done\n";
$bound = ordered_reader()->bindTo(new OrderedScopeVault("new"), new OrderedScopeVault("scope"));
echo "bound|", $bound(), "|";
unset($bound);
echo "done\n";
try { Closure::bind(ordered_reader(), new OrderedScopeVault("new"), throwing_scope()); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
try { ordered_reader()->bindTo(new OrderedScopeVault("new"), throwing_scope()); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "old|scope|bound|value|new|done\nscope|old|bound|value|new|done\nold|new|caught\nnew|old|caught\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Explicit runtime scope keeps the reference-return signature and invoker descriptor intact.
#[test]
fn test_dynamic_scoped_bind_preserves_reference_return_value() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ReferenceScopeVault {
    private array $values = ["item" => 1];
    public function read(): int { return $this->values["item"]; }
}
function bind_reference_scope(callable $reader, ReferenceScopeVault $object): callable {
    return Closure::bind($reader, $object, $object);
}
$reader = function &() { return $this->values; };
$object = new ReferenceScopeVault();
$bound = bind_reference_scope($reader, $object);
$values = $bound();
$values["item"] = 7;
echo $values["item"], "|", $object->read(), "\n";
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "7|1\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A later scope callback cannot change the already selected closure or receiver arguments.
#[test]
fn test_dynamic_scoped_bind_roots_arguments_across_scope_callback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CallbackScopeVault { private string $value = "kept"; }
function callback_scope(bool $fail): CallbackScopeVault {
    global $reader, $receiver;
    $reader = null;
    $receiver = null;
    if ($fail) { throw new RuntimeException("scope"); }
    return new CallbackScopeVault();
}
for ($i = 0; $i < 5; $i++) {
    $reader = function () { return $this->value; };
    $receiver = new CallbackScopeVault();
    $bound = Closure::bind($reader, $receiver, callback_scope(false));
    echo $bound(), "|";
    $reader = function () { return $this->value; };
    $receiver = new CallbackScopeVault();
    try { Closure::bind($reader, $receiver, callback_scope(true)); }
    catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
}
echo "done\n";
"#);
    assert!(out.success, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "kept|scope|".repeat(5) + "done\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
