//! Purpose:
//! Executes loop counters bounded by a checked difference (`$i < $n - 1`) or seeded by checked
//! arithmetic on an enclosing counter (`$j = $i + 1`), and checks they run as raw integers.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - Runtime cases run with IR optimization on and off and strict heap validation; the PHP
//!   fixtures also run against the optional real PHP oracle. The boundary cases overflow the
//!   bound at PHP_INT_MIN and PHP_INT_MAX, where PHP compares an int with the promoted float
//!   (`$i <= PHP_INT_MIN - 1` holds for the 513 integers that convert to -2^63).
//! - EIR assertions cover the pair-loop shapes and the buffer shape of Ivory's
//!   `Grid::findPairs`, which used `php_rel_cmp` and a boxed `str_inc_dec` for both counters.

use super::*;

/// Pair loops over `$n - 1` and `$i + 1`, strict and inclusive, plus bounds that change per run.
const PAIR_LOOPS: &str = r#"<?php
function pairCount(int $n): int {
    $pairs = 0;
    for ($i = 0; $i < $n - 1; $i++) {
        for ($j = $i + 1; $j < $n; $j++) {
            $pairs += $i ^ $j;
        }
    }
    return $pairs;
}
function inclusivePairs(int $n): int {
    $pairs = 0;
    for ($i = 0; $i <= $n - 1; $i++) {
        for ($j = $i + 1; $j <= $n - 1; $j++) {
            $pairs += 1;
        }
    }
    return $pairs;
}
function runs(array $ends): string {
    $out = '';
    $start = 0;
    foreach ($ends as $end) {
        for ($i = $start; $i < $end - 1; $i++) {
            for ($j = $i + 1; $j < $end; $j++) {
                $out .= $i . $j . ',';
            }
        }
        $start = $end;
        $out .= '|';
    }
    return $out;
}
echo pairCount($argc + 9), '|', inclusivePairs($argc + 4), '|', runs([3, 3, 6]), "\n";
"#;

/// The buffer-backed run scan of Ivory's grid broadphase: entries `starts[c]..starts[c + 1]`.
/// Buffers are manually managed, so the program frees both before the heap check runs.
const CELL_PAIRS: &str = r#"<?php
final class Cells {
    private buffer<int> $starts;
    private buffer<int> $entries;
    public function __construct(int $slots) {
        $this->starts = buffer_new<int>($slots);
        $this->entries = buffer_new<int>($slots);
    }
    public function set(int $start, int $entry, int $index): void {
        $this->starts[$index] = $start;
        $this->entries[$index] = $entry;
    }
    public function pairs(int $cells): int {
        $count = 0;
        for ($cell = 0; $cell < $cells; $cell++) {
            $end = $this->starts[$cell + 1];
            for ($i = $this->starts[$cell]; $i < $end - 1; $i++) {
                $a = $this->entries[$i];
                for ($j = $i + 1; $j < $end; $j++) {
                    if ($a < $this->entries[$j]) {
                        $count += 1;
                    }
                }
            }
        }
        return $count;
    }
    public function free(): void {
        $starts = $this->starts;
        $entries = $this->entries;
        buffer_free($starts);
        buffer_free($entries);
    }
}
$cells = new Cells(6);
foreach ([[0, 5], [3, 1], [5, 7], [0, 2], [0, 9]] as $index => [$start, $entry]) {
    $cells->set($start, $entry, $index);
}
echo $cells->pairs($argc + 1), "\n";
$cells->free();
"#;

/// Runs one IR mode with strict heap validation, and the PHP oracle when asked and requested.
fn run(source: &str, optimized: bool, php_oracle: bool) -> String {
    let dir = make_cli_test_dir("elephc_checked_bound_counter");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compiled = command.arg(&path).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    if php_oracle && std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(output.stdout, php.stdout, "actual PHP oracle");
    }
    let text = String::from_utf8(output.stdout).unwrap();
    fs::remove_dir_all(dir).unwrap();
    text
}

/// Emits the optimized EIR module text of `source`.
fn optimized_ir(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_checked_bound_counter_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let output = elephc_cli_command(&dir).arg("--emit-ir").arg(&path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    fs::remove_dir_all(dir).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// Returns the printed EIR of one function or method of a module dump.
fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
    ir.split(&format!("function {name}("))
        .nth(1)
        .unwrap_or_else(|| panic!("missing function {name} in:\n{ir}"))
        .split("\n  function ")
        .next()
        .unwrap()
}

/// Returns the instruction lines of `body` that apply `op`.
fn op_lines<'a>(body: &'a str, op: &str) -> Vec<&'a str> {
    let pattern = format!("= {op} ");
    body.lines().filter(|line| line.contains(&pattern)).collect()
}

/// Pair loops bounded by `$n - 1` and seeded by `$i + 1` print what PHP prints, in both modes.
#[test]
fn test_checked_bound_pair_loops_match_php() {
    for optimized in [false, true] {
        assert_eq!(run(PAIR_LOOPS, optimized, true), "297|10|01,02,12,||34,35,45,|\n");
    }
}

/// The grid-style run scan counts the same pairs in both IR modes.
#[test]
fn test_checked_bound_cell_pairs_count_runs() {
    for optimized in [false, true] {
        assert_eq!(run(CELL_PAIRS, optimized, false), "3\n");
    }
}

/// A bound that overflows keeps PHP's int-versus-float comparison and the counter stays an int.
///
/// `PHP_INT_MIN - 1` is the float -2^63: no integer is below it, but the 513 integers from
/// PHP_INT_MIN to PHP_INT_MIN + 512 convert to exactly -2^63 and satisfy `<=`. Counters run up
/// to one below PHP_INT_MAX, a `$i + 1` seed reaches PHP_INT_MAX, `PHP_INT_MAX + 1` bounds a
/// descending loop from above, and a seed that does overflow still produces PHP's float.
#[test]
fn test_checked_bound_counters_keep_php_overflow_semantics() {
    let source = r#"<?php
function belowMin(int $end): string {
    $seen = 0;
    for ($i = PHP_INT_MIN; $i < $end - 1; $i++) { $seen++; }
    return $seen . ':' . gettype($i);
}
function inclusiveBelowMin(int $end): string {
    $seen = 0;
    for ($i = PHP_INT_MIN; $i <= $end - 1; $i++) { $seen++; }
    return $seen . ':' . gettype($i) . ':' . ($i - PHP_INT_MIN);
}
function nearMax(int $end): string {
    $seen = '';
    for ($i = $end - 4; $i < $end - 1; $i++) {
        for ($j = $i + 1; $j < $end; $j++) { $seen .= ($j - $i); }
    }
    return $seen . ':' . gettype($i) . ':' . gettype($j) . ':' . ($end - $i) . ':' . ($end - $j);
}
function descendingAboveMax(int $start): string {
    $seen = 0;
    for ($i = PHP_INT_MAX; $i > $start + 1; $i--) { $seen++; }
    return $seen . ':' . gettype($i);
}
function overflowingSeed(int $top): string {
    $seen = '';
    for ($i = $top - 1; $i <= $top; $i++) {
        $j = $i + 1;
        $seen .= gettype($j) . ',';
        if ($i === PHP_INT_MAX) { break; }
    }
    return $seen;
}
echo belowMin(PHP_INT_MIN + $argc - 1), "\n";
echo belowMin(PHP_INT_MIN + $argc), "\n";
echo inclusiveBelowMin(PHP_INT_MIN + $argc - 1), "\n";
echo nearMax(PHP_INT_MAX - 1 + $argc), "\n";
echo descendingAboveMax(PHP_INT_MAX - 1 + $argc), "\n";
echo descendingAboveMax(PHP_INT_MAX - 3 + $argc), "\n";
echo overflowingSeed(PHP_INT_MAX - 1 + $argc), "\n";
"#;
    let expected = "0:integer\n0:integer\n513:integer:513\n123121:integer:integer:1:0\n\
        0:integer\n1:integer\ninteger,double,\n";
    for optimized in [false, true] {
        assert_eq!(run(source, optimized, true), expected);
    }
}

/// Both counters of a strict pair loop are raw integers and `$n - 1` is never boxed.
///
/// Before, the `$n - 1` bound kept `$i` boxed (`php_rel_cmp`, `str_inc_dec`) and the `$i + 1`
/// seed kept `$j` boxed the same way.
#[test]
fn test_checked_bound_pair_loop_counters_are_integers() {
    let ir = optimized_ir(PAIR_LOOPS);
    let body = function_body(&ir, "pairCount");
    assert!(op_lines(body, "str_inc_dec").is_empty(), "no boxed update:\n{body}");
    assert!(op_lines(body, "php_rel_cmp").is_empty(), "no boxed test:\n{body}");
    assert_eq!(op_lines(body, "ichecked_sub_to_int").len(), 1, "an unboxed `$n - 1`:\n{body}");
    assert!(
        op_lines(body, "icmp").iter().filter(|line| line.contains(" Slt ;")).count() >= 2,
        "both loops compare raw integers:\n{body}"
    );
}

/// An inclusive `<= $n - 1` keeps the PHP comparison for the bound, but not boxed counters.
#[test]
fn test_checked_bound_inclusive_counters_are_integers() {
    let ir = optimized_ir(PAIR_LOOPS);
    let body = function_body(&ir, "inclusivePairs");
    assert!(op_lines(body, "str_inc_dec").is_empty(), "no boxed update:\n{body}");
    for line in op_lines(body, "php_rel_cmp") {
        assert!(line.contains(" Sle ;"), "only the inclusive bound tests stay PHP comparisons:\n{body}");
    }
}

/// The grid run scan compares and steps both counters as raw integers.
#[test]
fn test_checked_bound_cell_pair_counters_are_integers() {
    let ir = optimized_ir(CELL_PAIRS);
    let body = function_body(&ir, "Cells::pairs");
    assert!(op_lines(body, "str_inc_dec").is_empty(), "no boxed update:\n{body}");
    assert!(op_lines(body, "php_rel_cmp").is_empty(), "no boxed test:\n{body}");
    assert_eq!(op_lines(body, "ichecked_sub_to_int").len(), 1, "an unboxed `$end - 1`:\n{body}");
}

/// Every supported target emits the checked-bound shapes in both IR modes.
#[test]
fn test_checked_bound_counters_emit_on_all_supported_targets() {
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        for (source, optimized) in [
            (PAIR_LOOPS, false),
            (PAIR_LOOPS, true),
            (CELL_PAIRS, false),
            (CELL_PAIRS, true),
        ] {
            let dir = make_cli_test_dir("elephc_checked_bound_counter_target");
            let path = dir.join("main.php");
            fs::write(&path, source).unwrap();
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
            assert!(!fs::read_to_string(path.with_extension("s")).unwrap().is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
