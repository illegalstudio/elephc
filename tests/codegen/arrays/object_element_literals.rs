//! Purpose:
//! Regression tests for indexed array literals whose items are instances of different classes
//! sharing an ancestor (Code Journal task 80b3dd39).
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - The checker infers the common ancestor as the element of `[First, First, Second]`. EIR
//!   lowering used to build the same literal with boxed `mixed` cells, so a declared
//!   `array<Base>` local, parameter or return read those cells as object pointers (wrong
//!   class, object id 0, garbage properties), and a property store failed in the backend with
//!   `prop_set assigning Array(Mixed)`.
//! - Every fixture runs with the EIR optimizer on and off, under heap debug, so the kept
//!   object storage also proves balanced ownership in both lowering modes.

use crate::support::*;

/// Shared hierarchy: an abstract base holding a nullable reservation, two final siblings with
/// different layouts, a holder with a declared `array<Base>` property and a typed consumer.
const SIBLINGS: &str = r#"<?php
class Token {}
abstract class Base
{
    private ?Token $token = null;
    final public function pending(): bool { return $this->token !== null; }
    final public function reserve(Token $token): void
    {
        if ($this->pending()) { throw new \LogicException('already reserved'); }
        $this->token = $token;
    }
    public function tag(): string { return 'base'; }
}
final class First extends Base
{
    public function __construct(private readonly string $name) {}
    public function tag(): string { return 'first:' . $this->name; }
}
final class Second extends Base
{
    public int $inputs = 7;
    public function tag(): string { return 'second:' . $this->inputs; }
}
final class Holder
{
    public array<Base> $initial = [];
    public function enter(Token $token): void
    {
        foreach ($this->initial as $actor) {
            echo get_class($actor), ':', $actor->tag(), ':', spl_object_id($actor) > 0 ? 'id' : 'zero',
                ':', $actor->pending() ? 'owned' : 'free', "\n";
            $actor->reserve($token);
        }
        foreach ($this->initial as $actor) { echo $actor->pending() ? 'owned' : 'free', '|'; }
        echo "\n";
    }
}
function show(array<Base> $items): void
{
    foreach ($items as $item) { echo get_class($item), ':', $item->tag(), '|'; }
    echo "\n";
}
"#;

/// Compiles `source` with the EIR optimizer on and off, under heap debug, and asserts stdout
/// and a clean heap in both lowering modes.
fn assert_in_both_lowering_modes(source: &str, expected: &str) {
    let runs = [
        ("ir-opt on", compile_and_run_with_heap_debug(source)),
        ("ir-opt off", without_ir_opt(|| compile_and_run_with_heap_debug(source))),
    ];
    for (mode, out) in runs {
        assert!(out.success, "{mode}: {}", out.stderr);
        assert_eq!(out.stdout, expected, "{mode}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", out.stderr);
    }
}

/// Runs `body` after the shared `SIBLINGS` hierarchy in both lowering modes.
fn assert_sibling_program(body: &str, expected: &str) {
    assert_in_both_lowering_modes(&format!("{SIBLINGS}{body}"), expected);
}

/// The reported shape: a declared `array<Base>` local built from sibling locals, assigned to a
/// declared property and iterated. Each element keeps its class, identity and free reservation.
#[test]
fn test_declared_sibling_literal_reaches_property_and_foreach() {
    assert_sibling_program(r#"
function exercise(): void
{
    $holder = new Holder();
    $left = new First('left'); $right = new First('right'); $probe = new Second();
    array<Base> $initial = [$left, $right, $probe]; $holder->initial = $initial;
    echo 'distinct=', $left !== $right && $left !== $probe && $right !== $probe ? 'yes' : 'no', "\n";
    $holder->enter(new Token());
    echo $left->pending() && $right->pending() && $probe->pending() ? 'shared' : 'copied', "\n";
}
exercise();
"#, "distinct=yes\nFirst:first:left:id:free\nFirst:first:right:id:free\nSecond:second:7:id:free\n\
owned|owned|owned|\nshared\n");
}

/// An inferred sibling literal is `array<Base>` for the checker, so it can be stored into a
/// declared property from a local and directly. Both stores failed in the backend before.
#[test]
fn test_inferred_sibling_literal_reaches_declared_property() {
    assert_sibling_program(r#"
function exercise(): void
{
    $left = new First('left'); $probe = new Second();
    $holder = new Holder();
    $initial = [$left, $probe]; $holder->initial = $initial;
    $holder->enter(new Token());
    $direct = new Holder();
    $direct->initial = [new Second(), new First('new'), new Second()];
    $direct->enter(new Token());
}
exercise();
"#, "First:first:left:id:free\nSecond:second:7:id:free\nowned|owned|\n\
Second:second:7:id:free\nFirst:first:new:id:free\nSecond:second:7:id:free\nowned|owned|owned|\n");
}

/// Declared and inferred locals read the literal back by `foreach` and by offset, and pass it
/// to an `array<Base>` parameter, with each element dispatching on its runtime class.
#[test]
fn test_sibling_literal_local_consumers_keep_runtime_classes() {
    assert_sibling_program(r#"
function exercise(): void
{
    $left = new First('left'); $probe = new Second();
    array<Base> $typed = [$left, $probe];
    foreach ($typed as $item) { echo get_class($item), ':', $item->tag(), '|'; }
    echo $typed[1]->tag(), '|', count($typed), "\n";
    show($typed);
    $inferred = [$probe, $left, $probe];
    foreach ($inferred as $item) { echo get_class($item), ':', $item->tag(), '|'; }
    echo $inferred[0]->tag(), '|', $inferred[0] === $inferred[2] ? 'same' : 'other', "\n";
    show($inferred);
}
exercise();
"#, "First:first:left|Second:second:7|second:7|2\nFirst:first:left|Second:second:7|\n\
Second:second:7|First:first:left|Second:second:7|second:7|same\n\
Second:second:7|First:first:left|Second:second:7|\n");
}

/// A `Base`-typed value mixed with child instances joins to `Base` in either order, whether
/// the base-typed item is a declared local or a call result.
#[test]
fn test_base_and_child_literal_mixtures_keep_objects() {
    assert_sibling_program(r#"
function make(): Base { return new First('made'); }
function exercise(): void
{
    $probe = new Second();
    Base $base = new First('base');
    array<Base> $leading = [$base, $probe];
    array<Base> $trailing = [$probe, $base, make()];
    $inferred = [make(), new Second(), $base];
    show($leading); show($trailing); show($inferred);
}
exercise();
"#, "First:first:base|Second:second:7|\nSecond:second:7|First:first:base|First:first:made|\n\
First:first:made|Second:second:7|First:first:base|\n");
}

/// A sibling literal is an `array<Base>` argument, typed return and closure return, and a
/// spread of one class joins with a sibling item on either side.
#[test]
fn test_sibling_literal_argument_return_closure_and_spread() {
    assert_sibling_program(r#"
function build(): array<Base> { return [new First('built'), new Second()]; }
function exercise(): void
{
    $left = new First('left'); $probe = new Second();
    show(build());
    show([new First('arg'), new Second(), new First('tail')]);
    $closure = function () use ($left, $probe) { return [$left, $probe]; };
    show($closure());
    $arrow = fn() => [$probe, $left, $probe];
    foreach ($arrow() as $item) { echo $item->tag(), '|'; }
    echo "\n";
    $firsts = [$left, new First('more')];
    array<Base> $after = [...$firsts, $probe];
    $before = [$probe, ...$firsts];
    show($after); show($before);
}
exercise();
"#, "First:first:built|Second:second:7|\nFirst:first:arg|Second:second:7|First:first:tail|\n\
First:first:left|Second:second:7|\nsecond:7|first:left|second:7|\n\
First:first:left|First:first:more|Second:second:7|\n\
Second:second:7|First:first:left|First:first:more|\n");
}

/// A sibling literal carried into a loop, read there and reassigned to another sibling literal
/// keeps object storage across the header, as does one grown by appends in a loop. The checker
/// rejected the typed calls before with `expects array<Base>, got array<mixed>`.
#[test]
fn test_sibling_literal_carried_through_loops_keeps_object_storage() {
    assert_sibling_program(r#"
function exercise(): void
{
    $left = new First('left'); $right = new First('right'); $probe = new Second();
    $pair = [$left, $probe];
    for ($i = 0; $i < 2; $i++) {
        foreach ($pair as $item) { echo get_class($item), ':', $item->tag(), '|'; }
        echo $pair[1]->tag(), "\n";
        $pair = [$probe, $right, $left];
        show($pair);
    }
    show($pair);
    $grown = [$left, $probe];
    while (count($grown) < 4) { $grown[] = new First('n' . count($grown)); }
    show($grown);
}
exercise();
"#, "First:first:left|Second:second:7|second:7\n\
Second:second:7|First:first:right|First:first:left|\n\
Second:second:7|First:first:right|First:first:left|first:right\n\
Second:second:7|First:first:right|First:first:left|\n\
Second:second:7|First:first:right|First:first:left|\n\
First:first:left|Second:second:7|First:first:n2|First:first:n3|\n");
}

/// A copy of a sibling literal is an ordinary copy-on-write snapshot: appends and offset
/// writes separate it, a subclass write keeps object storage, and a scalar write widens only
/// the written copy.
#[test]
fn test_sibling_literal_copy_on_write_snapshot() {
    assert_sibling_program(r#"
function exercise(): void
{
    $left = new First('left'); $probe = new Second();
    array<Base> $source = [$left, $probe];
    $copy = $source; $copy[] = new First('extra'); $copy[0] = $probe;
    show($source); show($copy);
    $inferred = [$left, $probe];
    $widened = $inferred; $widened[] = 5; $widened[] = 'text';
    show($inferred);
    echo count($widened), ':', $widened[2], ':', $widened[3], ':', $widened[1]->tag(), "\n";
}
exercise();
"#, "First:first:left|Second:second:7|\nSecond:second:7|Second:second:7|First:first:extra|\n\
First:first:left|Second:second:7|\n4:5:text:second:7\n");
}

/// Elements owned only by a sibling literal are destroyed once, in element order, when the
/// literal's last owner goes away.
#[test]
fn test_sibling_literal_destructor_order() {
    assert_in_both_lowering_modes(r#"<?php
abstract class Base
{
    public function __construct(public string $name) {}
    public function __destruct() { echo 'drop ', $this->name, "\n"; }
}
final class First extends Base {}
final class Second extends Base {}
function show(array<Base> $items): void
{
    foreach ($items as $item) { echo get_class($item), ':', $item->name, '|'; }
    echo "\n";
}
function exercise(): void
{
    array<Base> $typed = [new First('a'), new Second('b'), new First('c')];
    show($typed);
    $inferred = [new Second('d'), new First('e')];
    show($inferred);
    $inferred = [];
    echo "cleared\n";
}
exercise();
echo "after\n";
"#, "First:a|Second:b|First:c|\nSecond:d|First:e|\ndrop d\ndrop e\ncleared\n\
drop a\ndrop b\ndrop c\nafter\n");
}

/// An interface-typed value joins with a class implementing that interface, while classes
/// that only share an interface, unrelated classes and a nested literal keep working through
/// boxed `mixed` slots.
#[test]
fn test_interface_join_and_unrelated_class_literals() {
    assert_in_both_lowering_modes(r#"<?php
interface Shape { public function area(): int; }
abstract class Poly implements Shape {}
final class Square extends Poly { public function area(): int { return 4; } }
final class Dot implements Shape { public function area(): int { return 1; } }
final class Stranger { public function area(): int { return 9; } }
function total(array<Shape> $shapes): int
{
    $sum = 0;
    foreach ($shapes as $shape) { $sum += $shape->area(); }
    return $sum;
}
function exercise(): void
{
    Shape $shape = new Dot();
    $inferred = [$shape, new Square()];
    array<Shape> $typed = [new Square(), $shape, new Square()];
    echo total($inferred), ':', total($typed), "\n";
    $implementations = [new Square(), new Dot()];
    foreach ($implementations as $item) { echo get_class($item), ':', $item->area(), '|'; }
    $unrelated = [new Square(), new Stranger()];
    foreach ($unrelated as $item) { echo get_class($item), ':', $item->area(), '|'; }
    echo "\n";
    $errors = [new RuntimeException('a'), new LogicException('b'), new Exception('c')];
    foreach ($errors as $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    echo "\n";
}
exercise();
"#, "5:9\nSquare:4|Dot:1|Square:4|Stranger:9|\nRuntimeException:a|LogicException:b|Exception:c|\n");
}
