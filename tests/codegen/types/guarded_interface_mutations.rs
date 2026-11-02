//! Purpose:
//! Check property mutations through flow-narrowed interface receivers.
//!
//! Called from:
//! - `tests/codegen/types.rs`.
//!
//! Key details:
//! - Runtime choices preserve both guard branches with EIR optimization enabled and disabled.

use super::*;

/// A guarded property assignment selects concrete storage without changing the interface local.
#[test]
fn test_guarded_interface_mutation_assignment() {
    let source = r#"<?php
interface Root {}
final class Label implements Root { public int $value = 1; }
final class Plain implements Root {}
function choose(bool $label): Root {
    if ($label) { return new Label(); }
    return new Plain();
}
function update(Root $root): int {
    if (!($root instanceof Label)) { return 0; }
    $root->value = 9;
    return $root->value;
}
echo update(choose($argc > 1));
"#;
    let expected = ["0", "9"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["label"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["label"]])), expected);
}

/// Unset chooses the guarded class before checking target support and clears its typed slot.
#[test]
fn test_guarded_interface_mutation_unset() {
    let source = r#"<?php
interface Root {}
final class Label implements Root { public int $value = 1; }
final class Plain implements Root {}
function choose(bool $label): Root {
    if ($label) { return new Label(); }
    return new Plain();
}
function clear(Root $root): void {
    if ($root instanceof Label) {
        unset($root->value);
        echo isset($root->value) ? "set" : "ok";
        try { echo $root->value; } catch (Error $error) { echo ":uninitialized"; }
    } else { echo "plain"; }
}
clear(choose($argc > 1));
"#;
    let expected = ["plain", "ok:uninitialized"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["label"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["label"]])), expected);
}

/// Dynamic names run before the RHS once, and unset never materializes an uninitialized value.
#[test]
fn test_guarded_interface_mutation_dynamic_order() {
    let source = r#"<?php
interface Root {}
final class Label implements Root { public int $value = 1; }
function propertyName(): string { echo "N"; return "value"; }
function replacement(): int { echo "R"; return 9; }
function mutate(Root $root): void {
    if ($root instanceof Label) {
        $root->{propertyName()} = replacement();
        echo $root->value, ":";
        unset($root->{propertyName()});
        echo isset($root->value) ? "bad" : "unset";
        $root->{'value'} = 3;
        unset($root->{'value'});
        echo ":", $root->value ?? "missing";
    }
}
mutate(new Label());
"#;
    assert_eq!(compile_and_run(source), "NR9:Nunset:missing");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "NR9:Nunset:missing");
}

/// Builtin name expressions keep working when their generic signature has a boxed result.
#[test]
fn test_guarded_interface_mutation_builtin_names() {
    let source = r#"<?php
$object = new stdClass();
$name = json_encode("value");
$object->$name = 1;
unset($object->{json_encode("value")});
echo isset($object->$name) ? "set" : "unset";
$object->value = 2;
unset($object->{json_decode('"value"')});
echo isset($object->value) ? ":set" : ":unset";
interface Root {}
final class Label implements Root { public int $value = 1; }
function clear(Root $root): void {
    if ($root instanceof Label) {
        unset($root->{json_decode('"value"')});
        echo isset($root->value) ? ":set" : ":unset";
    }
}
clear(new Label());
"#;
    assert_eq!(compile_and_run(source), "unset:unset:unset");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "unset:unset:unset");
}

/// Instance and static property receivers keep their interface storage through guarded removal.
#[test]
fn test_guarded_interface_mutation_stored_receivers() {
    let source = r#"<?php
interface Root {}
class Label implements Root { public int $value = 1; }
final class MagicChild extends Label {
    public function __unset(string $name): void { echo "U"; throw new Error("stop"); }
}

final class Plain implements Root {}
final class Holder {
    public Root $item;
    public function __construct(Root $item) { $this->item = $item; }
}
final class StaticHolder { public static Root $item; }
function setStaticHolder(Root $item): void { StaticHolder::$item = $item; }
function clearHolder(Holder $holder): void {
    if ($holder->item instanceof Label) { unset($holder->item->value); }
}
function clearStaticHolder(): void {
    if (StaticHolder::$item instanceof Label) { unset(StaticHolder::$item->value); }
}
function clearMissing(Holder $holder): void {
    if ($holder->item instanceof Label) { unset($holder->item->missing); }
}
for ($i = 0; $i < 3; $i++) {
    $label = new Label();
    $holder = new Holder($label);
    clearHolder($holder);
    echo isset($label->value) ? "bad" : "I";
    $staticLabel = new Label();
    setStaticHolder($staticLabel);
    clearStaticHolder();
    echo isset($staticLabel->value) ? "bad" : "S";
    clearMissing(new Holder(new Label()));
    try { clearMissing(new Holder(new MagicChild())); }
    catch (Error $error) { echo "M"; }
    clearHolder(new Holder(new Plain()));
    setStaticHolder(new Plain());
    clearStaticHolder();
    echo "P";
    unset($label, $holder, $staticLabel, $error);
}
"#;
    for out in [
        compile_and_run_with_heap_debug(source),
        without_ir_opt(|| compile_and_run_with_heap_debug(source)),
    ] {
        assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "ISUMPISUMPISUMP", "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Delayed static writes select the current runtime class after RHS and name side effects.
#[test]
fn test_guarded_interface_mutation_static_write_order() {
    let source = r#"<?php
interface Root {}
class Label implements Root { public int $value = 1; }
class Locked implements Root {
    public readonly int $value;
    public function __construct() { $this->value = 1; }
}
class Other implements Root { public int $padding = 31; public int $value = 1; }
class Holder { public static Root $item; }
function setItem(Root $item): void { Holder::$item = $item; }
function replacement(Root $next): int { echo "R"; setItem($next); return 9; }
function propertyName(Root $next): string { echo "N"; setItem($next); return "value"; }
function namedWrite(): void {
    if (Holder::$item instanceof Label) { Holder::$item->value = 9; }
}
function dynamicWrite(string $name): void {
    if (Holder::$item instanceof Label) { Holder::$item->$name = 8; }
}
function rebindWrite(Root $next): void {
    if (Holder::$item instanceof Label) { Holder::$item->value = replacement($next); }
}
function dynamicRebind(Root $next): void {
    if (Holder::$item instanceof Label) { Holder::$item->{propertyName($next)} = replacement($next); }
}
function invalidValue(): mixed { return [str_repeat("x", 64)]; }
function failingWrite(): void {
    if (Holder::$item instanceof Label) { Holder::$item->value = invalidValue(); }
}
function throwingValue(): int { echo "T"; throw new Error("stop"); }
function throwingWrite(): void {
    if (Holder::$item instanceof Label) { Holder::$item->value = throwingValue(); }
}
function readonlyWrite(): void {
    if (Holder::$item instanceof Locked) { Holder::$item->value = 9; }
}
function readonlyRebind(Root $next): void {
    if (Holder::$item instanceof Locked) { Holder::$item->value = replacement($next); }
}
for ($i = 0; $i < 3; $i++) {
    $left = new Label(); $right = new Other();
    setItem($left); namedWrite(); echo $left->value;
    dynamicWrite("value"); echo $left->value, ":";
    rebindWrite($right);
    echo $left->value, ":", $right->padding, ":", $right->value, ":";
    setItem($left); dynamicRebind($right);
    echo $left->value, ":", $right->padding, ":", $right->value, ":";
    setItem($left);
    try { failingWrite(); } catch (TypeError $typeError) { echo "E"; }
    echo $left->value;
    try { throwingWrite(); } catch (Error $valueError) { echo "C"; }
    $locked = new Locked(); setItem($locked);
    try { readonlyWrite(); } catch (Error $readonlyError) { echo "L"; }
    echo $locked->value, ":";
    $right->value = 1; readonlyRebind($right);
    echo $locked->value, ":", $right->value;
    echo "|";
    unset($left, $right, $locked, $typeError, $valueError, $readonlyError);
}
"#;
    for out in [
        compile_and_run_with_heap_debug(source),
        without_ir_opt(|| compile_and_run_with_heap_debug(source)),
    ] {
        assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, "98:R8:31:9:NR8:31:9:E8TCL1:R1:9|".repeat(3), "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Receiver facts reach method and closure scopes while loop-storage probes omit guard facts.
#[test]
fn test_guarded_interface_mutation_scopes_and_reassignment() {
    let source = r#"<?php
interface Root { public function name(): string; }
final class Label implements Root {
    public int $value = 1;
    public function name(): string { return "label"; }
}
final class Plain implements Root { public function name(): string { return "plain"; } }
class Writer {
    public function update(Root $root): void {
        for ($i = 0; $i < 2; $i++) {
            if ($root instanceof Label) {
                $root->value = $i + 3;
                echo $root->value;
                unset($root->value);
            }
        }
    }
}
function closures(Root $root): void {
    for ($i = 0; $i < 2; $i++) {
        $write = function (Root $item): void {
            if ($item instanceof Label) { $item->value = 8; unset($item->value); }
        };
        $write($root);
        if ($root instanceof Label) {
            $captured = function () use ($root): void { $root->value = 7; unset($root->value); };
            $captured();
        }
    }
}
function choose(bool $label): Root {
    if ($label) { return new Label(); }
    return new Plain();
}
function reassign(Root $root): void {
    if ($root instanceof Label) {
        $original = $root;
        $root->value = 9;
        echo $original->value, ":", $root->value, ":";
        $root = choose(false);
    }
    echo $root->name();
}
$root = choose($argc > 1);
$writer = new Writer();
$writer->update($root);
closures($root);
reassign(choose($argc > 1));
"#;
    let expected = ["plain", "349:9:plain"];
    assert_eq!(compile_and_run_per_argv(source, &[&[], &["label"]]), expected);
    assert_eq!(without_ir_opt(|| compile_and_run_per_argv(source, &[&[], &["label"]])), expected);
}

/// Guarded mutation preserves magic dispatch, readonly errors and RHS-before-error ordering.
#[test]
fn test_guarded_interface_mutation_magic_and_readonly() {
    let source = r#"<?php
interface Root {}
class MagicLabel implements Root {
    public function __set(string $name, mixed $value): void { echo "S", $name, ":", $value, "|"; }
    public function __unset(string $name): void { echo "U", $name, "|"; }
}
class LockedLabel implements Root {
    public readonly int $value;
    public function __construct() { $this->value = 1; }
}
function rhs(): int { echo "R"; return 9; }
function mutate(Root $root): void {
    if ($root instanceof MagicLabel) { $root->value = rhs(); unset($root->value); }
    if ($root instanceof LockedLabel) {
        $root->value ??= rhs();
        try { echo ($root->value = rhs()); } catch (Error $error) { echo $error->getMessage(), "|"; }
        echo $root->value;
    }
}
mutate(new MagicLabel()); mutate(new LockedLabel());
"#;
    let expected = "RSvalue:9|Uvalue|RCannot modify readonly property LockedLabel::$value|1";
    assert_eq!(compile_and_run(source), expected);
    assert_eq!(without_ir_opt(|| compile_and_run(source)), expected);
}
