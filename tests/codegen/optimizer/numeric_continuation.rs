//! Purpose:
//! Checks that numeric locals preserve PHP float promotion around loop backedges.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - Both IR modes use heap validation and optionally compare with an actual PHP interpreter.
//! - Casts after several overflowing iterations distinguish persistent floats from truncated ints.

use super::*;

/// Checks the observable result, ownership cleanup and optional PHP oracle in both IR modes.
fn assert_modes(source: &str, expected: &str) {
    for optimized in [false, true] {
        let dir = make_cli_test_dir("elephc_numeric_continuation");
        let path = dir.join("main.php");
        fs::write(&path, source).unwrap();
        let mut command = elephc_cli_command(&dir);
        command.args(["--heap-debug", "--gc-stats"]);
        if !optimized { command.arg("--no-ir-opt"); }
        let compiled = command.arg(&path).output().unwrap();
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let output = run_binary(&path.with_extension(""), &dir);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
        if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
            let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "actual PHP oracle");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}

/// A final explicit cast must observe all float additions, rather than an intermediate int cast.
#[test]
fn test_numeric_continuation_for_add_and_subtract() {
    assert_modes(r#"<?php
function add_continue(int $n): int {
    $s = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) { $s += 1; }
    return @(int)$s;
}

function sub_continue(int $n): int {
    $s = PHP_INT_MIN + 1;
    for ($i = 0; $i < $n; $i++) { $s -= 1; }
    return @(int)$s;
}
function fractional_continue(int $n): int {
    $s = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) { $s += 1; $s -= 0.5; }
    return @(int)$s;
}
echo add_continue($argc + 2), '|', sub_continue($argc + 2), '|', fractional_continue($argc + 2);
"#, "-9223372036854775808|-9223372036854775808|-9223372036854775808");
}

/// Copying through another ordinary local cannot turn the next iteration's float back into int.
#[test]
fn test_numeric_continuation_through_local_copy() {
    assert_modes(r#"<?php
function copied_for(int $n): int {
    $sum = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) { $temporary = $sum + 1; $sum = $temporary; }
    return @(int)$sum;
}
function copied_while(int $n): int {
    $sum = PHP_INT_MAX - 1; $i = 0;
    while ($i < $n) { $first = $sum + 1; $second = $first; $sum = $second; $i++; }
    return @(int)$sum;
}
echo copied_for($argc + 2), '|', copied_while($argc + 2);
"#, "-9223372036854775808|-9223372036854775808");
}

/// A bound promoted beyond the integer domain cannot justify narrowing its induction variable.
#[test]
fn test_numeric_continuation_promoted_bound_keeps_counter_tags() {
    assert_modes(r#"<?php
function growing_bound(int $n): void {
    $limit = PHP_INT_MAX; $value = PHP_INT_MAX - 1; $iterations = 0;
    while ($value < $limit) {
        $limit += 4096; $value++; $iterations++;
        if ($iterations == $n) { break; }
    }
    echo gettype($value), ':', @(int)$value;
}
growing_bound($argc + 2);
"#, "double:-9223372036854775808");
}

/// While and do/while loops share the same numeric storage requirement as for loops.
#[test]
fn test_numeric_continuation_while_and_do_while() {
    assert_modes(r#"<?php
function while_continue(int $n): int {
    $s = PHP_INT_MAX - 1; $i = 0;
    while ($i < $n) { $s += 1; $i++; }
    return @(int)$s;
}
function do_continue(int $n): int {
    $s = PHP_INT_MAX - 1; $i = 0;
    do { $s += 1; $i++; } while ($i < $n);
    return @(int)$s;
}
echo while_continue($argc + 2), '|', do_continue($argc + 2);
"#, "-9223372036854775808|-9223372036854775808");
}

/// Incoming parameter ABI stays integer while the mutable local and its snapshot can become float.
#[test]
fn test_numeric_continuation_parameter_and_snapshot() {
    assert_modes(r#"<?php
function parameter_continue(int $seed, int $n): int {
    for ($i = 0; $i < $n; $i++) { $seed += 1; }
    return @(int)$seed;
}

function snapshot_continue(int $n): void {
    $s = PHP_INT_MAX - 1; $copy = 0;
    for ($i = 0; $i < $n; $i++) {
        $s += 1;
        if ($i == 1) { $copy = $s; }
    }
    $read = function() use ($s) { return @(int)$s; };
    echo @(int)$copy, ':', $read();
}
echo parameter_continue(PHP_INT_MAX - 1, $argc + 2), '|';
snapshot_continue($argc + 2);
"#, "-9223372036854775808|-9223372036854775808:-9223372036854775808");
}

/// Integer counters that actually overflow retain PHP float tags in either optimizer mode.
#[test]
fn test_numeric_continuation_overflowing_increment_and_decrement() {
    assert_modes(r#"<?php
function increment_continue(int $n): void {
    $value = PHP_INT_MAX - 1;
    for ($count = 0; $count < $n; $count++) { $value++; }
    echo gettype($value), ':', @(int)$value, '|';
}
function decrement_continue(int $n): void {
    $value = PHP_INT_MIN + 1;
    for ($count = 0; $count < $n; $count++) { $value--; }
    echo gettype($value), ':', @(int)$value;
}
increment_continue($argc + 2); decrement_continue($argc + 2);
"#, "double:-9223372036854775808|double:-9223372036854775808");
}

/// Ending a binding after the loop cannot hide its earlier numeric backedge from discovery.
#[test]
fn test_numeric_continuation_before_unset_and_string_rebind() {
    assert_modes(r#"<?php
function rebind_continue(int $n): string {
    $s = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) { $s += 1; }
    echo @(int)$s, '|';
    unset($s);
    $s = (string)$n;
    $s .= 'x';
    return $s;
}
$text = rebind_continue($argc + 2);
echo $text;
"#, "-9223372036854775808|3x");
}

/// A catch-only backedge must preserve the promoted float and release each exception normally.
#[test]
fn test_numeric_continuation_through_exceptional_backedge() {
    assert_modes(r#"<?php
function exceptional_continue(int $n): int {
    $s = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) {
        try { $s += 1; throw new Exception('next'); }
        catch (Exception $error) { }
    }
    return @(int)$s;
}
echo exceptional_continue($argc + 2);
"#, "-9223372036854775808");
}

/// Target-neutral replay must compile through every supported ABI with either optimizer setting.
#[test]
fn test_numeric_continuation_all_supported_targets() {
    let source = r#"<?php
#[Export]
function exported_numeric_continue(int $n): int {
    $s = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) { $s += 1; }
    return @(int)$s;
}
#[Export]
function exported_numeric_inclusive(int $n, float $limit): int {
    $sum = 0;
    for ($i=1; $i<=$n; $i++) { $sum += $i; }
    return (int)$sum + (int)($sum < $limit);
}
echo exported_numeric_continue($argc + 2), exported_numeric_inclusive($argc+2, INF);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for optimized in [false, true] {
            let dir = make_cli_test_dir("elephc_numeric_continuation_target");
            let path = dir.join("main.php");
            fs::write(&path, source).unwrap();
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
            if !optimized { command.arg("--no-ir-opt"); }
            let output = command.arg(&path).output().unwrap();
            assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
            assert!(!fs::read_to_string(path.with_extension("s")).unwrap().is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

/// Both operands may point at the old cell; multiplication preserves later float iterations too.
#[test]
fn test_numeric_continuation_self_add_and_multiply() {
    assert_modes(r#"<?php
function self_continue(int $n): int {
    $s = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) { $s += $s; }
    return @(int)$s;
}
function multiply_continue(int $n): int {
    $s = PHP_INT_MIN + 1;
    for ($i = 0; $i < $n; $i++) { $s *= 2; }
    return @(int)$s;
}
echo self_continue($argc + 2), '|', multiply_continue($argc + 2);
"#, "0|0");
}

/// Executes a runtime-sized loop with ownership checks and returns its actual allocation count.
fn allocations(source: &str, n: usize) -> u64 {
    let dir = make_cli_test_dir("elephc_numeric_continuation_allocations");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let output = elephc_cli_command(&dir).args(["--heap-debug", "--gc-stats"]).arg(&path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let result = Command::new(path.with_extension("")).arg(n.to_string()).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    let (allocs, frees) = parse_gc_stats(&stderr);
    assert_eq!(allocs, frees, "every owner must be retired");
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).arg(n.to_string()).output().unwrap();
        assert!(php.status.success());
        assert_eq!(result.stdout, php.stdout, "actual PHP oracle");
    }
    fs::remove_dir_all(dir).unwrap();
    allocs
}

/// Inclusive unknown bounds keep numeric cells in the frame even when a raw counter is unproven.
#[test]
fn test_numeric_continuation_inclusive_counter_allocation_is_bounded() {
    let source = r#"<?php
function inclusive_sum(int $n): int {
    $s = 0;
    for ($i = 1; $i <= $n; $i++) { $s += $i; }
    return (int)$s;
}
echo inclusive_sum((int)$argv[1]);
"#;
    let small = allocations(source, 1000);
    let large = allocations(source, 10000);
    assert!(large.abs_diff(small) <= 8, "inclusive allocation count: {small} -> {large}");
    assert_modes(&source.replace("(int)$argv[1]", "$argc + 2"), "6");
}

/// Numeric replay must not box a different string binding before or after an explicit unset.
#[test]
fn test_numeric_continuation_unset_bindings_keep_geometric_string_growth() {
    let number = "$s=PHP_INT_MAX-1; for($i=0;$i<$count;$i++){$s+=1;} echo @(int)$s,'|';";
    let string = "$s=''; for($j=0;$j<$n;$j++){$s.='x';} echo strlen($s),'|';";
    for body in [format!("{number} unset($s); {string}"), format!("{string} unset($s); {number}")] {
        let source = format!("<?php function probe(int $n,int $count):void {{{body}}} probe((int)$argv[1],$argc+1);");
        let small = allocations(&source, 1000);
        let large = allocations(&source, 10000);
        assert!(large.abs_diff(small) <= 12, "distinct binding allocations: {small} -> {large}");
    }
}

/// Numeric comparison preserves NaN ordering, integer precision and float counter continuation.
#[test]
fn test_numeric_continuation_numeric_comparisons_match_php() {
    let source = r#"<?php
function ordered(int $n, float $limit): void {
    $value = PHP_INT_MAX - 1;
    for ($i=0; $i<$n; $i++) { $value++; }
    echo (int)($value < $limit), (int)($value <= $limit), (int)($value > $limit), (int)($value >= $limit), '|';
    echo @(int)$value, '|';
}
function exact_pair(int $n, int $step): void {
    $left=PHP_INT_MAX-2; $right=PHP_INT_MAX-1;
    for($i=0; $i<$n; $i++) { $left+=$step; $right+=$step; }
    echo (int)($left<$right), (int)($left>=$right), '|';
}
function scalar_increment(int $n): int {
    $value=PHP_INT_MAX-1;
    for($i=0; $i<$n; $i++) { $value++; }
    return @(int)$value;
}
function scalar_decrement(int $n): int {
    $value=PHP_INT_MIN+1;
    for($i=0; $i<$n; $i++) { $value--; }
    return @(int)$value;
}
function signed_zero(int $n, int $step, float $limit): void {
    $value=0;
    for($i=0; $i<$n; $i++) { $value+=$step; }
    echo (int)($value<$limit), (int)($value<=$limit), (int)($value>$limit), (int)($value>=$limit);
}
ordered($argc+2, NAN); ordered($argc+2, INF); exact_pair($argc+1, $argc-1);
echo scalar_increment($argc+2), '|', scalar_decrement($argc+2), '|';
signed_zero($argc+1, $argc-1, -0.0);
"#;
    assert_modes(source, "0000|-9223372036854775808|1100|-9223372036854775808|10|-9223372036854775808|-9223372036854775808|0101");
}

/// An actually promoted index must publish float payloads rather than box addresses to packed fields.
#[test]
fn test_numeric_continuation_promoted_value_into_packed_float() {
    let source = r#"<?php
packed class NumericContinuationCell { public float $value; }
function stored_promotion(int $n): float {
    $items = buffer_new<NumericContinuationCell>(1);
    $value = PHP_INT_MAX - 1;
    for ($i = 0; $i < $n; $i++) {
        $items[0]->value = $value * 1.5;
        $value++;
    }
    $result = $items[0]->value;
    buffer_free($items);
    return $result;
}
echo stored_promotion($argc + 2);
"#;
    let model = source.replace("packed class ", "class ")
        .replace("buffer_new<NumericContinuationCell>(1)", "[new NumericContinuationCell()]")
        .replace("buffer_free($items);", "unset($items);");
    let dir = make_cli_test_dir("elephc_numeric_continuation_packed");
    let path = dir.join("main.php");
    let model_path = dir.join("model.php");
    fs::write(&path, source).unwrap();
    fs::write(&model_path, model).unwrap();
    let expected = b"1.3835058055282E+19";
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&model_path).output().unwrap();
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(php.stdout, expected);
    }
    for optimized in [false, true] {
        let mut command = elephc_cli_command(&dir);
        command.args(["--heap-debug", "--gc-stats"]);
        if !optimized { command.arg("--no-ir-opt"); }
        let compiled = command.arg(&path).output().unwrap();
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let output = run_binary(&path.with_extension(""), &dir);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, expected, "optimized={optimized}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"));
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut command = elephc_cli_command(&dir);
            command.args(["--emit-asm", "--target", target]);
            if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
            if !optimized { command.arg("--no-ir-opt"); }
            let compiled = command.arg(&path).output().unwrap();
            assert!(compiled.status.success(), "{target}: {}", String::from_utf8_lossy(&compiled.stderr));
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Heterogeneous foreach values preserve float tags after an integer accumulator seed.
#[test]
fn test_numeric_continuation_mixed_foreach_preserves_fractional_sum() {
    assert_modes(r#"<?php
function fractional_foreach(int $seed): void {
    $values = [$seed, 1.5, 1.5]; $sum = 0;
    foreach ($values as $value) { $sum += $value; }
    echo $sum, ':', gettype($sum);
}
fractional_foreach($argc);
"#, "4:double");
}
