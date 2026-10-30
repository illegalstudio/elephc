//! Purpose:
//! Verifies nullable local storage survives coalescing and reference calls on every target.
//!
//! Called from:
//! - The AST-to-EIR library test harness.
//!
//! Key details:
//! - A scalar flow refinement must preserve the parameter's tag and payload representation.

use crate::ir::{Immediate, IrHeapKind, IrType, Op};

/// Coalescing assignments retain nullable parameter loads and emit valid calls on all targets.
#[test]
fn test_nullable_fallback_flow_storage_all_targets() {
    let source = r#"<?php
function containerFinallyTarget(bool $flag): int {
    $value = $flag ? [1, 2] : null;
    try { if ($value === null) { return 0; } }
    finally { $value[] = 'abc'; $value[2] = 'abcd'; }
    return strlen($value[2]);
}
function genericContainerFinallyTarget(?array $value): array {
    try { if ($value === null) { return []; } }
    finally { $value[] = 1; }
    return $value;
}
function boxedArrayReductionTarget(?array $value): int {
    $value ??= [];
    $value[] = 5;
    return max($value);
}
function concreteChunksTarget(bool $flag): void {
    $a = $flag ? [1, 2] : null;
    $a ??= [3, 4];
    $chunks = array_chunk($a, 1);
    $preserved = array_chunk($a, 1, true);
    $counts = array_count_values($a);
    echo $chunks[0][0], $preserved[0][0], $counts[1] ?? 0;
}
function concreteHashChunksTarget(bool $flag): void {
    $a = $flag ? ['key' => 1] : null;
    $a = $a ?? ['other' => 2];
    $chunks = array_chunk($a, 1, true);
    foreach ($chunks as $chunk) { foreach ($chunk as $key => $value) { echo $key, $value; } }
}
function concreteGuardedChunksTarget(bool $flag): void {
    $a = $flag ? [4, 5] : null;
    if ($a === null) { return; }
    $chunks = array_chunk($a, 1);
    echo $chunks[0][0];
    vprintf('%d:%d', $a);
}
function checkedReturnTarget(mixed $input): int { return $input; }
function recoverReturnTarget(?int $value, mixed $input): int {
    $value ??= 1;
    $ignored = 0;
    try { $ignored = checkedReturnTarget($input); }
    catch (TypeError $error) { $value = null; $value ??= 2; }
    echo $ignored;
    return $value;
}
function localReturnTarget(?int $value, mixed $input): int {
    try { return $input; }
    catch (TypeError $error) { $value ??= 2; return $value; }
}
function resetValue(?int &$value): void { $value = null; }
function fallback(?int $value): ?int {
    $value ??= 5;
    resetValue($value);
    return $value;
}
function consume(int $value): void {}
class GuardedBox { public int $value = 1; }
function replaceGuardedBox(GuardedBox &$box): void { $box = new GuardedBox(); }
function guardedBoxValue(?GuardedBox $box): int {
    if ($box === null) { return 0; }
    replaceGuardedBox($box);
    return $box->value;
}
function returnedGuardValue(?int $value): void {
    $value ??= 1;
    try { if ($value !== null) { $value = null; return; } }
    finally { var_dump($value); }
}
function thrownGuardValue(?int $value): void {
    $value ??= 1;
    try { if ($value !== null) { $value = null; throw new RuntimeException('failed'); } }
    finally { var_dump($value); }
}
function failedGuardValue(): never { throw new RuntimeException('failed'); }
function caughtGuardValue(?int $value): void {
    $value ??= 4;
    try { if ($value !== null) { $value = null; FAILEDGUARDVALUE(); } }
    catch (RuntimeException $error) { $value ??= 2; consume($value); }
    finally { consume($value); }
}
class StatementLabelFlow { public function __toString(): string { throw new RuntimeException('label'); } }
function statementGuardValue(?int $value): int {
    $value ??= 5;
    try { $value = null; echo new StatementLabelFlow(); }
    catch (Throwable $error) { $value ??= 7; return $value; }
    return 0;
}
function switchEntryValue(?int $value, int $key): ?int {
    switch ($key) { case 1: $value = null; case 2: return $value; default: return 4; }
}
function failInitializerValue(): int { throw new RuntimeException('init'); }
function referenceInitializerValue(?int $value): ?int {
    $value ??= 4;
    $array = [1, 2];
    try { $reference = &$array[failInitializerValue()]; }
    catch (RuntimeException $error) { $value = null; }
    return $value;
}
function typedInitializerValue(?int $value): ?int {
    $value ??= 4;
    try { int $local = failInitializerValue(); }
    catch (RuntimeException $error) { $value = null; }
    return $value;
}
class EagerTargetFlow { public function __construct(mixed $arg = null) {} public function take(int $value): void {} }
function eagerTargetValue(?int $value, string $className): ?int {
    $value ??= 4;
    if (($value = null) instanceof EagerTargetFlow) {}
    new $className($value = null);
    $kept = array_filter([1, 0], $value = null);
    return $value;
}
function nullsafeTargetValue(?int $value, ?EagerTargetFlow $receiver): ?int {
    $receiver?->take($value ??= 4);
    return $value;
}
function matchTargetValue(?int $value, int $key): ?int {
    $value ??= 4;
    return match ($key) { ($value = null) => 0, default => $value };
}
function resetTargetDefault(?int &$value): bool { $value = null; return true; }
function continuingFinallyTarget(?int $value, bool $closed): int {
    try { if ($closed || $value === null) { return 0; } }
    finally { echo 'done'; }
    return $value;
}
function storedFinallyTarget(?int $value, ?int $other): int {
    try { if ($value === null) { return 0; } if ($other === null) { return 0; } }
    finally { $other = 7; $value = $other; }
    return $value;
}
function referenceFinallyTarget(?int &$value): int {
    try { if ($value === null) { return 0; } }
    finally { echo 'done'; }
    return $value;
}
function refinedFinallyTarget(?int $value): int {
    try { echo 'try'; }
    finally { if ($value === null) { return 0; } }
    return $value;
}
function coalescedTargetDefault(?int $value, ?bool $other): ?int {
    $value ??= 1;
    $other ??= resetTargetDefault($value);
    return $value;
}
function resetMixedTargetDefault(mixed &$value): bool { $value = null; return true; }
function mixedTargetDefault(?int $value, ?bool $other): int {
    $value ??= 1;
    $other ??= resetMixedTargetDefault($value);
    return $value;
}
function onceValue(?int $value): int {
    do { if ($value === null) { return 0; } } while (false);
    return $value;
}
function trueLoopValue(?int $value): int {
    while (true) { if ($value === null) { return 0; } break; }
    return $value;
}
function pendingFinallyValue(?int $value, bool $flag): ?int {
    $value ??= 1;
    foreach ([1] as $i) {
        try { if ($flag) { $value = null; break; } }
        finally { var_dump($value); }
    }
    return $value;
}
function conditionalIntLoop(?int $value, bool $flag): ?int {
    foreach ([1, 2] as $i) { $value = $flag ? null : 1; }
    resetValue($value);
    return $value;
}
function defaultFirst(?int $value, int $key): ?int {
    $value ??= 5;
    switch ($key) { default: $value = null; case 1: return $value; }
    return $value;
}
function nullableBump(?int &$value): void { $value = ($value ?? 0) + 1; }
function continuedTail(?int $value): ?int {
    for ($i = 0; $i < 3; $previous = $i, $i++, nullableBump($value)) {
        if ($value !== null) { continue; }
        $value = null;
        break;
    }
    return $value;
}
function switchedValue(?int $value, int $key): ?int {
    $value ??= 3;
    switch ($key) { case 1: if ($key > 0) { $value = null; break; } break; }
    return $value;
}
function continuedLoop(?int $value): int {
    foreach ([1, 2] as $i) {
        if ($value === null) { $value = 3; continue; }
        return $value;
    }
    return 0;
}
function brokenLoop(?int $value): ?int {
    $value ??= 1;
    foreach ([1] as $i) {
        if (is_int($value)) { $value = null; break; }
    }
    return $value;
}
function expressionFallback(?int $value): ?int {
    consume($value ??= 5);
    resetValue($value);
    return $value;
}
function guardedFallback(?int $value): ?int {
    if ($value === null) { return null; }
    $value = 5;
    resetValue($value);
    return $value;
}
function convertedFallback(?int $value): mixed {
    $value ??= 5;
    settype($value, 'string');
    return $value;
}
function incrementedFallback(?int $value): mixed {
    $value ??= 5;
    ++$value;
    return $value;
}
function conditionalFallback(?int $value, bool $flag): ?int {
    $value ??= 5;
    $flag && resetValue($value);
    return $value;
}
function dynamicFallback(?int $value, mixed $default): mixed {
    $value ??= $default;
    return $value;
}
function widenValue(mixed &$value): bool { $value = 'str'; return true; }
function conditionalWidenFallback(?int $value, bool $flag): mixed {
    $value ??= 5;
    $flag && widenValue($value);
    return $value;
}
function freshGuardIncrement(?int $value): mixed {
    if ($value !== null) { $value++; }
    widenValue($value);
    return $value;
}
function loopGuardIncrement(?int $value): mixed {
    foreach ([1, 2] as $i) {
        widenValue($value);
        $value = $i;
        if ($value !== null) { $value++; }
    }
    return $value;
}
function incomingGuardIncrement(?int &$value): void {
    if ($value !== null) { $value++; }
    resetValue($value);
}
function incomingLoopIncrement(?int &$value): void {
    foreach ([1] as $i) { resetValue($value); if ($value !== null) { $value++; } }
}
function capturedLoopIncrement(?int $value): void {
    $show = function () use (&$value) { var_dump($value); };
    foreach ([1, 2] as $i) {
        resetValue($value);
        $value = $i;
        if ($value !== null) { $value++; }
        $show();
    }
}
class A extends Exception {}
class B extends Exception {}
function throwAlternative(Throwable $error): void {
    if ($error instanceof A || $error instanceof B) { throw $error; }
}
echo fallback(null);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source,
            std::path::Path::new("main.php"), std::path::Path::new("."), target);
        for function_name in ["fallback", "expressionFallback", "guardedFallback", "conditionalFallback"] {
            let function = module.functions.iter().find(|function| function.name == function_name).unwrap();
            let slot = function.locals.iter().find(|local| local.name.as_deref() == Some("value")).unwrap().id;
            let loads: Vec<_> = function.instructions.iter().filter(|inst|
                inst.op == Op::LoadLocal && inst.immediate == Some(Immediate::LocalSlot(slot))).collect();
            assert!(!loads.is_empty(), "{name}");
            assert!(loads.iter().all(|inst| inst.result_type == IrType::TaggedScalar),
                "{name}: {}", crate::ir::print_function(function));
        }
        for function_name in ["convertedFallback", "incrementedFallback", "dynamicFallback", "conditionalWidenFallback", "freshGuardIncrement", "loopGuardIncrement"] {
            let widened = module.functions.iter().find(|function| function.name == function_name).unwrap();
            assert!(widened.instructions.iter().any(|inst|
                inst.result_type == IrType::Heap(IrHeapKind::Mixed)), "{name}: {function_name}");
        }
        for function_name in ["incomingGuardIncrement", "incomingLoopIncrement", "capturedLoopIncrement"] {
            let incoming = module.functions.iter().find(|function|
                function.name == function_name).unwrap();
            assert!(incoming.instructions.iter().any(|inst| inst.op == Op::ICheckedAddToInt), "{name}: {function_name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Unsupported string-slot chunks produce a source refusal on every target.
#[test]
fn test_nullable_fallback_flow_string_chunks_diagnostic_all_targets() {
    use std::path::Path;
    use crate::codegen::platform::Target;
    use crate::ir_lower::LoweringError;
    let bodies = [
        "$a = ['x', 'y'];",
        "$a = $flag ? ['x', 'y'] : null; $a ??= ['a', 'b'];",
        "$a = $flag ? ['x', 'y'] : null; $a = $a ?? ['a', 'b'];",
        "$a = $flag ? ['x', 'y'] : null; if ($a === null) { return; }",
    ];
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for body in bodies {
            let source = format!("<?php function stringChunks(bool $flag): void {{ {body} $chunks = array_chunk($a, 1); echo $chunks[0][0]; }} stringChunks($argc > 0);");
            let error = match super::try_lower_source_at_for_target(&source, Path::new("main.php"),
                Path::new("."), Target::parse(name).unwrap())
            {
                Err(LoweringError::Unsupported(error)) => error,
                Err(other) => panic!("{name}: expected source refusal, got {other:?}"),
                Ok(_) => panic!("{name}: string chunks must be refused"),
            };
            assert!(error.message.contains("indexed string arrays is not supported"), "{name}: {}", error.message);
            assert!(error.span.line > 0 && error.span.col > 0);
        }
    }
}
