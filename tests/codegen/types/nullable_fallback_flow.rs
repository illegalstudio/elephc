//! Purpose:
//! Runs nullable fallback guards and coalesced object receivers through native EIR.
//!
//! Called from:
//! - The `codegen_tests` integration test harness.
//!
//! Key details:
//! - Each fixture runs with EIR optimization enabled and disabled.

use super::*;

/// Boxed concrete arrays keep element and key layouts across value-reading builtins.
#[test]
fn test_nullable_fallback_flow_review_typed_array_builtins_runtime() {
    let source = r#"<?php
function typedChunkDefault(bool $flag): void {
    $a = $flag ? [1, 2, 2] : null;
    $a ??= [3, 3];
    $saved = $a;
    foreach (array_chunk($a, 2) as $chunk) { echo $chunk[0] + 1, ","; }
    echo "|";
    foreach (array_chunk($a, 2, true) as $chunk) {
        foreach ($chunk as $key => $value) { echo $key, ":", $value, ";"; }
    }
    echo "|", min($a), ":", max($a), "|", $saved[0], "\n";
}
function typedHashDefault(bool $flag): void {
    $a = $flag ? ['one' => 1, 'two' => 2] : null;
    $a = $a ?? ['fallback' => 3];
    foreach (array_chunk($a, 1, true) as $chunk) {
        foreach ($chunk as $key => $value) { echo $key, ":", $value, ";"; }
    }
    echo "|";
    foreach (array_chunk($a, 1) as $chunk) {
        foreach ($chunk as $key => $value) { echo $key, ":", $value, ";"; }
    }
    echo "|";
    foreach (array_count_values($a) as $key => $count) { echo $key, ":", $count, ";"; }
    echo "\n";
}
function typedStringsDefault(bool $flag): void {
    $a = $flag ? ['x', 'y'] : null;
    $a ??= ['a', 'b'];
    echo $a[0], $a[1], "|";
    vprintf("%s:%s\n", $a);
}
function guardedChunks(bool $flag): void {
    $a = $flag ? [4, 5] : null;
    if ($a === null) { return; }
    foreach (array_chunk($a, 1) as $chunk) { echo $chunk[0]; }
    echo "\n";
}
$flag = $argc > 0;
typedChunkDefault($flag); typedChunkDefault(!$flag);
typedHashDefault($flag); typedHashDefault(!$flag);
typedStringsDefault($flag); typedStringsDefault(!$flag);
guardedChunks($flag); guardedChunks(!$flag);
"#;
    let expected = "2,3,|0:1;1:2;2:2;|1:2|1\n4,|0:3;1:3;|3:3|3\none:1;two:2;|0:1;0:2;|1:1;2:1;\nfallback:3;|0:3;|3:1;\nxy|x:y\nab|a:b\n45\n";
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), expected);
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Dynamic int-return validation stays observable and catchable in every callable body.
#[test]
fn test_nullable_fallback_flow_review_return_boundaries_runtime() {
    let source = r#"<?php
function checkedReturnBoundary(mixed $input): int { return $input; }
class ReturnBoundaryRuntimeBox {
    public static function read(mixed $input): int { return $input; }
}
function recoverReturnBoundary(?int $value, mixed $input): int {
    $value ??= 1;
    $ignored = 0;
    try { $ignored = checkedReturnBoundary($input); }
    catch (TypeError $error) { $value = null; $value ??= 2; }
    echo $ignored, "\n";
    return $value;
}
function explicitReturnBoundary(?int $value, mixed $input): int {
    $value = $value ?? 1;
    $ignored = 0;
    try { $ignored = ReturnBoundaryRuntimeBox::read($input); }
    catch (TypeError $error) { $value = null; $value = $value ?? 2; }
    echo $ignored, "\n";
    return $value;
}
function localReturnBoundary(?int $value, mixed $input): int {
    try { return $input; }
    catch (TypeError $error) { $value ??= 2; return $value; }
}
function closureReturnBoundary(?int $value, mixed $input): int {
    $callback = fn(mixed $item): int => $item;
    $value ??= 1;
    $ignored = 0;
    try { $ignored = $callback($input); }
    catch (TypeError $error) { $value = null; $value ??= 2; }
    echo $ignored, "\n";
    return $value;
}
function discardedReturnBoundary(?int $value, mixed $input): int {
    $value ??= 1;
    try { checkedReturnBoundary($input); }
    catch (TypeError $error) { $value = null; $value ??= 2; }
    return $value;
}
$input = $argc > 0 ? 'bad' : 42;
var_dump(recoverReturnBoundary(null, $input), recoverReturnBoundary(null, 7));
var_dump(explicitReturnBoundary(null, $input), explicitReturnBoundary(null, 7));
var_dump(localReturnBoundary(null, $input), localReturnBoundary(null, 7));
var_dump(closureReturnBoundary(null, $input), closureReturnBoundary(null, 7));
var_dump(discardedReturnBoundary(null, $input), discardedReturnBoundary(null, 7));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "0\n7\nint(2)\nint(1)\n0\n7\nint(2)\nint(1)\nint(2)\nint(7)\n0\n7\nint(2)\nint(1)\nint(2)\nint(1)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Guarded and coalesced array reads convert boxed storage before native min/max reduction.
#[test]
fn test_nullable_fallback_flow_review_boxed_array_reductions_runtime() {
    let source = r#"<?php
function coalescedArrayReduction(?array $value): void {
    $value ??= [];
    $value[] = 5;
    var_dump(min($value), max($value));
}
function explicitArrayReduction(?array $value): void {
    $value = $value ?? [];
    $value[2] = 5;
    var_dump(min($value), max($value));
}
function guardedArrayReduction(?array $value): void {
    if ($value === null) { return; }
    $value[] = 5;
    var_dump(min($value), max($value));
}
coalescedArrayReduction(null); coalescedArrayReduction([1, 2]);
coalescedArrayReduction(['first' => 3, 'second' => 1]);
explicitArrayReduction(null); explicitArrayReduction([1, 2]);
guardedArrayReduction(null); guardedArrayReduction([1, 2]);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "int(5)\nint(5)\nint(1)\nint(5)\nint(1)\nint(5)\nint(5)\nint(5)\nint(1)\nint(5)\nint(1)\nint(5)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Appends and indexed stores in finally widen nullable containers on the continuing path.
#[test]
fn test_nullable_fallback_flow_review_finally_container_runtime() {
    let source = r#"<?php
function appendFinallyContainer(bool $flag): int {
    $value = $flag ? [1, 2] : null;
    try { if ($value === null) { return 0; } }
    finally { $value[] = 'abc'; }
    return strlen($value[2]);
}
function keyedFinallyContainer(bool $flag): int {
    $value = $flag ? [1, 2] : null;
    try { if ($value === null) { return 0; } }
    finally { $value[2] = 'abc'; }
    return strlen($value[2]);
}
function expressionFinallyContainer(bool $flag): int {
    $value = $flag ? [1, 2] : null;
    try { if ($value === null) { return 0; } }
    finally { $ignored = ($value[2] = 'abc'); }
    return strlen($value[2]);
}
function genericFinallyContainer(?array $value): array {
    try { if ($value === null) { return []; } }
    finally { $value[] = 1; }
    return $value;
}
var_dump(count(genericFinallyContainer([2])), count(genericFinallyContainer(null)));
var_dump(appendFinallyContainer(true), appendFinallyContainer(false));
var_dump(keyedFinallyContainer(true), keyedFinallyContainer(false));
var_dump(expressionFinallyContainer(true), expressionFinallyContainer(false));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "int(2)\nint(0)\nint(3)\nint(0)\nint(3)\nint(0)\nint(3)\nint(0)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Finally sees every exit while only its continuing path reaches the following consumer.
#[test]
fn test_nullable_fallback_flow_review_finally_continuation_runtime() {
    let source = r#"<?php
function returnFinallyContinuation(?int $value, bool $closed): int {
    try { if ($closed || $value === null) { return 0; } }
    finally { echo "return\n"; }
    return $value;
}
final class FinallyContinuationService {}
function serviceFinallyContinuation(?FinallyContinuationService $value, bool $closed): FinallyContinuationService {
    try { if ($closed || $value === null) { throw new LogicException('exit'); } }
    finally { echo "service\n"; }
    return $value;
}
function continueFinallyContinuation(?int $value): int {
    $total = 0;
    foreach ([1, 2] as $index) {
        try { if ($value === null) { continue; } }
        finally { echo "continue\n"; }
        $total += $value;
    }
    return $total;
}
function breakFinallyContinuation(?int $value): int {
    $total = 0;
    foreach ([1, 2] as $index) {
        try { if ($value === null) { break; } }
        finally { echo "break\n"; }
        $total += $value;
    }
    return $total;
}
function assignedFinallyContinuation(?int $value, bool $flag): int {
    $value ??= 1;
    try { if ($flag) { $value = null; return 0; } }
    finally { echo "assigned\n"; }
    return $value;
}
function explicitFinallyContinuation(?int $value, bool $flag): int {
    $value = $value ?? 1;
    try { if ($flag) { $value = null; return 0; } }
    finally { echo "explicit\n"; }
    return $value;
}
function storeFinallyContinuation(?int $value, bool $flag): int {
    try { if ($value === null) { return 0; } }
    finally { if ($flag) { $value = 7; } }
    return $value;
}
function copyFinallyContinuation(?int $value, ?int $other): int {
    try { if ($value === null) { return 0; } if ($other === null) { return 0; } }
    finally { $other = 7; $value = $other; }
    return $value;
}
function referenceFinallyContinuation(?int &$value): int {
    try { if ($value === null) { return 0; } }
    finally { echo "reference\n"; }
    return $value;
}
function invokeReferenceFinallyContinuation(?int $value): int {
    return referenceFinallyContinuation($value);
}
function refinedFinallyContinuation(?int $value): int {
    try { echo "refined\n"; }
    finally { if ($value === null) { return 0; } }
    return $value;
}
var_dump(returnFinallyContinuation(null, false), returnFinallyContinuation(3, false));
echo get_class(serviceFinallyContinuation(new FinallyContinuationService(), false)), "\n";
try { serviceFinallyContinuation(null, false); } catch (LogicException $error) { echo "caught\n"; }
var_dump(continueFinallyContinuation(null), continueFinallyContinuation(3));
var_dump(breakFinallyContinuation(null), breakFinallyContinuation(3));
var_dump(assignedFinallyContinuation(null, true), assignedFinallyContinuation(null, false));
var_dump(explicitFinallyContinuation(null, true), explicitFinallyContinuation(null, false));
var_dump(storeFinallyContinuation(2, true), storeFinallyContinuation(2, false));
var_dump(copyFinallyContinuation(2, 3));
var_dump(invokeReferenceFinallyContinuation(5));
var_dump(invokeReferenceFinallyContinuation(null));
var_dump(refinedFinallyContinuation(6), refinedFinallyContinuation(null));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "return\nreturn\nint(0)\nint(3)\nservice\nFinallyContinuationService\nservice\ncaught\ncontinue\ncontinue\ncontinue\ncontinue\nint(0)\nint(6)\nbreak\nbreak\nbreak\nint(0)\nint(6)\nassigned\nassigned\nint(0)\nint(1)\nexplicit\nexplicit\nint(0)\nint(1)\nint(7)\nint(2)\nint(7)\nreference\nint(5)\nreference\nint(0)\nrefined\nrefined\nint(6)\nint(0)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Defaults preserve evaluated null stores, skipped values and Mixed return checks.
#[test]
fn test_nullable_fallback_flow_review_coalesce_default_runtime() {
    let source = r#"<?php
function resetCoalesceDefault(?int &$value): bool { $value = null; return true; }
function consumeCoalesceDefault(bool $flag): void {}
function assignedCoalesceDefault(?int $value, ?bool $other): ?int {
    $value ??= 1;
    $other ??= resetCoalesceDefault($value);
    return $value;
}
function explicitCoalesceDefault(?int $value, ?bool $other): ?int {
    $value = $value ?? 1;
    $other = $other ?? resetCoalesceDefault($value);
    return $value;
}
function guardedCoalesceDefault(?int $value, ?bool $other, bool $closed): ?int {
    if ($closed || $value === null) { return 7; }
    consumeCoalesceDefault($other ??= resetCoalesceDefault($value));
    return $value;
}
function directCoalesceDefault(?int $value, ?bool $other): ?int {
    $value ??= 1;
    $other ??= (($value = null) === null);
    return $value;
}
function skippedCoalesceDefault(?int $value, bool $other): int {
    $value ??= 1;
    $other ??= resetCoalesceDefault($value);
    return $value;
}
var_dump(assignedCoalesceDefault(null, null), assignedCoalesceDefault(null, false));
var_dump(explicitCoalesceDefault(null, null), explicitCoalesceDefault(null, true));
var_dump(guardedCoalesceDefault(2, null, false), guardedCoalesceDefault(2, false, false));
var_dump(directCoalesceDefault(null, null), directCoalesceDefault(null, true));
var_dump(skippedCoalesceDefault(null, false));
"#;
    let mixed = r#"<?php
function resetMixedCoalesceDefault(mixed &$value): bool { $value = null; return true; }
function checkedCoalesceDefault(?int $value, ?bool $other): int {
    $value ??= 1;
    $other ??= resetMixedCoalesceDefault($value);
    return $value;
}
checkedCoalesceDefault(null, null);
"#;
    for optimized in [true, false] {
        let run = || {
            assert_eq!(compile_and_run(source), "NULL\nint(1)\nNULL\nint(1)\nNULL\nint(2)\nNULL\nint(1)\nint(1)\n");
            let error = compile_and_run_expect_failure(mixed);
            assert!(error.contains("Return value must be of type int, null returned"), "{error}");
        };
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Do-while, infinite-loop breaks, exhaustive switches and object references keep exit views.
#[test]
fn test_nullable_fallback_flow_review_must_run_runtime() {
    let source = r#"<?php
function once(?int $value): int {
    do { if ($value === null) { return 0; } } while (false);
    return $value;
}
function forever(?int $value): int {
    while (true) { if ($value === null) { return 0; } break; }
    return $value;
}
function exhaustive(?int $value, int $key): int {
    switch ($key) {
        case 1: if ($value === null) { return 0; } break;
        default: if ($value === null) { return 0; } break;
    }
    return $value;
}
class Box { public int $value = 1; }
function swapBox(Box &$box): void { $box = new Box(); $box->value = 9; }
function replaced(?Box $box): int {
    if ($box === null) { return -1; }
    swapBox($box);
    return $box->value;
}
function finalized(?int $value, bool $flag): ?int {
    $value ??= 1;
    foreach ([1] as $i) {
        try { if ($flag) { $value = null; break; } }
        finally { var_dump($value); }
    }
    return $value;
}
function returnedGuard(?int $value): void {
    $value ??= 1;
    try { if ($value !== null) { $value = null; return; } }
    finally { var_dump($value); }
}
function thrownGuard(?int $value): void {
    $value ??= 1;
    try { if ($value !== null) { $value = null; throw new RuntimeException('failed'); } }
    finally { var_dump($value); }
}
function showInt(int $value): void { var_dump($value); }
function caughtGuard(?int $value): void {
    $value ??= 1;
    try { if ($value !== null) { $value = null; throw new RuntimeException('failed'); } }
    catch (RuntimeException $error) { $value ??= 2; }
    finally { showInt($value); }
}
function isolatedScope(?int $value): void {
    $value ??= 1;
    try { $noop = function (?int $value): void { $value = null; return; }; $noop(null); }
    finally { showInt($value); }
}
var_dump(once(5), once(null), forever(6), forever(null));
var_dump(exhaustive(7, 1), exhaustive(null, 2), replaced(new Box()));
var_dump(finalized(3, true), finalized(3, false));
returnedGuard(3);
try { thrownGuard(3); } catch (RuntimeException $error) { echo get_class($error), "\n"; }
caughtGuard(3); isolatedScope(4);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "int(5)\nint(0)\nint(6)\nint(0)\nint(7)\nint(0)\nint(9)\nNULL\nint(3)\nNULL\nint(3)\nNULL\nNULL\nRuntimeException\nint(2)\nint(4)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Catch entries and never-call unwinding observe the payload written before the throw.
#[test]
fn test_nullable_fallback_flow_review_catch_unwind_runtime() {
    let source = r#"<?php
function failFlow(): never { throw new RuntimeException('failed'); }
function needFlow(int $v): void { var_dump($v); }
function caughtFlow(?int $value): void {
    $value ??= 4;
    try { if ($value !== null) { $value = null; FAILFLOW(); } }
    catch (RuntimeException $error) { echo $value === null ? "NULL\n" : "wrong\n"; $value ??= 2; needFlow($value); }
    finally { needFlow($value); }
    needFlow($value);
}
function unwindFlow(?int $value): void {
    $value = $value ?? 4;
    try { if ($value !== null) { $value = null; failFlow(); } }
    finally { var_dump($value); }
}
caughtFlow(null);
try { unwindFlow(null); } catch (RuntimeException $error) { echo "caught\n"; }
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "NULL\nint(2)\nint(2)\nint(2)\nNULL\ncaught\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Statement callbacks preserve nullable payloads while repaired handlers expose typed reads.
#[test]
fn test_nullable_fallback_flow_review_statement_callbacks_runtime() {
    let source = r#"<?php
class CallbackLabelFlow { public function __toString(): string { throw new RuntimeException('label'); } }
class CallbackBagFlow { public function __set(string $name, mixed $value): void { throw new RuntimeException('set'); } }
function callbackRowsFlow(): iterable { yield 1; throw new RuntimeException('rows'); }
function callbackLabelFlow(?int $value): int {
    $value ??= 5;
    try { $value = null; echo new CallbackLabelFlow(); }
    catch (Throwable $error) { $value ??= 7; return $value; }
    return 0;
}
function callbackBagFlow(?int $value): int {
    $value ??= 5;
    $bag = new CallbackBagFlow();
    try { $value = null; $bag->missing = 1; }
    catch (Throwable $error) { $value ??= 8; return $value; }
    return 0;
}
function callbackRowsValueFlow(?int $value): int {
    $value ??= 5;
    try { foreach (callbackRowsFlow() as $row) { $value = null; continue; } }
    catch (Throwable $error) { $value ??= 9; return $value; }
    return 0;
}
function callbackFlagFlow(?int $value): ?int {
    $value ??= 5;
    $flag = null;
    $set = function () use (&$flag) { $flag = 1; };
    try { $set(); if ($flag === null) { return -1; } $value = null; }
    catch (RuntimeException $error) {}
    return $value;
}
function callbackSwitchFlow(?int $value, int $key): ?int {
    switch ($key) { case 1: $value = null; case 2: return $value; default: return 4; }
}
var_dump(callbackLabelFlow(null), callbackBagFlow(null), callbackRowsValueFlow(null));
var_dump(callbackFlagFlow(null), callbackSwitchFlow(3, 1), callbackSwitchFlow(3, 2));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "int(7)\nint(8)\nint(9)\nNULL\nNULL\nint(3)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Declaration initializers and reference-source calls preserve caught null writes at runtime.
#[test]
fn test_nullable_fallback_flow_review_initializer_runtime() {
    let source = r#"<?php
function failInitializerFlow(): int { throw new RuntimeException('init'); }
class ReferenceInitializerFlow {
    public function &item(): int { throw new RuntimeException('ref'); }
}
function indexInitializerFlow(?int $value): ?int {
    $value ??= 4;
    $array = [1, 2];
    try { $reference = &$array[failInitializerFlow()]; }
    catch (RuntimeException $error) { $value = null; }
    return $value;
}
function methodInitializerFlow(?int $value): ?int {
    $value ??= 4;
    $holder = new ReferenceInitializerFlow();
    try { $reference = &$holder->item(); }
    catch (RuntimeException $error) { $value = null; }
    return $value;
}
function typedInitializerFlow(?int $value): ?int {
    $value ??= 4;
    try { int $local = failInitializerFlow(); }
    catch (RuntimeException $error) { $value = null; }
    return $value;
}
function staticInitializerFlow(?int $value): ?int {
    $value ??= 4;
    try { static $saved = failInitializerFlow(); }
    catch (RuntimeException $error) { $value = null; }
    return $value;
}
var_dump(indexInitializerFlow(1), methodInitializerFlow(1));
var_dump(typedInitializerFlow(1), staticInitializerFlow(1));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "NULL\nNULL\nNULL\nNULL\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Nested stores retain nullable payloads and skipped nullsafe arguments preserve the entry value.
#[test]
fn test_nullable_fallback_flow_review_eager_runtime() {
    let source = r#"<?php
class EagerRuntimeFlow {
    public mixed $item = null;
    public function __construct(mixed $arg = null) {}
    public function take(int $value): void {}
    public function accept(int $value): bool { return true; }
    public function __set(string $name, mixed $value): void { throw new RuntimeException('set'); }
}
function eagerInstanceFlow(?int $value): ?int {
    $value ??= 4;
    if (($value = null) instanceof EagerRuntimeFlow) { echo "wrong\n"; }
    return $value;
}
function eagerCloneFlow(?int $value): ?int {
    $value ??= 4;
    $copy = clone (($value = null) ?? new EagerRuntimeFlow());
    return $value;
}
function eagerFilterFlow(?int $value): ?int {
    $value ??= 4;
    $kept = array_filter([1, 0, 2], $value = null);
    echo count($kept), "\n";
    return $value;
}
function eagerReceiverFlow(?int $unused): EagerRuntimeFlow { return new EagerRuntimeFlow(); }
function eagerCallableFlow(?int $value): ?int {
    $value ??= 4;
    $kept = array_filter([1], eagerReceiverFlow($value = null)->accept(...));
    echo count($kept), "\n";
    return $value;
}
function eagerNullsafeFlow(?int $value, ?EagerRuntimeFlow $receiver): ?int {
    $receiver?->take($value ??= 4);
    return $value;
}
function eagerMatchFlow(?int $value, int $key): ?int {
    $value ??= 4;
    return match ($key) { ($value = null) => 0, default => $value };
}
function eagerSetterFlow(?int $value, EagerRuntimeFlow $receiver, string $property): ?int {
    $value ??= 4;
    try { $receiver->{$property} = 1; }
    catch (RuntimeException $error) { $value = null; }
    return $value;
}
var_dump(eagerInstanceFlow(2), eagerCloneFlow(2));
var_dump(eagerFilterFlow(2), eagerCallableFlow(2));
var_dump(eagerNullsafeFlow(null, null), eagerNullsafeFlow(null, new EagerRuntimeFlow()));
var_dump(eagerMatchFlow(2, 1), eagerSetterFlow(2, new EagerRuntimeFlow(), 'missing'));
"#;
    let dynamic = "<?php class DynamicOperandFlow { public function __construct(mixed $arg = null) {} } function dynamicOperandFlow(?int $value, string $className): ?int { $value ??= 4; new $className($value = null); return $value; } var_dump(dynamicOperandFlow(2, 'DynamicOperandFlow'));";
    for optimized in [true, false] {
        let run = || {
            assert_eq!(compile_and_run(source), "NULL\nNULL\n2\n1\nNULL\nNULL\nNULL\nint(4)\nNULL\nNULL\n");
            assert_eq!(compile_and_run(dynamic), "NULL\n");
        };
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Loop-tail locals and nullable continue-edge references survive checking in execution order.
#[test]
fn test_nullable_fallback_flow_review_loop_tail_runtime() {
    let source = r#"<?php
for ($i = 0; $i < 3; $prev = $i, $i++) { if ($i > 0) { echo $prev, "\n"; } }
for ($i = 0, $acc = 0; $i < 3; $i++, $total = $acc) { $acc += $i; }
echo $total, "\n";
function nextLine(): ?string { return null; }
do { $k = 1; } while (($line = nextLine()) !== null);
var_dump($line);
function bump(?int &$x): void { $x = ($x ?? 0) + 1; }
function run(?int $value): void {
    for ($i = 0; $i < 3; $i++, bump($value)) {
        echo $i, ":", var_export($value, true), "\n";
        if ($value !== null) { continue; }
        $value = null;
        break;
    }
    var_dump($value);
}
function switched(?int $value, int $key): ?int {
    $value ??= 3;
    switch ($key) { case 1: if ($key > 0) { $value = null; break; } break; }
    return $value;
}
function switchedContinue(?int $value, int $key): ?int {
    $value = $value ?? 3;
    foreach ([1, 2] as $i) {
        switch ($key) { case 1: if ($key > 0) { $value = null; continue 2; } break; }
    }
    return $value;
}
function resetValue(?int &$value): void { $value = null; }
function assignedLoop(?int $value, bool $flag): void {
    foreach ([1, 2] as $i) { $value = $flag ? null : 1; }
    resetValue($value);
    var_dump($value);
}
function defaultFirst(?int $value, int $key): ?int {
    $value ??= 5;
    switch ($key) { default: $value = null; case 1: return $value; }
    return $value;
}
run(5); run(null); var_dump(switched(5, 1), switchedContinue(5, 1));
assignedLoop(5, false); assignedLoop(null, true); var_dump(defaultFirst(3, 0));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "0\n1\n3\nNULL\n0:5\n1:6\n2:7\nint(8)\n0:NULL\nNULL\nNULL\nNULL\nNULL\nNULL\nNULL\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Terminal loop stores preserve following complements and joined loop-exit values.
#[test]
fn test_nullable_fallback_flow_review_loop_control_runtime() {
    let source = r#"<?php
function continued(?int $value): int {
    foreach ([1, 2] as $x) {
        if ($value === null) { $value = 3; continue; }
        return $value;
    }
    return 0;
}
function broken(?int $value): int {
    foreach ([1, 2] as $x) {
        if ($value === null) { $value = 3; break; }
        return $value;
    }
    return 0;
}
function nulled(?int $value): ?int {
    $value ??= 1;
    foreach ([1, 2] as $x) {
        if (is_int($value)) { $value = null; break; }
    }
    return $value;
}
function updated(?int $value): int {
    for ($i = 0; $i < 2; $i++) {
        if ($value === null) { $value = 4; continue; }
        return $value;
    }
    return 0;
}
function finalized(?int $value): ?int {
    foreach ([1] as $x) {
        try { if ($value === null) { $value = 3; break; } }
        finally { $value = null; }
    }
    return $value;
}
var_dump(continued(null), continued(5), broken(null), broken(5));
var_dump(nulled(null), updated(null), updated(5), finalized(null));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "int(3)\nint(5)\nint(0)\nint(5)\nNULL\nint(4)\nint(5)\nNULL\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Compound guards agree with nested guards for null, zero and false defaults.
#[test]
fn test_nullable_fallback_flow_scalar_runtime() {
    let source = r#"<?php
function intFallback(?string $raw, ?int $default = null): int {
    if ($raw === null && $default !== null) { return $default; }
    return 7;
}
function boolFallback(?string $raw, ?bool $default = null): bool {
    if (!($raw !== null || $default === null)) { return $default; }
    return true;
}
function nested(?string $raw, ?int $default = null): int {
    if ($raw === null) { if ($default !== null) { return $default; } }
    return 7;
}
foreach ([null, 'present'] as $raw) {
    foreach ([null, 0, 42] as $default) {
        echo intFallback($raw, $default), ':', nested($raw, $default), '|';
    }
    foreach ([null, false, true] as $default) {
        echo boolFallback($raw, $default) ? 'T' : 'F';
    }
    echo "\n";
}
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "7:7|0:0|42:42|TFT\n7:7|7:7|7:7|TTT\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// OR guards retain concrete object values only on the path where every operand is false.
#[test]
fn test_nullable_fallback_flow_object_runtime() {
    let source = r#"<?php
final class Service { public string $name = 'service'; }
function service(?Service $value, bool $closed): Service {
    if ($closed || $value === null) { throw new LogicException('unavailable'); }
    return $value;
}
function attempt(?Service $value, bool $closed): void {
    try { echo service($value, $closed)->name, "\n"; }
    catch (LogicException $error) { echo "unavailable\n"; }
}
attempt(null, false); attempt(new Service(), false);
attempt(null, true); attempt(new Service(), true);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "unavailable\nservice\nunavailable\nunavailable\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Explicit and compound coalescing assignments keep both the receiver and its property precise.
#[test]
fn test_nullable_fallback_flow_coalesce_runtime() {
    let source = r#"<?php
final class Options {
    public readonly string $path;
    public function __construct(string $path = 'default') { $this->path = $path; }
}

function consume(string $path): void { echo $path, "\n"; }
function run(?Options $options = null): void {
    $options = $options ?? new Options();
    consume($options->path);
}
function compound(?Options $options = null): void {
    $options ??= new Options();
    consume($options->path);
}
run(); run(new Options('given'));
compound(); compound(new Options('given'));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "default\ngiven\ndefault\ngiven\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Reference calls keep nullable slots addressable after short-circuit and coalescing refinement.
#[test]
fn test_nullable_fallback_flow_reference_runtime() {
    let source = r#"<?php
function resetValue(?int &$value): bool { $value = null; return true; }
function inspect(?int $value): void {
    echo $value !== null && resetValue($value) ? 'reset:' : 'skip:';
    echo is_null($value) ? 'null|' : 'value|';
    $value ??= 5;
    resetValue($value);
    echo is_null($value) ? "null\n" : "value\n";
}
inspect(null); inspect(2);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "skip:null|null\nreset:null|null\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Expression assignment results preserve nullable integer reference storage after returning a value.
#[test]
fn test_nullable_fallback_flow_review_expression_reference_runtime() {
    for assignment in [
        "consume($value = $value ?? 5);",
        "consume($value ??= 5);",
        "echo ($value = $value ?? 5), '|';",
        "$assigned = ($value ??= 5); echo $assigned, '|';",
    ] {
        let source = format!(r#"<?php
function consume(int $value): void {{ echo $value, '|'; }}
function resetValue(?int &$value): void {{ $value = null; }}
function run(?int $value = null): void {{
    {assignment}
    resetValue($value);
    echo is_null($value) ? "null\n" : "value\n";
}}
run(); run(2);
"#);
        for optimized in [true, false] {
            let run = || assert_eq!(compile_and_run(&source), "5|null\n2|null\n", "{assignment}");
            if optimized { run(); } else { without_ir_opt(run); }
        }
    }
}

/// Normalization after a guard keeps its read type and the original nullable reference slot.
#[test]
fn test_nullable_fallback_flow_review_guarded_store_runtime() {
    let source = r#"<?php
function normalize(?string $value): string {
    if ($value === null) { return ''; }
    $value = trim($value);
    return $value;
}
function resetValue(?int &$value): void { $value = null; }
function run(?int $value): void {
    if ($value === null) { return; }
    $value = 5;
    resetValue($value);
    echo $value ?? 'null', "\n";
}
echo normalize(' ok '), "\n";
run(1);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "ok\nnull\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Throwable joins on OR-true and AND-false paths preserve executable exception handling.
#[test]
fn test_nullable_fallback_flow_review_throwable_runtime() {
    let source = r#"<?php
class A extends Exception {}
class B extends Exception {}
function rethrow(Throwable $error): void {
    if ($error instanceof A || $error instanceof B) { throw $error; }
}
function filter(Throwable $error, bool $closed): void {
    if ($error instanceof A && $closed) { return; }
    throw $error;
}
try { rethrow(new A()); } catch (A $error) { echo "a|"; }
try { rethrow(new B()); } catch (B $error) { echo "b|"; }
try { filter(new B(), false); } catch (B $error) { echo "filtered\n"; }
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "a|b|filtered\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// A reference capture can replace a coalesced value, while a value capture retains its copy.
#[test]
fn test_nullable_fallback_flow_review_capture_runtime() {
    let source = r#"<?php
final class S {}
function cleared(?S $value): ?S {
    $value ??= new S();
    $callback = function () use (&$value) { $value = null; };
    $callback();
    return $value;
}

function copied(?S $value): S {
    $value ??= new S();
    $callback = function () use ($value) { return $value; };
    $value = null;
    return $callback();
}
echo is_null(cleared(null)) ? 'null|' : 'bad|';
echo get_class(copied(null)), "\n";
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "null|S\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Widened values remain usable through matching mixed references and reference captures.
#[test]
fn test_nullable_fallback_flow_review_storage_effects_runtime() {
    let source = r#"<?php
function widen(mixed &$value): void { $value = 'str'; }
function resetMixed(mixed &$value): void { $value = null; }
function captured(?int $value): void {
    $value ??= 1;
    widen($value);
    $callback = function () use (&$value) { echo $value, '|'; };
    $callback();
    echo gettype($value), "\n";
}
function incremented(?int $value): void {
    $value ??= 1;
    ++$value;
    echo gettype($value), ':';
    resetMixed($value);
    echo is_null($value) ? "null\n" : "bad\n";
}
function converted(?int $value): void {
    $value ??= 1;
    settype($value, 'string');
    resetMixed($value);
    echo is_null($value) ? "null\n" : "bad\n";
}
captured(null); incremented(null); incremented(9223372036854775807); converted(null);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "str|string\ninteger:null\ndouble:null\nnull\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Nullable returns and mixed matches observe writes only on the selected expression path.
#[test]
fn test_nullable_fallback_flow_review_conditional_runtime() {
    let source = r#"<?php
final class S {}
function resetObject(?S &$value): bool { $value = null; return true; }
function resetValue(?int &$value): bool { $value = null; return true; }
function widen(mixed &$value): bool { $value = 'str'; return true; }
function objectRun(?S $value, bool $flag): ?S {
    $value ??= new S();
    $r = $flag ? resetObject($value) : false;
    return $value;
}
function integerRun(?int $value, bool $flag): ?int {
    $value ??= 1;
    $flag && resetValue($value);
    return $value;
}
function matchRun(?int $value, int $n): mixed {
    $value ??= 1;
    $r = match ($n) { 1 => widen($value), default => false };
    return $value;
}
var_dump(is_null(objectRun(null, false)), is_null(objectRun(null, true)));
var_dump(integerRun(null, false), integerRun(null, true));
var_dump(matchRun(null, 0), matchRun(null, 1));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "bool(false)\nbool(true)\nint(1)\nNULL\nint(1)\nstring(3) \"str\"\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Incompatible dynamic defaults and bool stores widen an integer/null slot to boxed storage.
#[test]
fn test_nullable_fallback_flow_review_union_stores_runtime() {
    let source = r#"<?php
function explicit(?int $value, mixed $default): void {
    $value = $value ?? $default;
    var_dump($value);
}
function compound(?int $value, mixed $default): void {
    $value ??= $default;
    var_dump($value);
}
function expression(?int $value, mixed $default): void {
    var_dump($value = $value ?? $default);
    var_dump($value);
}
function booleanStore(?int $value): void {
    if ($value !== null) { $value = true; var_dump($value); }
}
explicit(null, 'zz'); compound(null, 1.5); expression(null, 'zz');
explicit(2, 'zz'); booleanStore(1);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "string(2) \"zz\"\nfloat(1.5)\nstring(2) \"zz\"\nstring(2) \"zz\"\nint(2)\nbool(true)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// A conditional mixed-reference null reaches the runtime return check instead of becoming zero.
#[test]
fn test_nullable_fallback_flow_review_conditional_return_error() {
    let source = r#"<?php
function widen(mixed &$value): bool { $value = null; return true; }
function run(?int $value, bool $flag): int {
    $value ??= 1;
    $flag && widen($value);
    return $value;
}
run(null, true);
"#;
    for optimized in [true, false] {
        let run = || {
            let error = compile_and_run_expect_failure(source);
            assert!(error.contains("Return value must be of type int, null returned"), "{error}");
        };
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Destructuring and catch variables can replace a coalesced binding with a fresh value type.
#[test]
fn test_nullable_fallback_flow_review_rebinding_runtime() {
    let source = r#"<?php
function destructured(?int $value): void {
    $value ??= 1;
    [$value] = ['str'];
    var_dump($value);
    $value = 7;
    var_dump($value);
}
function caught(?int $value): void {
    $value ??= 1;
    try { throw new Exception(); }
    catch (Exception $value) {
        $value = new Exception();
        echo get_class($value), "\n";
    }
}
destructured(null); caught(null);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "string(3) \"str\"\nint(7)\nException\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Guarded null stores and matching reference writes preserve nullable values at branch exits.
#[test]
fn test_nullable_fallback_flow_review_guarded_stores_runtime() {
    let source = r#"<?php
function nullIf(?int $value): ?int {
    $value ??= 1;
    if (is_int($value)) { $value = null; }
    return $value;
}
function nullWhile(?int $value): ?int {
    $value ??= 1;
    while (is_int($value)) { $value = null; }
    return $value;
}
function nullTernary(?int $value): ?int {
    $value ??= 1;
    $r = is_int($value) ? ($value = null) : 0;
    return $value;
}
function resetValue(?int &$value): void { $value = null; }
function nestedReference(?int $value): ?int {
    $value ??= 1;
    if ($value !== null) { resetValue($value); }
    return $value;
}
function compatible(?int $value): int {
    $value ??= 1;
    if (is_int($value)) { $value = 2; }
    return $value;
}
var_dump(nullIf(null), nullWhile(null), nullTernary(null), nestedReference(null), compatible(null));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "NULL\nNULL\nNULL\nNULL\nint(2)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Fresh guards publish boxed increment storage before later matching Mixed reference calls.
#[test]
fn test_nullable_fallback_flow_review_fresh_guard_runtime() {
    let source = r#"<?php
function resetValue(mixed &$value): void { $value = null; }
function guardedIf(?int $value): mixed {
    if ($value !== null) { $value++; }
    return $value;
}
function guardedWhile(?int $value): mixed {
    while ($value !== null) { $value++; break; }
    resetValue($value);
    return $value;
}
function guardedTernary(?int $value): mixed {
    $r = $value !== null ? $value++ : 0;
    resetValue($value);
    return $value;
}
function guardedAnd(?int $value): mixed {
    if ($value !== null && $value++ > 100) { echo ''; }
    resetValue($value);
    return $value;
}
var_dump(guardedIf(5), guardedIf(null));
echo gettype(guardedIf(9223372036854775807)), "\n";
var_dump(guardedWhile(5), guardedWhile(null), guardedTernary(5), guardedTernary(null), guardedAnd(5), guardedAnd(null));
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "int(6)\nNULL\ndouble\nNULL\nNULL\nNULL\nNULL\nNULL\nNULL\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// A loop boxes nullable integer storage before an earlier matching Mixed reference call.
#[test]
fn test_nullable_fallback_flow_review_loop_reference_runtime() {
    let source = r#"<?php
function showValue(mixed &$value): void { var_dump($value); }
function run(?int $value): void {
    foreach ([1, 2, 3] as $i) {
        showValue($value);
        $value = $i;
        if ($value !== null) { $value++; }
    }
    var_dump($value);
}
function coalesced(?int $value): void {
    $value ??= 10;
    for ($i = 0; $i < 2; $i++) {
        showValue($value);
        if ($value !== null) { $value++; }
    }
    var_dump($value);
}
run(10); run(null); coalesced(null);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "int(10)\nint(2)\nint(3)\nint(4)\nNULL\nint(2)\nint(3)\nint(4)\nint(10)\nint(11)\nint(12)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Incoming nullable references retain their tagged cell and diagnose an unrepresentable float.
#[test]
fn test_nullable_fallback_flow_review_incoming_reference_runtime() {
    let source = r#"<?php
function update(?int &$value): void { if ($value !== null) { $value++; } }
function run(?int $value): void { update($value); var_dump($value); }
run(5); run(null);
"#;
    for optimized in [true, false] {
        let run = || {
            assert_eq!(compile_and_run(source), "int(6)\nNULL\n");
            let loop_source = source.replace("if ($value !== null) { $value++; }",
                "foreach ([1] as $i) { if ($value !== null) { $value++; } }");
            assert_eq!(compile_and_run(&loop_source), "int(6)\nNULL\n");
            for (update, initial) in [("$value++;", "9223372036854775807"), ("$value--;", "(-9223372036854775807 - 1)")] {
                for body in [format!("if ($value !== null) {{ {update} }}"),
                    format!("foreach ([1] as $i) {{ if ($value !== null) {{ {update} }} }}")]
                {
                    let overflow = format!("<?php function update(?int &$value): void {{ {body} }} function run(?int $value): void {{ update($value); var_dump($value); }} run({initial});");
                    let error = compile_and_run_expect_failure(&overflow);
                    assert!(error.contains("integer overflow") && error.contains("this storage cannot hold"), "{error}");
                }
            }
        };
        if optimized { run(); } else { without_ir_opt(run); }
    }
}

/// Fixed incoming and captured cells can be forwarded after checked integer updates.
#[test]
fn test_nullable_fallback_flow_review_fixed_reference_runtime() {
    let source = r#"<?php
function resetValue(?int &$value): void { $value = null; }
function showValue(?int &$value): void { var_dump($value); }
function incrementForward(?int &$value): void {
    if ($value !== null) { $value++; }
    resetValue($value);
}
function decrementForward(?int &$value): void {
    if ($value !== null) { $value--; }
    showValue($value);
}
function runIncrement(?int $value): void { incrementForward($value); var_dump($value); }
function runDecrement(?int $value): void { decrementForward($value); }
function captured(?int $value): void {
    $show = function () use (&$value) { var_dump($value); };
    if ($value !== null) { $value++; }
    resetValue($value);
    $show();
}
function capturedLoop(?int $value): void {
    $show = function () use (&$value) { var_dump($value); };
    foreach ([1, 2] as $i) {
        resetValue($value);
        $value = $i;
        if ($value !== null) { $value++; }
        $show();
    }
}
function maybeCaptured(?int $value, bool $flag): void {
    if ($flag) {
        $show = function () use (&$value) { var_dump($value); };
        $show();
    }
    if ($value !== null) { $value++; }
    resetValue($value);
    var_dump($value);
}
function nullableValue(?int $value): ?int { return $value; }
function detachedCapture(?int $value): void {
    $show = function () use (&$value) { var_dump($value); };
    unset($value);
    $value = nullableValue(9223372036854775807);
    if ($value !== null) { $value++; }
    echo gettype($value), "\n";
    $show();
}
runIncrement(5); runDecrement(5); runDecrement(null); captured(5); capturedLoop(1);
maybeCaptured(5, false); maybeCaptured(5, true); detachedCapture(5);
"#;
    for optimized in [true, false] {
        let run = || assert_eq!(compile_and_run(source), "NULL\nint(4)\nNULL\nNULL\nint(2)\nint(3)\nNULL\nint(5)\nNULL\ndouble\nint(5)\n");
        if optimized { run(); } else { without_ir_opt(run); }
    }
}
