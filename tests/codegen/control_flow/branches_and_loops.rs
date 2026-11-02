//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow branches and loops, including if true, if false, and if else.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies boolean negation evaluates a type-changing assignment expression only once during
/// checking, so the RHS receiver keeps its pre-assignment class contract.
#[test]
fn test_not_assignment_expression_uses_pre_assignment_receiver_type() {
    let out = compile_and_run(
        r#"<?php
class NextValue {}
class ValueSource {
    public function next(): ?NextValue { return null; }
}

$value = new ValueSource();
if (!$value = $value->next()) {
    echo "none";
}
"#,
    );
    assert_eq!(out, "none");
}

/// Verifies branch joins widen heterogeneous array elements inside one array container, allowing
/// a later element reference to remain writable on either path.
#[test]
fn test_branch_join_preserves_array_container_for_element_reference() {
    let out = compile_and_run(
        r#"<?php
class BranchValue {}

function replaceBranchValue(bool $objectBranch): string {
    $values = [];
    if ($objectBranch) {
        $values[] = new BranchValue();
    } else {
        $values[] = "initial";
    }
    $slot = &$values[0];
    $slot = "replaced";
    return $values[0];
}

echo replaceBranchValue(true).":".replaceBranchValue(false);
"#,
    );
    assert_eq!(out, "replaced:replaced");
}

/// Verifies a null final branch does not erase the concrete storage type at the join load.
#[test]
fn test_branch_join_load_uses_concrete_storage_after_null_assignment() {
    let out = compile_and_run(
        r#"<?php
function selectCallback(bool $enabled): callable|null {
    if ($enabled) {
        $callback = static fn (): string => "selected";
    } else {
        $callback = null;
    }
    return $callback;
}

$callback = selectCallback(true);
echo $callback(), ":", selectCallback(false) === null ? "null" : "wrong";
"#,
    );
    assert_eq!(out, "selected:null");
}

/// Verifies that if (true) executes the branch.
#[test]
fn test_if_true() {
    let out = compile_and_run("<?php if (1 == 1) { echo \"yes\"; }");
    assert_eq!(out, "yes");
}

/// Verifies that if (false) skips the branch, producing no output.
#[test]
fn test_if_false() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"yes\"; }");
    assert_eq!(out, "");
}

/// Verifies if/else selects the else branch when the condition is false.
#[test]
fn test_if_else() {
    let out = compile_and_run("<?php if (1 == 2) { echo \"a\"; } else { echo \"b\"; }");
    assert_eq!(out, "b");
}

/// Verifies elseif chain takes the second branch when first condition is false.
#[test]
fn test_if_elseif_else() {
    let out = compile_and_run(
        "<?php $x = 2; if ($x == 1) { echo \"one\"; } elseif ($x == 2) { echo \"two\"; } else { echo \"other\"; }",
    );
    assert_eq!(out, "two");
}

/// Verifies elseif/else falls through to the else branch when all conditions are false.
#[test]
fn test_if_else_falls_through() {
    let out = compile_and_run(
        "<?php $x = 99; if ($x == 1) { echo \"a\"; } elseif ($x == 2) { echo \"b\"; } else { echo \"c\"; }",
    );
    assert_eq!(out, "c");
}

// --- while ---

/// Verifies a basic while loop iterates 5 times and prints 01234.
#[test]
fn test_while_loop() {
    let out = compile_and_run("<?php $i = 0; while ($i < 5) { echo $i; $i = $i + 1; }");
    assert_eq!(out, "01234");
}

/// Verifies while (false) body never executes.
#[test]
fn test_while_zero_iterations() {
    let out = compile_and_run("<?php while (0) { echo \"no\"; }");
    assert_eq!(out, "");
}

/// Verifies break exits the while loop when $i reaches 3, producing 012.
#[test]
fn test_while_break() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 10) { if ($i == 3) { break; } echo $i; $i = $i + 1; }",
    );
    assert_eq!(out, "012");
}

/// Verifies continue skips to the next iteration, skipping the echo at $i==3.
#[test]
fn test_while_continue() {
    let out = compile_and_run(
        "<?php $i = 0; while ($i < 5) { $i = $i + 1; if ($i == 3) { continue; } echo $i; }",
    );
    assert_eq!(out, "1245");
}

/// Verifies break 2 exits both the inner and outer loop from within the innermost loop.
#[test]
fn test_multilevel_break_exits_nested_loops() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    echo "i" . $i . ":";
    for ($j = 0; $j < 3; $j++) {
        if ($i == 1) { break 2; }
        echo $j;
    }
}
echo "end";
"#,
    );
    assert_eq!(out, "i0:012i1:end");
}

/// Verifies continue 2 jumps to the outer loop's update expression, skipping the inner loop body and outer echo.
#[test]
fn test_multilevel_continue_targets_outer_loop_update() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    echo "i" . $i . ":";
    for ($j = 0; $j < 3; $j++) {
        if ($j == 1) { continue 2; }
        echo $j;
    }
    echo "x";
}
echo "end";
"#,
    );
    assert_eq!(out, "i0:0i1:0i2:0end");
}

/// Verifies continue 2 from within a switch targets the outer for loop, not the switch itself.
#[test]
fn test_multilevel_continue_from_switch_targets_outer_loop() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 3; $i++) {
    echo "a";
    switch ($i) {
        case 1:
            echo "b";
            continue 2;
        default:
            echo "c";
    }
    echo "d";
}
"#,
    );
    assert_eq!(out, "acdabacd");
}

/// Verifies break 2 through a try/finally runs the finally block exactly once before exiting.
#[test]
fn test_multilevel_break_through_finally_runs_finally_once() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 2; $i++) {
    for ($j = 0; $j < 2; $j++) {
        try {
            echo "t";
            break 2;
        } finally {
            echo "f";
        }
    }
    echo "x";
}
echo "e";
"#,
    );
    assert_eq!(out, "tfe");
}

// --- for ---

/// Verifies a basic for loop with manual increment in body iterates 5 times and prints 01234.
#[test]
fn test_for_loop() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i = $i + 1) { echo $i; }");
    assert_eq!(out, "01234");
}

/// Verifies comma-separated `for` clauses execute left-to-right, including after `continue`.
#[test]
fn test_for_multiple_init_and_update_expressions() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0, $j = 3; $i < 3; ++$i, --$j) {
    if ($i === 1) { continue; }
    echo $i . ":" . $j . ";";
}
echo $i . ":" . $j;
"#,
    );
    assert_eq!(out, "0:3;2:1;3:0");
}

/// Verifies that call expressions in `for` clauses execute for their side effects.
#[test]
fn test_for_call_expression_init_and_update() {
    let out = compile_and_run(
        r#"<?php
$paths = ['a', 'b', 'c'];
for (next($paths); null !== key($paths); next($paths)) {
    echo current($paths);
}
"#,
    );
    assert_eq!(out, "bc");
}

/// Verifies a literal-led short-circuit assignment statement preserves its RHS side effect.
#[test]
fn test_literal_led_short_circuit_assignment_statement() {
    let out = compile_and_run("<?php $value = -1; 0 > $value && $value += 0x40; echo $value;");
    assert_eq!(out, "63");
}

/// Verifies break exits the for loop when $i reaches 3, producing 012.
#[test]
fn test_for_break() {
    let out = compile_and_run(
        "<?php for ($i = 0; $i < 10; $i = $i + 1) { if ($i == 3) { break; } echo $i; }",
    );
    assert_eq!(out, "012");
}

// --- FizzBuzz ---

/// Verifies nested if/elseif/else chain correctly maps 1–15 to Fizz/Buzz/FizzBuzz/decimal output.
#[test]
fn test_fizzbuzz() {
    let source = r#"<?php
$i = 1;
while ($i <= 15) {
    if ($i % 15 == 0) {
        echo "FizzBuzz\n";
    } elseif ($i % 3 == 0) {
        echo "Fizz\n";
    } elseif ($i % 5 == 0) {
        echo "Buzz\n";
    } else {
        echo $i;
        echo "\n";
    }
    $i = $i + 1;
}
"#;
    let out = compile_and_run(source);
    assert_eq!(
        out,
        "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n"
    );
}

// --- Increment/Decrement ---

/// Verifies for loop with ++$i post-increment in update expression prints 01234.
#[test]
fn test_for_with_increment() {
    let out = compile_and_run("<?php for ($i = 0; $i < 5; $i++) { echo $i; }");
    assert_eq!(out, "01234");
}

/// Verifies pre-increment (++$i) updates before the loop body echo, producing 123.
#[test]
fn test_while_with_pre_increment() {
    let out = compile_and_run("<?php $i = 0; while ($i < 3) { ++$i; echo $i; }");
    assert_eq!(out, "123");
}

// --- Functions ---

/// Verifies null is falsy in an if condition, selecting the else branch.
#[test]
fn test_if_null_is_falsy() {
    let out = compile_and_run(
        r#"<?php
$x = null;
if ($x) {
    echo "true";
} else {
    echo "false";
}
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies null as a while condition prevents loop entry and prints ok.
#[test]
fn test_while_null_no_loop() {
    let out = compile_and_run("<?php $x = null; while ($x) { echo \"bad\"; } echo \"ok\";");
    assert_eq!(out, "ok");
}

/// Makes an assignment on the right of `||` visible when the false outcome selects `else`.
#[test]
fn test_false_or_outcome_exposes_rhs_assignment_in_else() {
    let out = compile_and_run(
        "<?php
        function render(bool $lead): void {
            if ($lead || !($value = 'ready')) {
                echo 'then';
            } else {
                echo $value;
            }
        }
        render(false);",
    );
    assert_eq!(out, "ready");
}

/// Preserves the value assigned as the left operand of an `instanceof` condition.
#[test]
fn test_instanceof_condition_preserves_assignment_effect() {
    let out = compile_and_run(
        "<?php
        class Marker {}
        function render(object $input): void {
            if (($value = $input) instanceof Marker) {
                echo $value::class;
            }
        }
        render(new Marker());",
    );
    assert_eq!(out, "Marker");
}

/// Makes assignments from a successful `&&` chain visible to the true ternary arm.
#[test]
fn test_ternary_true_arm_sees_short_circuit_assignment() {
    let out = compile_and_run(
        "<?php
        $result = true
            && (($value = 'ready') === 'ready')
            && strlen($value) > 0
                ? $value
                : 'missing';
        echo $result;",
    );
    assert_eq!(out, "ready");
}

/// Makes an assignment in a false equality condition visible to the false ternary arm.
#[test]
fn test_ternary_false_arm_sees_condition_assignment() {
    let out = compile_and_run(
        "<?php
        function prefix(string $value): string {
            return false === ($position = strrpos($value, '\\\\'))
                ? ''
                : substr($value, 0, $position);
        }
        echo prefix('Alpha\\\\Beta');",
    );
    assert_eq!(out, "Alpha");
}

/// Preserves a ternary's branch-local assignment inside a property `??=` initializer.
#[test]
fn test_property_null_coalesce_ternary_sees_condition_assignment() {
    let out = compile_and_run(
        "<?php
        final class PrefixCache {
            private ?string $cached = null;
            public function resolve(string $value): string {
                return $this->cached ??= false === ($position = strrpos($value, '\\\\'))
                    ? ''
                    : substr($value, 0, $position);
            }
        }
        echo (new PrefixCache())->resolve('Alpha\\\\Beta');",
    );
    assert_eq!(out, "Alpha");
}

/// Boxes a local whose value changes from null to an indexable object across a loop back-edge.
#[test]
fn test_loop_carried_local_widens_runtime_representation() {
    let out = compile_and_run(
        r#"<?php
class LoopStorageSource {
    public $value;
}
$source = new LoopStorageSource();
$source->value = new ArrayObject(['ready']);
$slot = null;
foreach ([0, 1] as $iteration) {
    if ($slot) {
        echo $slot[0];
    }
    $slot = $source->value;
}
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies a `break`-terminated switch case cannot leak its local type into a sibling case.
#[test]
fn test_switch_break_restores_direct_entry_local_types() {
    let out = compile_and_run(
        r#"<?php
function select_choice(bool $takeFirst, mixed $key): string {
    $choices = ["chosen" => "ok"];
    switch (true) {
        case $takeFirst:
            $key = ["not", "a", "key"];
            break;
        case true:
            return $choices[$key] ?? "missing";
    }
    return "first";
}
echo select_choice(false, "chosen");
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies assignments in a while condition define the logical type on its ordinary exit.
#[test]
fn test_while_exit_uses_condition_assignment_type() {
    let out = compile_and_run(
        r#"<?php
final class LoopKey {
    public function __construct(public string $value) {}
    public function __toString(): string { return $this->value; }
}
$definitions = ["chosen" => "ok"];
$target = new LoopKey("chosen");
$iteration = 0;
while (($target = (string) $target) && $iteration++ < 1) {
    $target = new LoopKey("chosen");
}
echo $definitions[$target];
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies an `if` whose arms leave a by-reference array indexed on one edge and hash-promoted
/// on the other joins at hash storage, so the nested write below the branch reaches the caller.
/// Joined as boxed `Mixed`, the raw hash pointer went down the Mixed write path and every
/// `++$calls[$id][0]` was lost (Symfony's `PhpDumper::getDefinitionsFromArguments`).
#[test]
fn test_if_join_of_indexed_and_hash_ref_param_keeps_nested_writes() {
    let out = compile_and_run(
        r#"<?php
final class Ref {
    public function __construct(public string $id, public int $b = 1) {}
    public function __toString(): string { return $this->id; }
}
final class Walker {
    public function walk(array $arguments, array &$calls = []): void {
        foreach ($arguments as $argument) {
            $id = (string) $argument;
            if (!isset($calls[$id])) {
                $calls[$id] = [0, $argument->b];
            }
            ++$calls[$id][0];
        }
    }
}
$calls = [];
(new Walker())->walk([new Ref("a"), new Ref("s", 0), new Ref("a")], $calls);
echo json_encode($calls);
"#,
    );
    assert_eq!(out, r#"{"a":[2,1],"s":[1,0]}"#);
}

/// Issue #476: a `for` update clause accepts an array push, and the pushed element is the
/// value the clause sees at the end of each iteration.
///
/// The clause used to go through a bespoke mini-grammar that recognised only `$v++`, `++$v`
/// and `$v = / op= / ??= expr`, so `$a[] = $i` failed to PARSE with
/// `Expected '=' after variable name`. PHP prints `3|2`.
#[test]
fn test_for_update_clause_accepts_an_array_push() {
    let out = compile_and_run(
        "<?php $a = []; $i = 0; for (; $i < 3; $a[] = $i) { $i++; } echo count($a), \"|\", $a[1];",
    );
    assert_eq!(out, "3|2");
}

/// Issue #476: both `for` clauses accept a COMMA-SEPARATED list, which is what PHP's grammar
/// has always been. The update form used to fail with `Expected ')' after for clauses` and
/// the init form — not reported on the issue, same gap — with `Expected ';'`.
#[test]
fn test_for_clauses_accept_comma_separated_lists() {
    let update = compile_and_run(
        "<?php $b = []; for ($i = 0; $i < 3; $i++, $b[] = $i) {} echo count($b), \"|\", $b[0];",
    );
    assert_eq!(update, "3|1");

    let init = compile_and_run(
        "<?php for ($x = 0, $y = 10; $x < 2; $x++) {} echo $x, \"|\", $y;",
    );
    assert_eq!(init, "2|10");
}

/// Issue #476: the same gap rejected an indexed assignment and a property assignment in the
/// update clause. Neither is on the issue; both fell into the same mini-grammar error arms
/// (`Expected '=' after variable name` and `Invalid assignment target`).
///
/// The indexed case also pins that the clause runs AFTER the body: `$i` is already 3 on the
/// final pass, so PHP grows the array to four elements rather than three.
#[test]
fn test_for_update_clause_accepts_indexed_and_property_assignment() {
    let indexed = compile_and_run(
        "<?php $c = [0, 0, 0]; for ($i = 0; $i < 3; $c[$i] = $i * 2) { $i++; } echo implode(\",\", $c);",
    );
    assert_eq!(indexed, "0,2,4,6");

    let property = compile_and_run(
        "<?php class Box { public int $n = 0; } $box = new Box(); \
         for ($i = 0; $i < 4; $box->n = $i) { $i++; } echo $box->n;",
    );
    assert_eq!(property, "4");
}

/// Issue #476 regression guard: a comma nested inside a call's argument list, an array
/// literal or an index is NOT a clause separator, and an empty clause still parses.
///
/// This is the half a naive comma split gets wrong — `for ($i = max(0, 1); …)` would be cut
/// in two at the argument comma.
#[test]
fn test_for_clause_commas_respect_nesting_and_empty_clauses() {
    let call = compile_and_run(
        "<?php for ($i = max(0, 1); $i < 3; $i = min($i + 1, 9)) {} echo $i;",
    );
    assert_eq!(call, "3");

    let literal = compile_and_run(
        "<?php for ($p = [1, 2, 3], $i = 0; $i < 2; $i++) {} echo count($p), \"|\", $i;",
    );
    assert_eq!(literal, "3|2");

    let empty = compile_and_run(
        "<?php $n = 0; for (;;) { $n++; if ($n > 2) { break; } } echo $n;",
    );
    assert_eq!(empty, "3");
}

/// Issue #476, the case that pins the depth tracking on BOTH axes at once: a closure in the
/// init clause puts a `;` inside braces (which must not end the clause) ahead of the comma
/// that really separates it from the next piece (which must).
///
/// A slicer that stopped at the first `;`, or split at the first `,`, would cut this clause
/// in half. PHP prints `10|3`.
#[test]
fn test_for_init_clause_accepts_a_closure_containing_a_semicolon() {
    let out = compile_and_run(
        "<?php for ($f = function (int $n): int { return $n * 2; }, $i = 0; $i < 3; $i++) {} \
         echo $f(5), \"|\", $i;",
    );
    assert_eq!(out, "10|3");
}

/// Issue #476 review follow-up: `throw` is an EXPRESSION in PHP 8, so it belongs in a clause.
///
/// The first version of the clause allow-list listed `throw` among the statements to reject,
/// on the reasoning that it is a statement in PHP's grammar. That was true before PHP 8.0 and
/// is not now: `for (throw new LogicException("init"); false; )` and a `throw` in the update
/// clause both run on 8.5.10, and the output below was measured there.
#[test]
fn test_for_clause_throw_is_an_expression() {
    let out = compile_and_run(
        r#"<?php
try {
    for (throw new LogicException("init"); false; ) {
    }
} catch (LogicException $e) {
    echo "init:", $e->getMessage(), "\n";
}

try {
    for ($i = 0; $i < 3; throw new RuntimeException("update")) {
        echo $i;
    }
} catch (RuntimeException $e) {
    echo "|update:", $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(out, "init:init\n0|update:update\n");
}

/// Issue #562: a local that enters a loop holding `null` and is assigned inside it must be
/// typed for BOTH, or the init guard re-runs every iteration.
///
/// A loop body is checked ONCE, with the entry environment, so the header kept the first
/// iteration's `null` for the whole loop and `$n === null` folded to always-true. The fixture
/// below printed `111` where PHP prints `123` — the guard fired every time, so the counter
/// reset instead of accumulating. The same shape is how any `$x = null;` accumulator, parser
/// cursor or lazily built buffer silently produced first-iteration results.
///
/// All four loop forms are here because they reach the header through different code:
/// `do`-`while` runs its body before the condition, and `foreach` binds its own value
/// variable. The widening targets beyond `int` — `null` to `string` through `.=`, `null` to
/// `array` through `[]=` — are here because the join that produces the header type is
/// per-shape. Nested loops check that an inner body's assignment reaches the OUTER header.
///
/// The last block is the control: a loop that never assigns the local must leave it `null` and
/// keep the guard, so the widening is driven by evidence rather than applied to every
/// null-initialized local that a loop happens to read.
///
/// Every expectation is the host PHP 8.5.10 output for the same fixture.
#[test]
fn test_null_local_assigned_inside_a_loop_is_typed_for_both() {
    let out = compile_and_run(
        r#"<?php
$a = null;
for ($i = 0; $i < 3; $i++) {
    if ($a === null) { $a = 0; }
    $a++;
    echo $a;
}
echo "|";

$b = null;
$j = 0;
while ($j < 3) {
    if ($b === null) { $b = 0; }
    $b++;
    echo $b;
    $j++;
}
echo "|";

$c = null;
$k = 0;
do {
    if ($c === null) { $c = 0; }
    $c++;
    echo $c;
    $k++;
} while ($k < 3);
echo "|";

$d = null;
foreach ([1, 2, 3] as $ignored) {
    if ($d === null) { $d = 0; }
    $d++;
    echo $d;
}
echo "|";

$s = null;
for ($i = 0; $i < 3; $i++) {
    if ($s === null) { $s = ""; }
    $s .= "x";
    echo strlen($s);
}
echo "|";

$arr = null;
for ($i = 0; $i < 3; $i++) {
    if ($arr === null) { $arr = []; }
    $arr[] = $i;
    echo count($arr);
}
echo "|";

$n = null;
for ($i = 0; $i < 2; $i++) {
    for ($j = 0; $j < 2; $j++) {
        if ($n === null) { $n = 0; }
        $n++;
    }
    echo $n;
}
echo "|";

$never = null;
for ($i = 0; $i < 2; $i++) {
    if ($never === null) { echo "n"; }
}
var_dump($never);
"#,
    );
    assert_eq!(out, "123|123|123|123|123|123|24|nnNULL\n");
}

// --- Ternary operator ---

/// A statement that starts with a variable but is not an assignment is an expression PHP
/// evaluates for its effects: a ternary choosing a call, `||`/`&&`/`and`/`or` guards including
/// `|| throw`, `??`, and a discarded concatenation. They failed to parse with
/// `Expected '=' after variable name`. Regression for #827 and #841.
#[test]
fn test_variable_led_expression_statements_run() {
    let out = compile_and_run(
        r#"<?php
function left(): void { echo "left\n"; }
function right(): void { echo "right\n"; }
$flag = $argc > 0;
$flag ? left() : right();
$flag ? print "yes\n" : print "no\n";
$valid = $argc > 0;
$valid || throw new RuntimeException('invalid');
$value = 0;
$valid && $value = 5;
echo $value, "\n";
$flag and left();
$flag or right();
$s = "a";
$s . "b";
$flag;
try {
    $zero = $argc - 1;
    $zero || throw new RuntimeException('thrown');
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
$n = null;
$n ?? right();
$x = 3;
$x += 2;
$x .= "!";
echo $x, "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "left\n",
            "yes\n",
            "5\n",
            "left\n",
            "thrown\n",
            "right\n",
            "5!\n",
        )
    );
}

/// A `mixed` switch matches `case null` with PHP's loose comparison while other cases are
/// present. The null case value used to reach `loose_eq` as a pointer type, so the backend
/// refused the whole program (#1994).
#[test]
fn test_mixed_switch_matches_null_alongside_other_cases() {
    let out = compile_and_run(
        r#"<?php
function f(mixed $x): string {
    switch ($x) {
        case 1: return "one";
        case null: return "null";
        case "a": return "a";
        default: return "other";
    }
}
echo f(1), "|", f(null), "|", f(0), "|", f("a"), "|", f("b"), "\n";
"#,
    );
    // PHP 8.5: `1` matches the first case, `0` matches `null` because `0 == null`, `"a"`
    // matches its own case, and `"b"` falls through to the default.
    assert_eq!(out, "one|null|null|a|other\n");
}

/// A `case null` follows PHP's `== null` for every subject type: only the strings/arrays/numbers
/// PHP equates to null match, an int/bool subject keeps the integer fast path with the cast
/// materialized, and packed record addresses keep their pointer identity (#1994).
#[test]
fn test_null_case_matches_php_across_subject_types() {
    let out = compile_and_run(
        r#"<?php
function s_string(string $x): string { switch ($x) { case null: return "null"; case "a": return "a"; default: return "other"; } }
function s_array(): string { $a = []; switch ($a) { case null: return "null"; case 1: return "one"; default: return "other"; } }
function s_nonempty_array(array $a): string { switch ($a) { case null: return "null"; default: return "other"; } }
function s_float(float $x): string { switch ($x) { case null: return "null"; case 1: return "one"; default: return "other"; } }
function s_int(int $x): string { switch ($x) { case null: return "null"; case 1: return "one"; default: return "other"; } }
function s_bool(bool $x): string { switch ($x) { case null: return "null"; case 1: return "one"; default: return "other"; } }
function s_nullable(?int $x): string { switch ($x) { case null: return "null"; case 2: return "two"; default: return "other"; } }
echo s_string("a"), s_string(""), "|", s_array(), "|", s_float(0.0), s_float(1.0), "|",
    s_int(0), s_int(1), "|", s_bool(false), s_bool(true), "|", s_nullable(null), s_nullable(2), "\n";
echo s_string("0"), "|", s_nonempty_array([1]), "|", s_float(0.5), "\n";
"#,
    );
    assert_eq!(out, "anull|null|nullone|nullone|nullone|nulltwo\nother|other|other\n");
}

/// A by-value `foreach` over a boxed `mixed` local iterates a snapshot: unsetting the element
/// being visited does not end the loop (Laravel's `data_get($data, 'tags.0')`).
/// Expected output verified against php 8.5.
#[test]
fn test_foreach_by_value_over_mixed_local_survives_unset_in_body() {
    let out = compile_and_run(
        r#"<?php
function snapshot($key) {
    $key = is_array($key) ? $key : explode('.', $key);
    foreach ($key as $i => $segment) {
        unset($key[$i]);
        echo $i, ':', $segment, ' rest=', count($key), ';';
    }
    echo "\n";
}
snapshot('a.b.c');
snapshot(['x', 'y']);
"#,
    );
    assert_eq!(
        out,
        r#"0:a rest=2;1:b rest=1;2:c rest=0;
0:x rest=1;1:y rest=0;
"#
    );
}

/// `$d instanceof BackedEnum && !is_string($d = $d->value)` compiles when the parameter also
/// receives a string elsewhere (Laravel's `Route::domain()`, `UrlGenerator::route()`).
/// Expected output verified against php 8.5.
#[test]
fn test_enum_value_reassignment_inside_instanceof_guard_compiles() {
    let out = compile_and_run(
        r#"<?php
enum Suit: string { case H = 'h'; }
function name($domain = null) {
    if ($domain instanceof BackedEnum && ! is_string($domain = $domain->value)) {
        throw new InvalidArgumentException('Enum must be string backed.');
    }
    return $domain;
}
echo name(Suit::H), name('x'), "\n";
"#,
    );
    assert_eq!(out, "hx\n");
}
