//! Purpose:
//! Executes frame-local checked arithmetic and conservative escape fallbacks.
//!
//! Called from:
//! - The focused optimizer codegen test suite.
//!
//! Key details:
//! - Both IR modes run with heap validation, actual PHP oracles and all target emitters.

use super::*;

/// Runtime-bound typed reductions exercise both a return check and a checked producer chain.
const TYPED_CHAIN: &str = r#"<?php
function addOnly(int $n): int {
    $sum=0; for($i=0;$i<$n;$i++){ $sum+=$i; } return $sum;
}
function mulAdd(int $n): int {
    $sum=0; for($i=0;$i<$n;$i++){ $sum=$sum+$i*3; } return $sum;
}
function subMul(int $n): int {
    $sum=0; for($i=0;$i<$n;$i++){ $sum=$sum-$i*3; } return $sum;
}
$n=$argc*1000; echo addOnly($n),'|',mulAdd($n),'|',subMul($n);
"#;

/// An unknown runtime width keeps the checked product in both local and property index chains.
const INDEX_CHAIN: &str = r#"<?php
function localIndex(int $n, int $columns): int {
    buffer<int> $cells=buffer_new<int>(1800);
    for($k=0;$k<$n;$k++){
        $row=$k%30; $column=$k%60; $cells[$row*$columns+$column]++;
    }
    $sum=0; for($k=0;$k<1800;$k++){ $sum+=$cells[$k]*($k+1); }
    buffer_free($cells); return $sum;
}
final class NumericCellGrid {
    public buffer<int> $cells;
    public int $columns;
    public function __construct(int $columns) {
        $this->columns=$columns; $this->cells=buffer_new<int>(1800);
    }
    public function indexCells(int $n): int {
        for($k=0;$k<$n;$k++){
            $row=$k%30; $column=$k%60;
            $this->cells[$row*$this->columns+$column]++;
        }
        $sum=0; for($k=0;$k<1800;$k++){ $sum+=$this->cells[$k]*($k+1); }
        $owned=$this->cells; buffer_free($owned); return $sum;
    }
}
$n=$argc*1000; echo localIndex($n,60),'|',(new NumericCellGrid(60))->indexCells($n);
"#;

/// Compiles and runs an explicit mode with heap validation and allocation counters.
fn run(source: &str, optimized: bool) -> (String, String) {
    run_with_oracle(source, source, optimized)
}

/// Lets an extension fixture supply a PHP array equivalent without changing its arithmetic.
fn run_with_oracle(source: &str, php_source: &str, optimized: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_stack_numeric");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compile = command.arg(&path).output().unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let result = run_binary(&path.with_extension(""), &dir);
    assert!(
        result.status.success(),
        "status {}: {}",
        result.status, String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(
        stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{stderr}"
    );
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let oracle = dir.join("oracle.php");
        fs::write(&oracle, php_source).unwrap();
        let php = Command::new("php")
            .arg("-n")
            .arg(&oracle)
            .output()
            .expect("PHP oracle");
        assert!(
            php.status.success(),
            "{}",
            String::from_utf8_lossy(&php.stderr)
        );
        assert_eq!(stdout.as_bytes(), php.stdout, "actual PHP oracle");
    }
    fs::remove_dir_all(dir).unwrap();
    (stdout, stderr)
}

/// Compares an unhandled return-boundary error's class, message and exit status with PHP.
fn return_boundary_failure(source: &str, optimized: bool) {
    let dir = make_cli_test_dir("elephc_numeric_return_failure");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized { command.arg("--no-ir-opt"); }
    let compiled = command.arg(&path).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    let message = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.status.code(), Some(255), "{message}");
    assert!(message.contains("TypeError"), "{message}");
    assert!(message.contains("Return value must be of type int, float returned"), "{message}");
    assert!(!message.contains("missed"), "the return check must precede observation");
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        assert_eq!(php.status.code(), output.status.code());
        let message = format!("{}{}", String::from_utf8_lossy(&php.stdout), String::from_utf8_lossy(&php.stderr));
        assert!(message.contains("TypeError"), "{message}");
        assert!(message.contains("Return value must be of type int, float returned"), "{message}");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// An accumulator has constant heap allocations rather than one numeric cell per iteration.
#[test]
fn test_stack_numeric_reduction_is_heap_clean_and_allocation_bounded() {
    let source = include_str!("../../../examples/numeric-local/main.php");
    let plain = run(source, false);
    let optimized = run(source, true);
    assert_eq!(plain.0, optimized.0);
    assert_eq!(optimized.0, "499500");
    let (plain_allocs, _) = parse_gc_stats(&plain.1);
    let (optimized_allocs, optimized_frees) = parse_gc_stats(&optimized.1);
    assert_eq!(
        optimized_allocs, optimized_frees,
        "every numeric owner is retired"
    );
    let small = run(&source.replace("* 1000", "* 10"), true);
    assert_eq!(small.0, "45");
    let (small_allocs, _) = parse_gc_stats(&small.1);
    assert!(optimized_allocs.abs_diff(small_allocs) <= 8,
        "frame numeric allocation count must stay bounded: small={small_allocs}, large={optimized_allocs}");
    assert!(
        optimized_allocs + 900 < plain_allocs,
        "numeric allocations: optimized={optimized_allocs}, plain={plain_allocs}"
    );
}

/// A numeric accumulator observed only through its final string form keeps its frame cell.
///
/// `echo $sum` casts the boxed slot to string at the loop exit. That read must not disqualify
/// the producer's frame cell, or the loop falls back to the allocating runtime numeric helper.
#[test]
fn test_stack_numeric_string_observation_keeps_allocation_bounded() {
    let source = r#"<?php
function local_sum_text(int $n): void {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i;
    }
    echo $sum . "|";
}
local_sum_text($argc * 1000);
"#;
    let large = run(source, true);
    assert_eq!(large.0, "499500|");
    let (large_allocs, large_frees) = parse_gc_stats(&large.1);
    assert_eq!(large_allocs, large_frees, "every numeric owner is retired");
    let small = run(&source.replace("* 1000", "* 10"), true);
    assert_eq!(small.0, "45|");
    let (small_allocs, _) = parse_gc_stats(&small.1);
    assert!(
        large_allocs.abs_diff(small_allocs) <= 8,
        "the loop must not allocate per iteration: small={small_allocs}, large={large_allocs}"
    );
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "499500|");
    }
}

/// A scalarized counter feeds a boxed accumulator without allocating a rhs Mixed box.
#[test]
fn test_stack_numeric_scalar_rhs_avoids_boxing_in_boxed_accumulator() {
    let source = r#"<?php
function sum_echo(int $n): void {
    $sum = 0;
    $i = 0;
    while ($i < $n) {
        $sum += $i;
        $i += 1;
    }
    echo $sum, "|";
}
sum_echo($argc + 1000);
"#;
    let dir = make_cli_test_dir("elephc_stack_numeric_scalar_rhs");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--emit-asm"]);
    let output = command.arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(path.with_extension("s")).unwrap();
    let function = asm
        .split("@fn name=sum_echo")
        .nth(1)
        .expect("sum_echo assembly")
        .split("@fn ")
        .next()
        .unwrap();
    let body = function
        .split("@block name=while.body")
        .nth(1)
        .expect("loop body")
        .split("@block name=while.exit")
        .next()
        .unwrap();
    assert!(body.contains("__rt_mixed_numeric_add"), "{body}");
    assert!(
        !body.contains("__rt_mixed_from_value"),
        "the scalar rhs must not be boxed:\n{body}"
    );
    assert_eq!(run(source, true).0, "500500|");
    fs::remove_dir_all(dir).unwrap();
}

/// A TaggedScalar array element narrows to null-as-zero int and keeps the accumulator cell.
#[test]
fn test_stack_numeric_tagged_scalar_rhs_uses_frame_cell() {
    let source = r#"<?php
function sum_tagged_rhs(int $n): void {
    $data = [];
    $i = 0;
    while ($i < $n) {
        $data[] = $i % 97;
        $i += 1;
    }
    $sum = 0;
    $i = 0;
    while ($i < count($data)) {
        $sum += $data[$i];
        $i += 1;
    }
    echo $sum . "|" . $i;
}
sum_tagged_rhs($argc + 9);
"#;
    let dir = make_cli_test_dir("elephc_stack_numeric_tagged_rhs");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--emit-asm"]);
    let output = command.arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(path.with_extension("s")).unwrap();
    let function = asm
        .split("@fn name=sum_tagged_rhs")
        .nth(1)
        .expect("sum_tagged_rhs assembly")
        .split("@fn ")
        .next()
        .unwrap();
    let body = function
        .split("@block name=while.body")
        .find(|body| body.contains("op=array_get"))
        .expect("array sum loop body");
    assert!(body.contains("op=cast"), "the element must narrow to int:\n{body}");
    assert!(
        body.contains("stack_numeric_integer"),
        "the accumulator must keep its frame cell:\n{body}"
    );
    assert!(
        !body.contains("__rt_mixed_from_value") && !body.contains("__rt_mixed_numeric_add"),
        "the narrowed element must stay out of the boxed numeric helper:\n{body}"
    );
    assert!(
        body.contains("numeric_local_cleanup_done"),
        "the numeric slot retirement must recognize its frame cell:\n{body}"
    );
    let condition = function
        .split("@block name=while.cond")
        .find(|block| block.contains("op=runtime_call"))
        .expect("count condition");
    let count_at = condition.find("op=runtime_call").expect("count lowering");
    let window_start = condition[..count_at].rfind("@src").unwrap_or(0);
    assert!(
        !condition[window_start..count_at].contains("_php_diagnostic_line"),
        "a typed-array count must not publish a diagnostic site:\n{condition}"
    );
    assert_eq!(run(source, true).0, "45|10");
    fs::remove_dir_all(dir).unwrap();
}

/// A missing int element is null-as-zero, and a tagged scalar sum still promotes to float.
#[test]
fn test_stack_numeric_tagged_scalar_null_and_overflow_match_php() {
    let source = r#"<?php
function sum_sparse(int $n): void {
    $data = [1, 2];
    $data[4] = 3;
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += @$data[$i];
    }
    echo gettype($sum), ':', @(int)$sum, '|';
}
function sum_overflow(int $n): void {
    $data = [PHP_INT_MAX, 1];
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $data[$i];
    }
    echo gettype($sum), ':', @(int)$sum, '|';
}
sum_sparse($argc + 4);
sum_overflow($argc + 1);
"#;
    let expected = "integer:6|double:-9223372036854775808|";
    assert_eq!(run(source, false).0, expected);
    assert_eq!(run(source, true).0, expected);
}

/// Escaping boxes, aliases, captures and a delayed snapshot keep independently owned values.
#[test]
fn test_stack_numeric_escaping_values_preserve_php_snapshots() {
    let source = r#"<?php
function snapshots(int $n): void {
    $sum = 0; $items = [];
    for ($i = 0; $i < $n; $i++) { $sum += $i; $items[] = $sum; }
    echo $items[2], ':', $items[4], '|';
}
function reference_snapshot(int $n): void {
    $value = 0; $ref =& $value;
    for ($i = 0; $i < $n; $i++) { $value += $i; }
    echo $ref, '|';
}
function capture_snapshot(int $n): void {
    $s = 0; $seen = function() use (&$s) { return $s; };
    for ($i = 0; $i < $n; $i++) { $s += $i; }
    echo $seen(), '|';
}
function value_snapshot(int $n): void {
    $v = 0; $copy = 0;
    for ($i = 0; $i < $n; $i++) { $v += $i; if ($i == 2) { $copy = $v; } }
    echo (int)$copy, ':', (int)$v, '|';
}
function scope_snapshot(int $n): void {
    $g = 0;
    for ($i = 0; $i < $n; $i++) { $g += $i; }
    $vars = get_defined_vars();
    for ($i = 0; $i < $n; $i++) { $g += 1; }
    echo (int)$vars["g"], ':', (int)$g, '|';
}
snapshots($argc + 4);
reference_snapshot($argc + 4);
capture_snapshot($argc + 4);
value_snapshot($argc + 4);
scope_snapshot($argc + 4);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "3:10|10|10|3:10|10:15|");
    }
}

/// Overflow branches use the original operands for all three arithmetic operations.
#[test]
fn test_stack_numeric_checked_overflow_matches_php_in_both_modes() {
    let source = r#"<?php
function casts(int $n, int $seed, int $step): void {
    $a = 0; $b = 0; $c = 0;
    for ($i = 0; $i < $n; $i++) {
        $a = $seed + $step; $b = $seed - $step; $c = $seed * $step;
    }
    echo @(int)$a, ':', @(int)$b, ':', @(int)$c, '|';
}
casts($argc + 1, PHP_INT_MAX, 2);
casts($argc + 1, PHP_INT_MIN, 2);
casts($argc + 1, PHP_INT_MIN, -1);
"#;
    let expected = "-9223372036854775808:9223372036854775805:0|-9223372036854775806:-9223372036854775808:0|-9223372036854775808:-9223372036854775807:-9223372036854775808|";
    assert_eq!(run(source, false).0, expected);
    assert_eq!(run(source, true).0, expected);
}

/// Every target emits checked integer and float paths while rejected escapes stay heap-backed.
#[test]
fn test_stack_numeric_checked_emission_all_supported_targets() {
    let source = format!(
        "{}\n{}",
        include_str!("../../../examples/numeric-local/main.php"),
        r#"
#[Export]
function numeric_triple(int $n, int $seed, int $step): void {
    $a = 0; $b = 0; $c = 0;
    for ($i = 0; $i < $n; $i++) {
        $a = $seed + $step; $b = $seed - $step; $c = $seed * $step;
    }
    echo (int)$a, ':', (int)$b, ':', (int)$c;
}

"#
    );
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        for optimized in [false, true] {
            let dir = make_cli_test_dir("elephc_stack_numeric_target");
            let path = dir.join("main.php");
            fs::write(&path, &source).unwrap();
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") {
                command.args(["--emit", "staticlib"]);
            }
            if !optimized {
                command.arg("--no-ir-opt");
            }
            let output = command.arg(&path).output().unwrap();
            assert!(
                output.status.success(),
                "{target}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let asm = fs::read_to_string(path.with_extension("s")).unwrap();
            assert!(!asm.is_empty(), "{target}");
            if optimized {
                assert!(asm.contains("stack_numeric_overflow"), "{target}");
                assert!(asm.contains(if target == "linux-x86_64" {
                    "cvtsi2sd"
                } else {
                    "scvtf"
                }));
                assert!(asm.contains(if target == "linux-x86_64" {
                    "imul r11, r10"
                } else {
                    "smulh x9, x0, x10"
                }));
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

/// Normal and exceptional exits release heap seeds without treating stack cells as allocations.
#[test]
fn test_stack_numeric_throw_and_finally_keep_scalar_state() {
    let source = r#"<?php
function unwind_numeric(int $n): int {
    $sum = 0;
    try {
        for ($i = 0; $i < $n; $i++) { $sum += $i; }
        throw new Exception("done");
    } catch (Exception $e) {
        return (int)$sum;
    } finally { echo "finally|"; }
}
echo unwind_numeric($argc + 4);
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "finally|10");
    }
}

/// Typed return checks and adjacent producers must not force one heap allocation per update.
#[test]
fn test_stack_numeric_typed_reductions_and_chains_bound_allocations() {
    assert_eq!(run(TYPED_CHAIN, false).0, "499500|1498500|-1498500");
    let (large, large_stats) = run(TYPED_CHAIN, true);
    let (small, small_stats) = run(&TYPED_CHAIN.replace("*1000", "*10"), true);
    assert_eq!(large, "499500|1498500|-1498500");
    assert_eq!(small, "45|135|-135");
    let (allocs, frees) = parse_gc_stats(&large_stats);
    let (small_allocs, _) = parse_gc_stats(&small_stats);
    assert_eq!(allocs, frees, "numeric frame cells are never heap owners");
    assert!(allocs.abs_diff(small_allocs) <= 16,
        "typed reductions allocate once per frame, small={small_allocs}, large={allocs}");
}

/// Overflow stays floating point, while the declared integer boundary still raises TypeError.
#[test]
fn test_stack_numeric_chained_typed_overflow_keeps_php_boundary_checks() {
    let definitions = r#"<?php
function typedChain(int $n, int $seed, int $step): int {
    $sum=$seed;
    for($i=0;$i<$n;$i++){ $sum=$sum+$step*2; }
    return $sum;
}
function floatChain(int $n, int $seed, int $step): float {
    $sum=$seed;
    for($i=0;$i<$n;$i++){ $sum=$sum+$step*2; }
    return $sum;
}
"#;
    let source = format!("{definitions}\n$n=$argc+1;
echo typedChain($n,3,4),'|',typedChain($n,PHP_INT_MIN,-1),'|';
echo gettype(floatChain($n,PHP_INT_MAX,1)),'|',gettype(floatChain($n,0,PHP_INT_MAX));");
    for optimized in [false, true] {
        assert_eq!(run(&source, optimized).0, "19|-9223372036854775808|double|double");
        for (seed, step) in [("PHP_INT_MAX", "1"), ("PHP_INT_MIN", "-4096")] {
            return_boundary_failure(&format!("{definitions}\necho typedChain($argc+1,{seed},{step}); echo 'missed';"), optimized);
        }
    }
}

/// Every target uses its checked numeric frame path for products followed by typed reductions.
#[test]
fn test_stack_numeric_typed_chain_emits_on_all_supported_targets() {
    let source = format!("{}\n{}\n{}", TYPED_CHAIN,
        INDEX_CHAIN.strip_prefix("<?php").unwrap().replace("]++;", "]=$k;"),
        "function transientTarget(int $seed,int $step):float{return ($seed+$step)+($seed+$step);}\necho transientTarget($argc,2);");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for optimized in [false, true] {
            let dir = make_cli_test_dir("elephc_typed_numeric_chain_target");
            let path = dir.join("main.php");
            fs::write(&path, &source).unwrap();
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
            if !optimized { command.arg("--no-ir-opt"); }
            let output = command.arg(&path).output().unwrap();
            assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
            let assembly = fs::read_to_string(path.with_extension("s")).unwrap();
            if optimized {
                assert!(assembly.contains("stack_numeric_overflow"), "{target}");
                assert!(assembly.contains("ret_boundary_int_float_fail"), "{target}");
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

/// Index expressions stay on the frame while generic element updates retain their fallback.
#[test]
fn test_stack_numeric_buffer_updates_keep_php_and_fallback_ownership() {
    let oracle = INDEX_CHAIN.replace("public buffer<int>", "public array")
        .replace("buffer<int> $cells", "$cells")
        .replace("buffer_free(", "unset(")
        .replace("buffer_new<int>(1800)", "array_fill(0,1800,0)");
    for optimized in [false, true] {
        assert_eq!(run_with_oracle(INDEX_CHAIN, &oracle, optimized).0, "894100|894100");
    }
}

/// Plain scalar writes isolate index and reduction arithmetic from the element-update path.
#[test]
fn test_stack_numeric_runtime_width_index_chains_bound_allocations() {
    let source = INDEX_CHAIN.replace("]++;", "]=$k;");
    let oracle = source.replace("public buffer<int>", "public array")
        .replace("buffer<int> $cells", "$cells")
        .replace("buffer_free(", "unset(")
        .replace("buffer_new<int>(1800)", "array_fill(0,1800,0)");
    let expected = index_write_checksum(1000);
    let expected = format!("{expected}|{expected}");
    assert_eq!(run_with_oracle(&source, &oracle, false).0, expected);
    let (large, stats) = run_with_oracle(&source, &oracle, true);
    let (small, small_stats) = run_with_oracle(&source.replace("*1000", "*10"),
        &oracle.replace("*1000", "*10"), true);
    assert_eq!(large, expected);
    let expected_small = index_write_checksum(10);
    assert_eq!(small, format!("{expected_small}|{expected_small}"));
    let (allocs, frees) = parse_gc_stats(&stats);
    let (small_allocs, _) = parse_gc_stats(&small_stats);
    assert_eq!(allocs, frees);
    assert!(allocs.abs_diff(small_allocs) <= 16,
        "index chains allocate once per frame, small={small_allocs}, large={allocs}");
}

/// Computes a weighted distribution independently of the compiler's checked arithmetic graph.
fn index_write_checksum(n: usize) -> usize {
    let mut cells = vec![0; 1800];
    for k in 0..n { cells[(k % 30) * 60 + k % 60] = k; }
    cells.iter().enumerate().map(|(index, value)| (index + 1) * value).sum()
}

/// A throwing property read unwinds a published numeric index before the parent catches the error.
#[test]
fn test_stack_numeric_scoped_index_unwinds_on_property_failure() {
    let source = r#"<?php
final class MissingNumericCells {
    public buffer<int> $cells;
    public int $columns=60;
    public function write(int $row, int $column): void {
        $this->cells[$row*$this->columns+$column]=3;
    }
}

$grid=new MissingNumericCells();
try { $grid->write($argc,$argc+1); }
catch (Error $error) { echo 'caught'; }
unset($grid);
"#;
    // Array writes auto-initialize an uninitialized PHP array property. Like a buffer,
    // an ArrayObject property must already hold its indexable object before this write.
    let oracle = source.replace("public buffer<int>", "public ArrayObject");
    for optimized in [false, true] {
        assert_eq!(run_with_oracle(source, &oracle, optimized).0, "caught");
    }
}

/// Reused transient cells preserve both input payloads and float continuation in either IR mode.
#[test]
fn test_stack_numeric_transient_cell_reuse_keeps_numeric_payloads() {
    let source = r#"<?php
function transientChain(int $seed, int $step): float {
    return (($seed+$step)+$step)+$step;
}
function repeatedInputs(int $seed, int $step): float {
    return ($seed+$step)+($seed+$step);
}
$value=transientChain(3,$argc); echo gettype($value),':',(int)$value;
$value=transientChain(PHP_INT_MAX,$argc);
echo '|',gettype($value),':',($value===(float)PHP_INT_MAX?'high':'bad');
$value=transientChain(PHP_INT_MIN,-4096);
echo '|',gettype($value),':',($value<(float)PHP_INT_MIN?'low':'bad');
echo '|',(int)repeatedInputs(3,$argc);
echo '|',(repeatedInputs(PHP_INT_MAX,$argc)===(float)PHP_INT_MAX*2.0?'double-high':'bad');
"#;
    for optimized in [false, true] {
        assert_eq!(run(source, optimized).0, "double:6|double:high|double:low|8|double-high");
    }
}
