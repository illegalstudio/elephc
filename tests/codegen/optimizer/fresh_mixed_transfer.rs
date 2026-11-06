//! Purpose:
//! Regression coverage for transferring fresh numeric assignment boxes.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::fresh_mixed_transfer`.
//!
//! Key details:
//! - Checks allocation scaling, aliases, PHP overflow, and both optimizer modes.
//! - Target-independent ownership rewriting is checked on every supported target.

use super::*;

const SUM: &str = r#"<?php
$n = (int) ($argc * 1000);
$sum = 0;
for ($i = 1; $i <= $n; $i++) { $sum += $i; }
echo $sum;
"#;

/// Compiles and executes one fixture with explicit optimizer and heap diagnostics.
fn run_fixture(source: &str, optimized: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_fresh_mixed_transfer");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("write numeric transfer fixture");
    let mode = if optimized { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir).args([mode, "--heap-debug", "--gc-stats"])
        .arg(&php_path).output().expect("compile numeric transfer fixture");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = run_binary(&dir.join("main"), &dir);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&php_path).output().expect("run PHP oracle");
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(run.stdout, php.stdout, "PHP mismatch with {mode}");
    }
    let stdout = String::from_utf8(run.stdout).expect("UTF-8 stdout");
    let stderr = String::from_utf8(run.stderr).expect("UTF-8 stderr");
    fs::remove_dir_all(dir).expect("remove numeric transfer fixture");
    (stdout, stderr)
}

/// Reads the runtime allocation counter without depending on fixed startup costs.
fn allocations(stderr: &str) -> usize {
    stderr.split("GC: allocs=").nth(1).expect("GC allocation counter")
        .split_whitespace().next().unwrap().parse().expect("numeric allocation counter")
}

/// A checked accumulator needs one numeric box per update, without a second copy.
#[test]
fn test_fresh_mixed_transfer_removes_per_update_copy() {
    let (plain, plain_heap) = run_fixture(SUM, false);
    let (optimized, optimized_heap) = run_fixture(SUM, true);
    assert_eq!(plain, "500500");
    assert_eq!(optimized, plain);
    assert!(allocations(&optimized_heap) <= 1020, "{optimized_heap}");
    assert!(allocations(&plain_heap) >= 2000, "{plain_heap}");
    assert!(optimized_heap.contains("HEAP DEBUG: leak summary: clean"), "{optimized_heap}");
    assert!(plain_heap.contains("HEAP DEBUG: leak summary: clean"), "{plain_heap}");
}

/// Copies and references keep independent cells and retain promoted float values.
#[test]
fn test_fresh_mixed_transfer_preserves_aliases_and_overflow() {
    let source = r#"<?php
function copies(int $seed): void {
    $value = $seed + 10;
    $copy = $value;
    $value += 2;
    echo $value, '|', $copy, '|';
    $reference =& $value;
    $reference += 3;
    echo $value, '|', $copy, "\n";
}
copies($argc);
$positive = PHP_INT_MAX - $argc;
$negative = PHP_INT_MIN + $argc;
for ($i = 0; $i < 3; $i++) {
    $positive += 2;
    $negative -= 2;
    $product = PHP_INT_MAX;
    $product *= 2;
    echo gettype($positive), '|', gettype($negative), '|', gettype($product), "\n";
}
"#;
    let (plain, _) = run_fixture(source, false);
    let (optimized, _) = run_fixture(source, true);
    assert_eq!(optimized, plain);
    assert_eq!(optimized, "13|11|16|11\ndouble|double|double\ndouble|double|double\ndouble|double|double\n");
}

/// Every target removes the assignment clone while retaining checked arithmetic.
#[test]
fn test_fresh_mixed_transfer_all_target_eir() {
    let dir = make_cli_test_dir("elephc_fresh_mixed_transfer_targets");
    let php_path = dir.join("main.php");
    fs::write(&php_path, SUM).expect("write target fixture");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let result = elephc_cli_command(&dir).args(["--emit-ir", "--target", target])
            .arg(&php_path).output().expect("emit target EIR");
        assert!(result.status.success(), "{target}: {}", String::from_utf8_lossy(&result.stderr));
        let ir = String::from_utf8(result.stdout).expect("UTF-8 EIR");
        let main = ir.split("function main(").nth(1).expect("main EIR")
            .split("\n  function ").next().unwrap();
        let numeric_add = main.contains(" = ichecked_add ") || main.lines().any(|line| {
            line.contains(" = mixed_numeric_binop ") && line.contains(" add ")
        });
        assert!(numeric_add, "{target}: promotion-aware numeric addition must remain: {main}");
        assert!(!main.contains(" = mixed_clone "), "{target}: {main}");
    }
    fs::remove_dir_all(dir).expect("remove target fixture");
}

/// Dynamic closure arithmetic keeps PHP overflow and value/reference ownership semantics.
#[test]
fn test_fresh_mixed_transfer_dynamic_closure_arithmetic() {
    let source = r#"<?php
function arithmetic($seed): void {
    $f = function($x) { return $x + 1; };
    $sum = $seed;
    for ($i = 0; $i < 3; $i++) { $sum += $f($i); }
    $copy = $sum;
    $sum += 2;
    echo gettype($sum), '|', $sum, '|', $copy, "\n";
    $reference =& $sum;
    $reference *= 2;
    echo gettype($sum), '|', $sum, '|', $copy, "\n";
}


arithmetic($argc);
arithmetic(1.5);
arithmetic(PHP_INT_MAX);
$left = json_decode('2');
$right = json_decode('3');
$difference = $left - $right;
$product = $left * $right;
echo $difference, '|', $product, '|', $left, '|', $right, "\n";
"#;
    let (plain, plain_heap) = run_fixture(source, false);
    let (optimized, optimized_heap) = run_fixture(source, true);
    assert_eq!(optimized, plain);
    assert!(optimized.starts_with("integer|9|7\ninteger|18|7\ndouble|9.5|7.5\ndouble|19|7.5\n"), "{optimized}");
    assert!(optimized.ends_with("-1|6|2|3\n"), "{optimized}");
    assert!(optimized_heap.contains("HEAP DEBUG: leak summary: clean"), "{optimized_heap}");
    assert!(plain_heap.contains("HEAP DEBUG: leak summary: clean"), "{plain_heap}");
}

/// Every target keeps dynamic arithmetic while the finalizer removes only its assignment copy.
#[test]
fn test_fresh_mixed_transfer_dynamic_all_target_eir() {
    let dir = make_cli_test_dir("elephc_dynamic_transfer_targets");
    let path = dir.join("main.php");
    fs::write(&path, "<?php function add($x) { $sum = $x + 1; return $sum; } echo add($argc);")
        .expect("write dynamic transfer fixture");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for mode in ["on", "off"] {
            let result = elephc_cli_command(&dir).args(["--emit-ir", "--target", target,
                &format!("--ir-opt={mode}")]).arg(&path).output().expect("emit dynamic target EIR");
            assert!(result.status.success(), "{target}: {}", String::from_utf8_lossy(&result.stderr));
            let ir = String::from_utf8(result.stdout).expect("UTF-8 EIR");
            let function = ir.split("function add(").nth(1).expect("add EIR")
                .split("\n  }\n").next().unwrap();
            let numeric_tail = function.split(" = mixed_numeric_binop ").nth(1).expect("dynamic arithmetic");
            assert_eq!(numeric_tail.contains(" = mixed_clone "), mode == "off", "{target}, {mode}: {function}");
        }
    }
    fs::remove_dir_all(dir).expect("remove dynamic target fixture");
}

/// A dynamic accumulator allocates its result and raw-operand box without an assignment copy.
#[test]
fn test_fresh_mixed_transfer_dynamic_allocation_budget() {
    let source = r#"<?php
function accumulate($seed, int $n): int {
    $sum = $seed;
    for ($i = 0; $i < $n; $i++) { $sum += $i; }
    return (int)$sum;
}
echo accumulate($argc, 1000);
"#;
    let (out, heap) = run_fixture(source, true);
    assert_eq!(out, "499501");
    assert!(allocations(&heap) <= 2024, "{heap}");
    assert!(heap.contains("HEAP DEBUG: leak summary: clean"), "{heap}");
}
