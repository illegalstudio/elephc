//! Purpose:
//! Checks that `=== null` and `!== null` on a boxed value test its runtime tag without boxing
//! the null partner, and that the answers stay PHP's for every value shape.
//!
//! Called from:
//! - The `codegen_tests` runtime GC suite.
//!
//! Key details:
//! - Allocation counts are taken only around the guarded calls (Ivory nullable-object-guard probe).
//! - Falsy scalars, empty containers, nested boxes and references are never identical to null.
//! - Both EIR modes run under heap debugging.

use crate::support::*;

/// Runs one fixture in both EIR modes, requiring clean heaps and returning both stdouts.
fn both_modes(source: &str) -> (String, String) {
    let optimized = compile_and_run_with_heap_debug(source);
    let plain = without_ir_opt(|| compile_and_run_with_heap_debug(source));
    for (mode, output) in [("optimized", &optimized), ("--no-ir-opt", &plain)] {
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
    }
    (optimized.stdout, plain.stdout)
}

/// A guarded inherited wrapper over a nullable object property allocates nothing per call.
#[test]
fn test_nullable_identity_guards_wrapper_does_not_allocate() {
    let source = r#"<?php
final class Ctx {}
abstract class Behavior
{
    private ?Ctx $context = null;
    private bool $removing = false;
    final public function bind(?Ctx $context): void { $this->context = $context; }
    final public function update(float $dt): void
    {
        if ($this->context !== null && !$this->removing) { $this->onUpdate($dt); }
        if ($this->context === null) { $this->onDetached(); }
    }
    protected function onUpdate(float $dt): void {}
    protected function onDetached(): void {}
}
final class Idle extends Behavior {}
final class Logging extends Behavior
{
    protected function onUpdate(float $dt): void { echo "u"; }
    protected function onDetached(): void { echo "d"; }
}
function drive(Behavior $item, int $count): int
{
    $before = memory_get_allocations();
    for ($i = 0; $i < $count; $i++) { $item->update(1.0 / 60.0); }
    return memory_get_allocations() - $before;
}
$item = new Idle();
$item->bind(new Ctx());
echo "bound allocations=", drive($item, $argc + 9999), "\n";
$item->bind(null);
echo "unbound allocations=", drive($item, $argc + 9999), "\n";
$logging = new Logging();
$logging->update(0.5);
$logging->bind(new Ctx());
$logging->update(0.5);
$logging->bind(null);
$logging->update(0.5);
echo "\n";
"#;
    let (optimized, plain) = both_modes(source);
    let expected = "bound allocations=0\nunbound allocations=0\ndud\n";
    assert_eq!(optimized, expected);
    assert_eq!(plain, expected);
}

/// Every boxed value shape answers strict null identity exactly as PHP does, on either side.
#[test]
fn test_nullable_identity_guards_match_php_for_every_shape() {
    let source = r#"<?php
function describe(mixed $value): string
{
    return ($value === null ? 'N' : 'v') . (null !== $value ? 'v' : 'N');
}
$values = [null, 0, 0.0, -0.0, '', '0', false, true, [], [null], new stdClass(), 1.5, 'null'];
$out = '';
foreach ($values as $value) { $out .= describe($value) . ' '; }
$nested = [null];
$ref = &$nested[0];
$out .= describe($nested[0]) . ' ';
$ref = 3;
$out .= describe($nested[0]) . ' ';
$maybe = $argc > 5 ? new stdClass() : null;
$out .= ($maybe === null ? 'null' : 'object');
echo $out;
"#;
    let (optimized, plain) = both_modes(source);
    let expected = "NN vv vv vv vv vv vv vv vv vv vv vv vv NN vv null";
    assert_eq!(optimized, expected);
    assert_eq!(plain, expected);
}
