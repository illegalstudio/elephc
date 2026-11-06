//! Purpose:
//! Regressions for spread transport, selected static arrays and default-prefix exceptions.
//!
//! Called from:
//! - The OOP codegen integration suite.
//!
//! Key details:
//! - Dynamic eval must materialize owned defaults without invoking an unprotected native callback.
//! - Both EIR modes preserve lazy branch selection and exceptions before a deferred scope error.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug, without_ir_opt};

/// Checks output and retirement of native owners after evaluating default trees.
fn check(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Array spreads crossing native eval metadata retain their catchable lexical error.
#[test]
fn test_nested_default_review_boundaries_eval_spread() {
    check(r#"<?php
class SpreadDefaults {
    public function __construct(array $items = [...[parent::A]]) {}
    public static function value(array $items = [...[parent::A]]): int { return count($items); }
    public function instance(array $items = [...[parent::A]]): int { return count($items); }
}
function run(string $source): mixed { return eval($source); }
echo run('try { SpreadDefaults::value(); } catch (Error $e) { return "static"; }'), '|';
echo run('try { (new SpreadDefaults([]))->instance(); } catch (Error $e) { return "instance"; }'), '|';
echo run('try { new SpreadDefaults(); } catch (Error $e) { return "constructor"; }'), '|';
echo run('return SpreadDefaults::value([1, 2]);');
"#, "static|instance|constructor|2");
}

/// Spread transport remains valid without EIR optimization.
#[test]
fn test_nested_default_review_boundaries_eval_spread_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_eval_spread);
}

/// Resolved array defaults must not recompile an unselected missing-parent branch.
#[test]
fn test_nested_default_review_boundaries_static_arrays() {
    check(r#"<?php
class StaticDefaults {
    const ENABLED = true;
    public static array $items = [self::ENABLED ? 'ok' : parent::A];
    public static array $nested = ['key' => [self::ENABLED ? 'yes' : parent::A]];
    public static array $empty = [self::ENABLED ? [] : parent::A];
}
echo json_encode(StaticDefaults::$items), '|', json_encode(StaticDefaults::$nested), '|', json_encode(StaticDefaults::$empty);
"#, "[\"ok\"]|{\"key\":[\"yes\"]}|[[]]");
}

/// Static default resolution is independent of EIR optimization.
#[test]
fn test_nested_default_review_boundaries_static_arrays_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_static_arrays);
}

/// Earlier default constructors throw before later lexical errors on every call surface.
#[test]
fn test_nested_default_review_boundaries_prefix_exceptions() {
    check(r#"<?php
class PrefixValue { public function __construct() { throw new Exception('prefix'); } }
class PrefixMethods {
    public static function value(array $items = [new PrefixValue(), parent::A]): void {}
    public function instance(array $items = [new PrefixValue(), parent::A]): void {}
}
class PrefixConstructor { public function __construct(array $items = [new PrefixValue(), parent::A]) {} }
$object = new PrefixMethods();
try { PrefixMethods::value(); } catch (Exception $e) { echo 'static|'; unset($e); }
try { $object->instance(); } catch (Exception $e) { echo 'instance|'; unset($e); }
try { new PrefixConstructor(); } catch (Exception $e) { echo 'constructor'; unset($e); }
unset($object);
"#, "static|instance|constructor");
}

/// Prefix exception routing remains valid without EIR optimization.
#[test]
fn test_nested_default_review_boundaries_prefix_exceptions_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_prefix_exceptions);
}

/// Native eval spreads preserve explicit numeric keys, string overwrites and lazy selectors.
#[test]
fn test_nested_default_review_boundaries_spread_keys() {
    check(r#"<?php
class SelectedSpread {
    const ENABLED = true;
    public static function value(array $items = [10 => 'ten', ...(self::ENABLED ? [4 => 'four', 'name' => 'old'] : [parent::A]), 'name' => 'new', 'tail']): array { return $items; }
}
function run(string $source): mixed { return eval($source); }
echo json_encode(run('return SelectedSpread::value();'));
"#, "{\"10\":\"ten\",\"11\":\"four\",\"name\":\"new\",\"12\":\"tail\"}");
}

/// Key allocation and lazy spread branches are independent of EIR optimization.
#[test]
fn test_nested_default_review_boundaries_spread_keys_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_spread_keys);
}

/// A throwing inner spread retires both its constructor prefix and the outer partial array.
#[test]
fn test_nested_default_review_boundaries_spread_owners() {
    check(r#"<?php
class Payload {
    public static int $destroyed = 0;
    public function __destruct() { self::$destroyed++; }
}
class SpreadOwner {
    public static function value(array $items = ['prefix', ...[new Payload(), parent::A]]): int { return count($items); }
}
function run(string $source): mixed { return eval($source); }
for ($i = 0; $i < 16; $i++) {
    echo run('try { SpreadOwner::value(); return "bad"; } catch (Error $e) { return "E"; }');
}
echo '|', Payload::$destroyed;
"#, "EEEEEEEEEEEEEEEE|16");
}

/// Spread construction cleans abandoned owners with EIR optimization disabled too.
#[test]
fn test_nested_default_review_boundaries_spread_owners_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_spread_owners);
}

/// A constant-selected missing-parent branch remains a lazy class-initialization Error.
#[test]
fn test_nested_default_review_boundaries_selected_static_error() {
    check(r#"<?php
class DisabledDefaults {
    const ENABLED = false;
    public static array $items = [self::ENABLED ? 'ok' : parent::A];
}
function run(string $source): mixed { return eval($source); }
try { echo json_encode(DisabledDefaults::$items); } catch (Error $e) { echo 'static|'; unset($e); }
try { new DisabledDefaults(); } catch (Error $e) { echo 'new|'; unset($e); }
echo run('try { DisabledDefaults::$items; } catch (Error $e) { return "eval"; }');
"#, "static|new|eval");
}

/// Constant-selected class errors survive both EIR modes.
#[test]
fn test_nested_default_review_boundaries_selected_static_error_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_selected_static_error);
}

/// A nonarray selected spread raises PHP's constant-expression Error instead of bridge failure.
#[test]
fn test_nested_default_review_boundaries_invalid_spread() {
    check(r#"<?php
class SpreadBox {}
class InvalidSpread {
    const ENABLED = true;
    public static function value(array $items = [...(self::ENABLED ? 42 : [parent::A])]): array { return $items; }
}
class InvalidObjectSpread {
    const ENABLED = true;
    public static function value(array $items = [...(self::ENABLED ? new SpreadBox() : [parent::A])]): array { return $items; }
}
function run(string $source): mixed { return eval($source); }
echo run('try { InvalidSpread::value(); } catch (Error $e) { return $e->getMessage(); }');
echo '|';
echo run('try { InvalidObjectSpread::value(); } catch (Error $e) { return $e->getMessage(); }');
"#, "Only arrays can be unpacked in constant expression|Only arrays can be unpacked in constant expression");
}

/// Nonarray spread errors stay catchable with EIR optimization disabled.
#[test]
fn test_nested_default_review_boundaries_invalid_spread_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_invalid_spread);
}

/// Complete-schema selectors include interface constants and private-parent initialization.
#[test]
fn test_nested_default_review_boundaries_private_parent_error() {
    check(r#"<?php
interface Flags { const ENABLED = false; }
class PrivateDefaults implements Flags {
    private array $items = [self::ENABLED ? 'ok' : parent::A];
}
class ChildDefaults extends PrivateDefaults {}
try { new ChildDefaults(); } catch (Error $e) { echo 'child'; unset($e); }
"#, "child");
}

/// Private-parent initialization errors survive both EIR modes.
#[test]
fn test_nested_default_review_boundaries_private_parent_error_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_private_parent_error);
}

/// Unpacking at the largest integer key raises a catchable Error and retires the partial array.
#[test]
fn test_nested_default_review_boundaries_spread_overflow() {
    check(r#"<?php
class OverflowSpread {
    const ENABLED = true;
    public static function value(array $items = (self::ENABLED ? [PHP_INT_MAX => 'max', ...['item']] : [parent::A])): array { return $items; }
}
function run(string $source): mixed { return eval($source); }
echo run('try { OverflowSpread::value(); } catch (Error $e) { return $e->getMessage(); }');
"#, "Cannot add element to the array as the next element is already occupied");
}

/// Spread overflow errors retain their evaluation and cleanup behavior without EIR optimization.
#[test]
fn test_nested_default_review_boundaries_spread_overflow_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_spread_overflow);
}

/// Class initialization failures do not poison reflected defaults or mask visibility failures.
#[test]
fn test_nested_default_review_boundaries_static_metadata_controls() {
    let out = compile_and_run(r#"<?php
class MetadataDefaults {
    const ENABLED = false;
    public static array $items = [self::ENABLED ? 'ok' : parent::A];
    public static int $good = 1;
    private static int $secret = 2;
}
function run(string $source): mixed { return eval($source); }
echo (new ReflectionProperty(MetadataDefaults::class, 'good'))->getDefaultValue(), '|';
echo run('return (new ReflectionProperty("MetadataDefaults", "good"))->getDefaultValue();'), '|';
echo run('try { MetadataDefaults::$good; } catch (Error $e) { return get_class($e); }'), '|';
echo run('try { MetadataDefaults::$good = "bad"; } catch (Error $e) { return get_class($e); }'), '|';
echo run('try { isset(MetadataDefaults::$good); } catch (Error $e) { return "isset"; }'), '|';
echo run('try { empty(MetadataDefaults::$good); } catch (Error $e) { return "empty"; }'), '|';
echo run('return isset(MetadataDefaults::$secret) ? "bad" : "private";'), '|';
echo run('try { MetadataDefaults::$secret; } catch (Error $e) { return $e->getMessage(); }');
"#);
    assert_eq!(out, "1|1|Error|Error|isset|empty|private|Cannot access private property MetadataDefaults::$secret");
}

/// Static metadata precedence is independent of EIR optimization.
#[test]
fn test_nested_default_review_boundaries_static_metadata_controls_no_ir_opt() {
    without_ir_opt(test_nested_default_review_boundaries_static_metadata_controls);
}
