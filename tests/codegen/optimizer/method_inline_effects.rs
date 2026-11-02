//! Purpose:
//! Executes method splices with nested calls, property writes and string scratch lifetimes.
//!
//! Called from:
//! - The optimizer method-inlining codegen suite.
//!
//! Key details:
//! - Reuses strict heap validation and compares IR optimization on and off.

use super::*;

/// Inherited setters keep their declaring scope while nested static/free calls preserve evaluation.
#[test]
fn test_method_effects_inherited_setters_and_nested_calls() {
    let source = r#"<?php
function outside(int $n): int { for ($i = 0; $i < 2; $i++) { $n = $n + 1; } return $n; }
final class Entity { public static function index(int $id): int { return $id & 255; } }
class Balance {
    private int $value = 0;
    public function set(int $id): void { $this->value = Entity::index($id); }
    public function add(int $amount): void { $this->value = $this->value + $amount; }
    public function value(): int { return $this->value; }
}
final class Counter extends Balance {
    private int $value = 99;
    public function own(): int { return $this->value; }
}
function exercise(Counter $counter, int $id): int {
    $counter->set($id);
    $counter->add(outside($id));
    return $counter->value() * 1000 + $counter->own();
}
echo exercise(new Counter(), $argc), "\n";
"#;
    assert_both_modes(source, true, "4099\n");
}

/// Spliced string writes and directly returned concatenations preserve output and owners.
#[test]
fn test_method_effects_string_building_and_property_write() {
    let source = r#"<?php
final class Label {
    private string $prefix = '<';
    public function prefix(string $prefix): void { $this->prefix = $prefix; }
    public function wrap(string $name): string { return $this->prefix . $name . ']'; }
}
function decorate(Label $label, string $name): string {
    $label->prefix('[');
    return 'head:' . $label->wrap($name) . ':tail';
}
echo decorate(new Label(), 'item'), "\n";
"#;
    assert_both_modes(source, true, "head:[item]:tail\n");
}

/// A failure in a nested real call unwinds the spliced body with balanced owners.
#[test]
fn test_method_effects_nested_call_unwinds_spliced_body() {
    let source = r#"<?php
function nested_failure(int $n): int {
    for ($i = 0; $i < $n; $i++) {}
    throw new RuntimeException('nested');
}
final class Invoke { public function call(int $n): int { return nested_failure($n); } }
function exercise(Invoke $invoke, int $n): int { return $invoke->call($n); }
try { exercise(new Invoke(), $argc); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
"#;
    assert_both_modes(source, true, "nested\n");
}

/// Variance preserves runtime lexical-class semantics while the IR test checks the new guard.
///
/// `self::class` is itself frame-dependent and remains excluded independently of the variance guard.
#[test]
fn test_method_effects_generic_variance_keeps_runtime_semantics() {
    let source = r#"<?php
class VarianceAnimal {}
class VarianceDog extends VarianceAnimal {}
final class CovariantLabel<+T> {
    public function __construct(private T $value) {}
    public function kind(): string { return self::class; }
}
final class ContravariantLabel<-T> {
    public function __construct(private T $value) {}
    public function kind(): string { return self::class; }
}
function describeCovariant(CovariantLabel<VarianceAnimal> $label): string { return $label->kind(); }
function describeContravariant(ContravariantLabel<VarianceDog> $label): string { return $label->kind(); }
echo describeCovariant(new CovariantLabel<VarianceDog>(new VarianceDog())), "\n";
echo describeContravariant(new ContravariantLabel<VarianceAnimal>(new VarianceAnimal())), "\n";
"#;
    assert_both_modes(source, false,
        "CovariantLabel<VarianceDog>\nContravariantLabel<VarianceAnimal>\n");
}

/// Retained local-concat calls preserve scratch bytes used by both builtin and user-call siblings.
#[test]
fn test_method_effects_local_concat_keeps_host_sibling_arguments() {
    let source = r#"<?php
final class LocalLabel {
    public function wrap(string $name): string { $local = '[' . $name; return $local . ']'; }
    public function measure(string $a, string $b): int { $local = $a . $b; return strlen($local); }
}
function show(LocalLabel $label, string $prefix, string $name): void {
    printf("%s|%s\n", $prefix . '!', $label->wrap($name));
}
function combine(string $prefix, int $length, string $suffix): string {
    return $prefix . ':' . $length . ':' . $suffix;
}
$label = new LocalLabel();
$prefix = str_repeat('A', $argc + 4);
$x = str_repeat('x', $argc + 1);
$y = str_repeat('y', $argc + 1);
show($label, 'head', 'item');
echo combine($prefix . 'B', $label->measure($x, $y), $prefix . 'C'), "\n";
"#;
    assert_both_modes(source, true, "head!|[item]\nAAAAAB:4:AAAAAC\n");
}
