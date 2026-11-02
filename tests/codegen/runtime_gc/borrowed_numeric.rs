//! Purpose:
//! Checks PHP semantics, owned results and heap allocation cost of borrowed integer operands.
//!
//! Called from:
//! - Focused `borrowed_numeric` codegen tests on the executable supported targets.
//!
//! Key details:
//! - Both optimizer modes run with heap diagnostics and optional real PHP comparison.
//! - The integer operand lives only across one read-only runtime numeric call.
//! - Results must remain independently owned after other calls and source mutations.

use crate::support::*;

/// Removes only this fixture's source, runtime cache and executable on every exit.
struct FixtureDir(std::path::PathBuf);

impl Drop for FixtureDir {
    /// Removes this test-owned directory after normal completion or assertion failure.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Checks output and ownership in both optimizer modes, optionally bounding optimized allocations.
fn check_numeric(source: &str, expected: &str, allocation_limit: Option<usize>) {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = FixtureDir(make_cli_test_dir("elephc_borrowed_numeric"));
        let path = dir.0.join("main.php");
        fs::write(&path, source).expect("write borrowed numeric fixture");
        let compile = elephc_cli_command(&dir.0)
            .args([mode, "--php-version=8.5", "--heap-debug", "--gc-stats"])
            .arg(&path).output().expect("compile borrowed numeric fixture");
        assert!(compile.status.success(), "{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        let output = run_binary(&dir.0.join("main"), &dir.0);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, expected.as_bytes(), "{mode}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        let stats = stderr.lines().find_map(|line| line.strip_prefix("GC: allocs="))
            .expect("GC allocation and free counters");
        let (allocs, frees) = stats.split_once(" frees=").expect("GC counter format");
        let allocs: usize = allocs.parse().unwrap();
        let frees: usize = frees.parse().unwrap();
        assert_eq!(allocs, frees, "{mode}: {stderr}");
        if mode == "--ir-opt=on" {
            if let Some(limit) = allocation_limit {
                assert!(allocs <= limit, "temporary integer boxes returned: {allocs} > {limit}");
            }
        }
        if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
            let php = Command::new("php").arg("-n").arg(&path).output().expect("run PHP oracle");
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "PHP mismatch with {mode}");
        }
    }
}

/// Integer limits promote to float; bool, null, float and dynamic RHS retain ordinary tags.
#[test]
fn test_borrowed_numeric_coercions_and_overflow() {
    check_numeric(r#"<?php
function addOne($x) { return $x + 1; }
function subtractMinimum($x) { return $x - PHP_INT_MIN; }
function multiplyTwo($x) { return $x * 2; }
foreach ([0, PHP_INT_MAX, PHP_INT_MIN, 1.5, '1.5', true, null] as $value) {
    var_dump(addOne($value), subtractMinimum($value), multiplyTwo($value));
}
function withBool($x) { return $x + true; }
function withNull($x) { return $x + null; }
function withFloat($x) { return $x + 1.5; }
function withDynamic($x, int $right) { return $x + $right; }
var_dump(withBool(2), withNull(2), withFloat(2), withDynamic(2, $argc));
"#, r#"int(1)
float(9.223372036854776E+18)
int(0)
float(9.223372036854776E+18)
float(1.8446744073709552E+19)
float(1.8446744073709552E+19)
int(-9223372036854775807)
int(0)
float(-1.8446744073709552E+19)
float(2.5)
float(9.223372036854776E+18)
float(3)
float(2.5)
float(9.223372036854776E+18)
float(3)
int(2)
float(9.223372036854776E+18)
int(2)
int(1)
float(9.223372036854776E+18)
int(0)
int(3)
int(2)
float(3.5)
int(3)
"#, None);
}

/// Saved results outlive a later numeric call and remain separate from input aliases and references.
#[test]
fn test_borrowed_numeric_independent_results_and_source_aliases() {
    check_numeric(r#"<?php
function addSeven($value) { return $value + 7; }
function other($value) { return $value + 12345; }
$input = json_decode('0');
$saved = addSeven($input);
$noise = other($input);
unset($noise);
$copy = $input;
$reference =& $input;
$reference = 41;
var_dump($saved, $copy, $input);
"#, r#"int(7)
int(0)
int(41)
"#, None);
}

/// Integer and float increments preserve promotion; numeric and alphabetic strings keep PHP rules.
#[test]
fn test_borrowed_numeric_increment_decrement_and_string_fallback() {
    check_numeric(r#"<?php
error_reporting(0);
function nextValue($value) { $value++; return $value; }
function previousValue($value) { $value--; return $value; }
foreach ([0, PHP_INT_MAX, PHP_INT_MIN, 1.5, '9', '1.5', 'az'] as $value) {
    var_dump(nextValue($value), previousValue($value));
}
function postValue($value) { $old = $value++; return [$old, $value]; }
$pair = postValue(PHP_INT_MAX);
var_dump($pair[0], $pair[1]);
$pair = postValue('9');
var_dump($pair[0], $pair[1]);
"#, r#"int(1)
int(-1)
float(9.223372036854776E+18)
int(9223372036854775806)
int(-9223372036854775807)
float(-9.223372036854776E+18)
float(2.5)
float(0.5)
int(10)
int(8)
float(2.5)
float(0.5)
string(2) "ba"
string(2) "az"
int(9223372036854775807)
float(9.223372036854776E+18)
string(1) "9"
int(10)
"#, None);
}

/// Two thousand loop operations no longer allocate their temporary integer operand cells.
#[test]
fn test_borrowed_numeric_allocation_budget() {
    check_numeric(r#"<?php
function numericPart(int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $i; }
    return (int)$sum;
}
function closurePart(int $n): int {
    $f = function($x) { return $x + 1; };
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $f($i); }
    return (int)$sum;
}
$n=(int)($argc+999);
echo numericPart($n), ':', closurePart($n);
"#, r#"499500:500500"#, Some(6032));
}

