//! Purpose:
//! Regression coverage for checked integer updates of declared `int` properties (task ef49d6e1).
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - The postfix, compound and expanded update spellings must not heap-box the checked result.
//! - Overflow keeps PHP's typed-property `TypeError`, leaves the slot unchanged, and an
//!   in-range promoted float (PHP_INT_MIN - 1) still stores PHP_INT_MIN.
//! - Every fixture runs in both EIR modes with heap validation.

use super::*;

/// Cross-checks PHP-compatible fixtures when the focused run opts into the PHP oracle.
fn php_oracle(source: &str, expected: &str) {
    if std::env::var_os("ELEPHC_PHP_CHECK").is_none() { return; }
    let output = Command::new("php").args(["-n", "-r", source.trim_start_matches("<?php")])
        .output().expect("run PHP property update oracle");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

/// Compiles one variant and validates its heap before returning its standard output.
fn run(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_integer_property_updates");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.arg("--heap-debug");
    if !optimized { command.arg("--no-ir-opt"); }
    let compile = command.arg(&path).output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    stdout
}

/// The sprite `reserve()` shape and its expanded and compound spellings allocate per call
/// nothing: the window cost is flat in the call count (unoptimized EIR keeps one loop-counter
/// seed per window) and zero once EIR optimization runs.
#[test]
fn test_integer_property_updates_do_not_allocate() {
    let source = r#"<?php
final class Counter {
    private int $count = 0;
    public function reserve(): int { return 4 * $this->count++; }
    public function expanded(): int { $old = $this->count; $this->count = $old + 1; return 4 * $old; }
    public function compound(int $step): int { $this->count += $step; return $this->count; }
}
function measure(Counter $counter, int $mode, int $count): array {
    $sum = 0;
    $start = memory_get_allocations();
    for ($i = 0; $i < $count; $i++) {
        if ($mode === 0) { $sum ^= $counter->reserve(); }
        elseif ($mode === 1) { $sum ^= $counter->expanded(); }
        else { $sum ^= $counter->compound(3); }
    }
    $end = memory_get_allocations();
    return [$end - $start, $sum];
}
$counter = new Counter();
for ($mode = 0; $mode < 3; $mode++) {
    [$few] = measure($counter, $mode, 10);
    [$many, $sum] = measure($counter, $mode, 1000);
    echo $few === $many ? "flat" : "grows $few/$many", ":", $many, ":", $sum, "|";
}
"#;
    for optimized in [false, true] {
        let stdout = run(source, optimized);
        let windows: Vec<&str> = stdout.trim_end_matches('|').split('|').collect();
        let sums: Vec<&str> = windows.iter().map(|w| w.rsplit(':').next().unwrap()).collect();
        assert_eq!(sums, ["0", "0", "8064"], "optimized={optimized}: {stdout}");
        for window in &windows {
            assert!(window.starts_with("flat:"), "optimized={optimized}: {stdout}");
            if optimized {
                assert!(window.starts_with("flat:0:"), "optimized={optimized}: {stdout}");
            }
        }
    }
}

/// Overflow keeps PHP's typed-property TypeError, the old value and postfix snapshots.
#[test]
fn test_integer_property_updates_keep_overflow_semantics() {
    let source = r#"<?php
class Base { public int $n = 0; public int $m = 0; }
final class Child extends Base {}
function bump(Base $c, int $step): void {
    try { $c->n += $step; echo "stored "; } catch (TypeError $e) { echo $e->getMessage(), " "; }
}
$c = new Child();
$c->n = PHP_INT_MAX - 1;
$old = $c->n++;
echo $old === PHP_INT_MAX - 1 ? "old " : "bad ", $c->n === PHP_INT_MAX ? "max " : "bad ";
bump($c, $argc);
try { $c->n = $c->n * 2; } catch (TypeError $e) { echo $e->getMessage(), " "; }
echo $c->n === PHP_INT_MAX ? "kept " : "bad ";
$c->m = PHP_INT_MIN;
$c->m = $c->m - $argc;
echo $c->m === PHP_INT_MIN ? "min" : "bad";
"#;
    let expected = "old max Cannot assign float to property Base::$n of type int \
Cannot assign float to property Base::$n of type int kept min";
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}
