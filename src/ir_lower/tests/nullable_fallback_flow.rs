//! Purpose:
//! Verifies nullable local storage survives coalescing and reference calls on every target.
//!
//! Called from:
//! - The AST-to-EIR library test harness.
//!
//! Key details:
//! - A scalar flow refinement must preserve the parameter's tag and payload representation.

use crate::ir::{Immediate, IrHeapKind, IrType, Op};

/// Every supported target emits loop-first catch replacements and their outer reads.
#[test]
fn test_nullable_fallback_flow_r49_loop_catch_all_targets() {
    let source = r#"<?php
class LoopCatchException extends Exception {
    public function __destruct() {
        if ($this->getMessage() === 'a0') { throw new Exception('d'); }
    }
}
function needLoopCatch(LoopCatchException $value): void { echo $value->getMessage(); }
function showTopLoopCatch(): void {
    global $top;
    if ($top instanceof LoopCatchException) { needLoopCatch($top); }
}
function firstLoopCatch(): void {
    for ($i = 0; $i < 2; $i++) {
        try {
            try { throw new LoopCatchException('a' . $i); } catch (LoopCatchException $e) {}
        } catch (Exception $c) { needLoopCatch($e); }
    }
}
firstLoopCatch();
foreach ([0, 1] as $i) {
    try {
        try { throw new LoopCatchException('a' . $i); } catch (LoopCatchException $top) {}
    } catch (Exception $c) { showTopLoopCatch(); }
}
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Implicit retirement edges and bounded recursive facts check on every supported target.
#[test]
fn test_nullable_fallback_flow_r48_implicit_retirement_all_targets() {
    let negative = "<?php class R48D { public function __destruct() { throw new Exception('d'); } } function need(int $v): void {} function run(?int $v): void { $v ??= 1; $e = new R48D(); try { try { throw new Exception('a'); } catch (Exception $e) {} } catch (Exception $c) { $v = null; } need($v); }";
    let recursive = "<?php function run(array $xs): void { $a = [1]; foreach ($xs as $x) { try {} catch (Exception $other) {} $a = [$a]; } var_dump(count($a)); } run([1, 2, 3]);";
    let positive = "<?php class R48D { public function __destruct() { throw new Exception('d'); } } function run(): void { $e = new R48D(); try { try { throw new Exception('a'); } catch (Exception $e) {} } catch (Exception $c) { echo $c->getMessage(); } } run();";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let program = crate::parser::parse(&crate::lexer::tokenize(negative).unwrap()).unwrap();
        let error = crate::types::check_with_target(&program, target).err().expect("implicit retirement");
        assert!(error.to_string().contains("expects int"), "{name}: {error}");
        let program = crate::parser::parse(&crate::lexer::tokenize(recursive).unwrap()).unwrap();
        crate::types::check_with_target(&program, target).unwrap();
        let module = super::lower_source_at_for_target(positive, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Every target refuses a stale catch projection and emits the valid terminal-catch paths.
#[test]
fn test_nullable_fallback_flow_r47_catch_backedges_all_targets() {
    let negative = "<?php function need(int $e): void {} function run(?int $e, array $xs): void { $e ??= 1; foreach ($xs as $x) { need($e); try { throw new Exception('x'); } catch (Exception $e) {} } }";
    let tokens = crate::lexer::tokenize(negative).unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let positive = "<?php function need(int $e): void { var_dump($e); } function run(?int $e): void { $e ??= 1; foreach ([1, 2] as $x) { need($e); try { throw new Exception('x'); } catch (Exception $e) { break; } } } run(null);";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let error = crate::types::check_with_target(&program, target).err().expect("stale catch proof");
        assert!(error.to_string().contains("expects int"), "{name}: {error}");
        let scalar = "<?php class ColdLoopDestructor { public function __destruct() { throw new Exception('x'); } } function need(int $e): void {} function run(?int $e): void { if ($e === null) return; foreach ([1, 2] as $x) { need($e); try { $safe = 1; $e = 1; ($safe = 1); unset($safe); } catch (Exception $e) {} } } run(1);";
        let scalar_module = super::lower_source_at_for_target(scalar, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        crate::codegen::generate_user_asm_from_ir(&scalar_module, false, false).unwrap();
        let module = super::lower_source_at_for_target(positive, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Nullable array mutation chains lower and emit through every supported target ABI.
#[test]
fn test_nullable_fallback_flow_r46_guarded_arrays_all_targets() {
    let source = "<?php function sortedArray(?array $a): array { if ($a === null) return []; sort($a); rsort($a); return $a; } function pushedArray(?array $a): array { if ($a !== null) { array_push($a, 9); array_pop($a); array_unshift($a, 8); array_shift($a); return $a; } return []; } echo implode(',', sortedArray([3, 1, 2])), implode(',', pushedArray([1, 2]));";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        for function_name in ["sortedArray", "pushedArray"] {
            let function = module.functions.iter().find(|function| function.name == function_name).unwrap();
            assert!(function.return_php_type.is_php_array(), "{name}: {function_name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Array mutation chains and structural guarded stores lower on all supported targets.
#[test]
fn test_nullable_fallback_flow_r45_structural_all_targets() {
    let source = "<?php class C { const K = 7; public static int $s = 8; public int $n = 9; } function structural(?int $v, bool $flag, ?int $w, C $c): int { if ($v === null) { return 0; } $v = $flag ? C::K : $c->n; $v = $w ?? C::$s; return $v; } $a = [3, 1, 2]; sort($a); rsort($a); echo implode(',', $a); var_dump(structural(5, false, null, new C()));";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// An unsupported nested native array sort retains its compile-time diagnostic on every target.
#[test]
fn test_nullable_fallback_flow_r45_nested_sort_refusal() {
    use crate::codegen::platform::Target;
    use crate::ir_lower::LoweringError;
    use std::path::Path;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let result = super::try_lower_source_at_for_target("<?php $a = [[2, 'b'], [1, 'a']]; sort($a);",
            Path::new("main.php"), Path::new("."), Target::parse(name).unwrap());
        let error = match result {
            Err(LoweringError::Unsupported(error)) => error.message,
            Err(other) => panic!("{name}: expected source refusal, got {other:?}"),
            Ok(module) => {
                let error = crate::codegen::generate_user_asm_from_ir(&module, false, false)
                    .err().unwrap_or_else(|| panic!("{name}: nested array sort must be refused before execution"));
                format!("{error:?}")
            }
        };
        assert!(error.contains("sort indexed-array element PHP type"), "{name}: {error}");
    }
}

/// Unproved nullable backedge stores retain float-capable loop returns on every target.
#[test]
fn test_nullable_fallback_flow_r44_backedge_all_targets() {
    let source = "<?php function backedge(?int $value, ?int $other, int $key) { $ints = [10]; $other ??= 3; $result = 1; if ($value !== null) { foreach ([1, 2] as $item) { if ($value !== null) { $other = $ints[$key]; $value = $other; continue; } $result = 2.5; break; } } return $result; } function showBackedge(?int $value, int $key): void { $ints = [10]; if ($value !== null) { foreach ([1, 2] as $item) { var_dump($value); $value = $ints[$key]; } } } var_dump(backedge(5, 1, 4)); showBackedge(5, 4);";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        let function = module.functions.iter().find(|function| function.name == "backedge").unwrap();
        let result = &function.return_php_type;
        assert!(*result == crate::types::PhpType::Mixed || matches!(result, crate::types::PhpType::Union(members)
            if members.contains(&crate::types::PhpType::Float)), "{name}: {result:?}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Assignment guards retain reachable float returns before emission on every target.
#[test]
fn test_nullable_fallback_flow_r43_entry_guard_all_targets() {
    let source = "<?php function entryGuard(int $key) { $ints = [10]; $result = 1; foreach ([1, 2] as $item) { $value = 7; if (($value = $ints[$key]) !== null) { continue; } $result = 2.5; break; } return $result; } function staleNull(int $key) { $ints = [10]; $result = 1; foreach ([1, 2] as $item) { $value = null; if (($value = $ints[$key]) === null) { continue; } $result = 2.5; break; } return $result; } var_dump(entryGuard(5), staleNull(0));";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        for function_name in ["entryGuard", "staleNull"] {
            let function = module.functions.iter().find(|function| function.name == function_name).unwrap();
            let result = &function.return_php_type;
            assert!(*result == crate::types::PhpType::Mixed || matches!(result, crate::types::PhpType::Union(members)
                if members.contains(&crate::types::PhpType::Float)), "{name}: {function_name}: {result:?}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Condition-prefix value proofs retain reachable float returns on every supported target.
#[test]
fn test_nullable_fallback_flow_r42_condition_prefix_all_targets() {
    let source = "<?php function conditionPrefix(int $value, int $key) { $ints = [10]; $result = 1; if (($value = $ints[$key]) > 100) { $result = 0; } else { foreach ([1, 2] as $item) { if ($value !== null) { continue; } $result = 2.5; break; } } return $result; } var_dump(conditionPrefix(7, 5));";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        let function = module.functions.iter().find(|function| function.name == "conditionPrefix").unwrap();
        let result = &function.return_php_type;
        assert!(*result == crate::types::PhpType::Mixed || matches!(result, crate::types::PhpType::Union(members)
            if members.contains(&crate::types::PhpType::Float)), "{name}: {result:?}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Switch label prefix views retain float-capable returns on every supported target.
#[test]
fn test_nullable_fallback_flow_r41_switch_prefix_all_targets() {
    let source = "<?php function switchPrefix(int $subject, int $key) { $ints = [10]; $result = 1; $value = $ints[$key]; switch ($subject) { case 1: foreach ([1, 2] as $item) { if ($value !== null) { continue; } $result = 2.5; break; } break; case ($value = 7): break; } return $result; } var_dump(switchPrefix(1, 5));";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        let function = module.functions.iter().find(|function| function.name == "switchPrefix").unwrap();
        let result = &function.return_php_type;
        assert!(*result == crate::types::PhpType::Mixed || matches!(result, crate::types::PhpType::Union(members)
            if members.contains(&crate::types::PhpType::Float)), "{name}: {result:?}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Synthetic OPcache foreach results retain a return shape capable of carrying hash keys.
#[test]
fn test_nullable_fallback_flow_r37_synthetic_hash_return() {
    let source = "<?php function typedHash(array<string> $keys): array<string,int> { $all = []; foreach ($keys as $key) { $all[$key] = 1; } return $all; } echo count(typedHash([])), count(typedHash(['key'])); $settings = ini_get_all('zend opcache'); echo count($settings);";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), target);
        for (function_name, expected) in [
            ("__elephc_opcache_ini_all_details", IrType::Heap(IrHeapKind::Hash)),
            ("__elephc_opcache_ini_all_plain", IrType::Heap(IrHeapKind::Mixed)),
            ("typedHash", IrType::Heap(IrHeapKind::Hash)),
        ] {
            let function = module.functions.iter().find(|function| function.name == function_name).unwrap();
            assert_eq!(function.return_type, expected, "{name}: {function_name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Coalescing assignments retain nullable parameter loads and emit valid calls on all targets.
#[test]
fn test_nullable_fallback_flow_storage_all_targets() {
    let source = r#"<?php
function maybeBoundLocal(?int $parameter, bool $choose): void {
    if ($choose) { $value = $parameter; $value ??= 5; }
    var_dump($value); $show = fn() => var_dump($value); $show();
}
function unprovedAliasCapture(?int $value): Closure {
    $alias = &$value;
    if ($value !== null) { $alias = null; return fn() => var_dump($value); }
    return fn() => var_dump(null);
}
function conditionalProvenCapture(?int $value, bool $choose): Closure {
    if ($choose) {
        $value ??= 40;
        $items = []; $items[] = $value; echo json_encode($items);
        return fn(): int => $value + 2;
    }
    return fn(): int => 2;
}
function nullableIntegerCapture(?int $value): Closure { return fn() => var_dump($value); }
function nullableBooleanCapture(?bool $value): Closure { return fn() => var_dump($value); }
function nullableFloatCapture(?float $value): Closure { return fn() => var_dump($value); }
class ProjectedValueHolder {
    public static int $value = 0;
    function captured(?int $value): Closure { $value ??= 40; return fn(): int => $value + 2; }
}
function projectedValueConsumer(?int $value): Closure {
    $value ??= 40;
    $items = []; $items[] = $value;
    $pairs = [$value, 'key' => $value];
    echo count($items), count($pairs), json_encode([$value]);
    ProjectedValueHolder::$value = $value;
    $mapped = array_map(fn($item) => $item + $value, [1, 2]);
    echo implode(' ', $mapped);
    return fn(): int => $value + 2;
}
function projectedGuardConsumer(?int $value): Closure {
    if ($value === null) { $value = 40; }
    return fn(): int => $value + 2;
}
class RelativeGuardRoot {}
class RelativeGuardNode extends RelativeGuardRoot {
    static function accept(self $other): int { return 1; }
    static function ancestor(parent $other): int { return 1; }
    function instance(self $other): int { return 1; }
}
function relativeGuardContinuation(?RelativeGuardNode $maybe, ?int $value): mixed {
    $value ??= 7;
    try { $value = null; RelativeGuardNode::accept($maybe); return 1; }
    catch (TypeError $error) { var_dump($value); }
    return $value;
}
function relativeParentContinuation(?RelativeGuardRoot $maybe, ?int $value): mixed {
    $value ??= 7;
    try { $value = null; RelativeGuardNode::ancestor($maybe); return 1; }
    catch (TypeError $error) { var_dump($value); }
    return $value;
}
function relativeInstanceContinuation(?RelativeGuardNode $maybe, ?int $value): mixed {
    $value ??= 7;
    try { $value = null; (new RelativeGuardNode())->instance($maybe); return 1; }
    catch (TypeError $error) { var_dump($value); }
    return $value;
}
function aliasedIntegerRead(?int $value): void {
    $alias = &$value;
    if ($value !== null) { $alias = null; var_dump($value); var_dump([$value]); }
}
function incomingAliasedIntegerRead(?int &$value, ?int &$alias): void {
    if ($value !== null) { $alias = null; var_dump([$value]); }
}
function builtinErrorCatchRead(mixed $value): int {
    $local = $value === 0 ? 1 : null;
    if ($local === null) { $local = 7; }
    try { $local = null; return $value; } catch (Error $error) { var_dump($local); }
    var_dump($local); return 0;
}
function builtinThrowableCatchRead(mixed $value): int {
    $local = $value === 0 ? 1 : null;
    if ($local === null) { $local = 7; }
    try { $local = null; return $value; } catch (Throwable $error) { var_dump($local); }
    var_dump($local); return 0;
}
function projectedGeneratorCapture(?int $value): Generator {
    $value ??= 40;
    yield fn(): int => $value + 1;
}
function nullableScalarAlternative(mixed $value): ?string {
    if ($value === null || is_int($value)) { return nullableScalarTarget($value); }
    return null;
}
function nullableScalarTarget(?string $value): ?string { return $value; }
class ScalarBoundaryTarget { public string $text = ''; public static string $shared = ''; }
function scalarStringTarget(string $value): string { return $value; }
function scalarIntegerTarget(int $value): int { return $value; }
function scalarBoundaryTarget(mixed $value): string {
    if (is_int($value) || is_string($value)) {
        $box = new ScalarBoundaryTarget();
        $box->text = $value;
        ScalarBoundaryTarget::$shared = $value;
        $copy = $value;
        $closure = function () use ($value): string { return scalarStringTarget($value); };
        echo scalarStringTarget($copy), $closure(), ScalarBoundaryTarget::$shared;
        echo scalarStringTarget(value: $value), $box->text;
        return ($value = $value);
    }
    if (is_float($value) || is_bool($value)) { return scalarStringTarget($value); }
    return 'other';
}
function integerBoundaryTarget(mixed $value): int {
    if (is_int($value) || is_bool($value)) { return scalarIntegerTarget($value); }
    return -1;
}
class LoopArgumentTarget {}
function needLoopArgumentTarget(LoopArgumentTarget $value): void {}
function loopArgumentGuardTarget(?LoopArgumentTarget $value, array $items): void {
    $value ??= new LoopArgumentTarget();
    foreach ($items as $item) { needLoopArgumentTarget($value); $value = null; }
}
class ConditionalUnsetTarget {}
function conditionalUnsetTarget(?ConditionalUnsetTarget $value, int $key) {
    $value ??= new ConditionalUnsetTarget();
    if ($key > 1) { unset($value); }
    return $value;
}
function guardedUnsetTarget(?ConditionalUnsetTarget $value, int $key) {
    if ($value === null) { throw new RuntimeException('missing'); }
    if ($key > 1) { unset($value); }
    return $value;
}
function fallbackAfterUnsetTarget(?ConditionalUnsetTarget $value, int $key, ?ConditionalUnsetTarget $default) {
    $value ??= new ConditionalUnsetTarget();
    if ($key > 1) { unset($value); }
    $value ??= $default;
    return $value;
}
class RecoalesceTarget { public function __construct(public readonly string $name = 'dflt') {} }
function repeatedCoalesceTarget(?RecoalesceTarget $value, ?RecoalesceTarget $default): string {
    $value = $value ?? new RecoalesceTarget();
    $value ??= $default;
    return $value->name;
}
function repeatedStringTarget(?string $value, ?string $default): string {
    $value ??= 'x';
    $value = $value ?? $default;
    return $value;
}
function switchAssignmentStorageTarget(int $key) {
    $value = 0;
    switch ($key) {
        case 1: $value = 2.5; break;
        case 2: $value = 'two'; break;
        default: $value = $key;
    }
    return $value;
}
packed class RawNullsafeRowTarget { public int $value; }
class RawPointerSinkTarget { public function take(?ptr<RawNullsafeRowTarget> $value): void {} }
function rawNullsafePointerTarget(?RawPointerSinkTarget $receiver, buffer<RawNullsafeRowTarget> $rows): ?ptr<RawNullsafeRowTarget> {
    $value = null;
    $receiver?->take($value = $rows[0]);
    return $value;
}
class RawNullsafeTarget { public function take(mixed $value): void {} }
function rawNullsafeTarget(?RawNullsafeTarget $receiver) {
    $value = null;
    $receiver?->take($value = 5);
    return $value;
}
function rawNullsafeFloatTarget(?RawNullsafeTarget $receiver) {
    $value = 1;
    $receiver?->take($value = 2.5);
    return $value;
}
function rawLoopExitTarget(int $key) {
    $value = 1;
    foreach ([1] as $_) {
        try {
            switch ($key) { case 1: [$value] = [2.5]; break 2; default: break; }
        } catch (Exception $error) { $value = 3.5; continue; }
    }
    return $value;
}
function rawSwitchExitTarget(int $key) {
    $value = 1;
    switch ($key) { case 1: $value = 2.5; break; default: $value = 3; }
    return $value;
}
function nullableAccumulatorTarget(?int $value): int {
    $value ??= 0;
    for ($i = 0; $i < 3; $i++) { $value = $value + $i; }
    return $value;
}
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
interface CIEmitSink { public function write(): void; }
interface CIEmitWatch { public function touch(): void; }
function ciInterfaceJoinTarget(?CIEmitSink $handler, bool $lazy): void {
    if ($handler !== null) {
        if ($lazy) { if ($handler instanceof CIEmitWatch) { $handler->touch(); } }
        $handler->write();
    }
}
function ciOptionJoinTarget(array<string,string|int|bool> $options): mixed {
    $flag = false;
    foreach ($options as $key => $value) { if ($key === 'read_and_close') { $flag = $value; } }
    return $flag;
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
        for function_name in ["projectedValueConsumer", "projectedGuardConsumer", "conditionalProvenCapture"] {
            let function = module.functions.iter().find(|function| function.name == function_name).unwrap();
            let slot = function.locals.iter().find(|local| local.name.as_deref() == Some("value")).unwrap();
            assert_eq!(slot.ir_type, IrType::TaggedScalar, "{name}: {function_name}");
            let captures: Vec<_> = function.instructions.iter().filter(|inst| inst.op == Op::ClosureCapture).collect();
            assert!(!captures.is_empty(), "{name}: {function_name}");
            for capture in captures {
                assert_eq!(function.values[capture.operands[0].as_raw() as usize].ir_type, IrType::I64,
                    "{name}: {}", crate::ir::print_function(function));
            }
        }
        for (function_name, expected) in [("unprovedAliasCapture", IrType::Heap(IrHeapKind::Mixed)),
            ("nullableIntegerCapture", IrType::TaggedScalar),
            ("nullableBooleanCapture", IrType::Heap(crate::ir::IrHeapKind::Mixed)),
            ("nullableFloatCapture", IrType::Heap(crate::ir::IrHeapKind::Mixed))] {
            let function = module.functions.iter().find(|function| function.name == function_name).unwrap();
            let captures: Vec<_> = function.instructions.iter().filter(|inst| inst.op == Op::ClosureCapture).collect();
            assert!(!captures.is_empty(), "{name}: {function_name}");
            for capture in captures {
                assert_eq!(function.values[capture.operands[0].as_raw() as usize].ir_type, expected,
                    "{name}: {}", crate::ir::print_function(function));
            }
        }
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
