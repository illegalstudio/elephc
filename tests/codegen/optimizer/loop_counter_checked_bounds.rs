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

/// A compound-assignment counter (`$i += 1`) is proven raw integer exactly like `$i++`.
#[test]
fn test_compound_assignment_counter_is_raw_integer() {
    let source = r#"<?php
function compoundCount(int $n): int {
    $i = 0;
    while ($i < $n) {
        $i += 1;
    }
    return (int)$i;
}
echo compoundCount($argc + 7), "\n";
"#;
    let ir = optimized_ir(source);
    let body = function_body(&ir, "compoundCount");
    assert!(
        op_lines(body, "mixed_numeric_binop").is_empty(),
        "compound counter must not stay boxed:\n{body}"
    );
    assert!(op_lines(body, "str_inc_dec").is_empty(), "no string-aware update:\n{body}");
    assert!(!op_lines(body, "icmp").is_empty(), "loop test is a raw integer compare:\n{body}");
    assert!(body.contains("origin: integer_range"), "update is range-proven:\n{body}");
    for optimized in [false, true] {
        assert_eq!(run(source, optimized, true), "8\n");
    }
}

/// A top-level compound counter bounded by a literal keeps PHP semantics and loses its box.
#[test]
fn test_top_level_compound_counter_is_raw_integer() {
    let source = r#"<?php
$sum = 0;
$i = 1;
while ($i <= 200) {
    $sum += $i;
    $i += 1;
}
echo $sum, "\n";
"#;
    let ir = optimized_ir(source);
    let body = function_body(&ir, "main");
    assert_eq!(
        op_lines(body, "mixed_numeric_binop").len(),
        1,
        "only the accumulator stays boxed:\n{body}"
    );
    assert!(!op_lines(body, "icmp").is_empty(), "loop test is a raw integer compare:\n{body}");
    for optimized in [false, true] {
        assert_eq!(run(source, optimized, true), "20100\n");
    }
}

/// A read-only typed-array count in a loop condition is hoisted to the preheader.
#[test]
fn test_count_hoist_moves_array_length_out_of_condition() {
    let source = r#"<?php
function count_loop(int $n): int {
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
    return (int)$sum;
}
echo count_loop($argc + 9), "\n";
"#;
    let optimized = optimized_ir(source);
    let body = function_body(&optimized, "count_loop");
    assert_eq!(
        body.matches("runtime.count").count(),
        1,
        "the loop evaluates count once:\n{body}"
    );
    let mut conditions = body.split("while.cond");
    let _entry = conditions.next().expect("entry");
    let between = conditions.next().expect("first condition");
    let second = conditions.next().expect("second condition");
    assert!(
        between.contains("runtime.count") && between.contains("origin: count_hoist"),
        "count moved to the second loop's preheader:\n{between}"
    );
    assert!(
        !second.contains("runtime.count"),
        "count must leave the condition:\n{second}"
    );
    for optimized_mode in [false, true] {
        assert_eq!(run(source, optimized_mode, true), "45\n");
    }
}

/// A dense int array summed by its strict induction value; the read cannot miss.
const BOUNDED_SUM: &str = r#"<?php
function sum_bounded(int $n): int {
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
    return (int)$sum;
}
echo sum_bounded($argc + 9), "\n";
"#;

/// The same bounded sum with the induction observed after the loop, so the reduction
/// rewrite leaves the guarded/unchecked read path intact for the read-shape tests.
const BOUNDED_READ_SUM: &str = r#"<?php
function read_bounded(int $n): string {
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
    return $sum . ':' . $i;
}
echo read_bounded($argc + 9), "\n";
"#;

/// A strict typed-array bound turns the body's element read into an unchecked int load.
#[test]
fn test_array_get_bounds_removes_missing_key_and_bounds_guards() {
    let source = BOUNDED_READ_SUM;
    let optimized = optimized_ir(source);
    let body = function_body(&optimized, "read_bounded");
    assert!(
        body.contains("array_get_unchecked"),
        "the bounded read must lower to the unchecked op:\n{body}"
    );
    assert!(
        !body.contains("= array_get "),
        "no guarded read may remain in the bounded loop:\n{body}"
    );
    let loop_body = body
        .split("while.body")
        .nth(2)
        .expect("the bounded sum loop body");
    let before_read = loop_body
        .split("array_get_unchecked")
        .next()
        .expect("the unchecked read");
    assert!(
        !before_read.contains("concat_reset"),
        "the typed count keeps the loop entry concat-clean:\n{before_read}"
    );
    for optimized_mode in [false, true] {
        assert_eq!(run(source, optimized_mode, true), "45:10\n");
    }
}

/// A bounded boxed reduction becomes one `__rt_array_sum_int` helper call.
#[test]
fn test_array_sum_loop_rewrites_bounded_reduction() {
    let source = BOUNDED_SUM;
    let optimized = optimized_ir(source);
    let body = function_body(&optimized, "sum_bounded");
    assert!(
        body.contains("array_sum_int"),
        "the reduction must become one helper call:\n{body}"
    );
    assert!(
        !body.contains("mixed_numeric_binop"),
        "the boxed reduction loop must be gone:\n{body}"
    );
    for optimized_mode in [false, true] {
        assert_eq!(run(source, optimized_mode, true), "45\n");
    }
}

/// The helper keeps PHP's seed, float promotion and empty-range behavior.
#[test]
fn test_array_sum_loop_matches_php_seed_and_overflow() {
    let source = r#"<?php
function sum_overflow(): string {
    $data = []; $data[] = PHP_INT_MAX; $data[] = 1; $data[] = 2;
    $sum = 0; $i = 0;
    while ($i < count($data)) { $sum += $data[$i]; $i += 1; }
    return gettype($sum) . ':' . $sum;
}
function sum_float(): string {
    $data = []; $data[] = 1; $data[] = 2; $data[] = 3;
    $sum = 1.5; $i = 0;
    while ($i < count($data)) { $sum += $data[$i]; $i += 1; }
    return gettype($sum) . ':' . $sum;
}
function sum_empty(): string {
    $data = []; $sum = 5; $i = 0;
    while ($i < count($data)) { $sum += $data[$i]; $i += 1; }
    return gettype($sum) . ':' . $sum;
}
function sum_start(): string {
    $data = []; $data[] = 10; $data[] = 20; $data[] = 30; $data[] = 40;
    $sum = 0; $i = 2;
    while ($i < count($data)) { $sum += $data[$i]; $i += 1; }
    return gettype($sum) . ':' . $sum;
}
echo sum_overflow(), '|', sum_float(), '|', sum_empty(), '|', sum_start(), "\n";
"#;
    for optimized_mode in [false, true] {
        assert_eq!(
            run(source, optimized_mode, true),
            "double:9.2233720368548E+18|double:7.5|integer:5|integer:70\n"
        );
    }
}

/// An observed induction value keeps the boxed loop because the helper cannot return it.
#[test]
fn test_array_sum_loop_keeps_loop_when_index_is_observed() {
    let optimized = optimized_ir(
        r#"<?php
function sum_then_index(int $n): string {
    $data = [];
    $i = 0;
    while ($i < $n) { $data[] = $i % 97; $i += 1; }
    $sum = 0;
    $i = 0;
    while ($i < count($data)) { $sum += $data[$i]; $i += 1; }
    return $sum . ':' . $i;
}
echo sum_then_index($argc + 9), "\n";
"#,
    );
    let body = function_body(&optimized, "sum_then_index");
    assert!(
        !body.contains("array_sum_int") && body.contains("mixed_numeric_binop"),
        "an observed induction must keep the boxed loop:\n{body}"
    );
}

/// Every supported target emits the reduction helper call.
#[test]
fn test_array_sum_loop_emits_on_all_supported_targets() {
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let dir = make_cli_test_dir("elephc_array_sum_loop_target");
        let path = dir.join("main.php");
        fs::write(&path, BOUNDED_SUM).unwrap();
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--target", target]);
        if target.starts_with("ios-") {
            command.args(["--emit", "staticlib"]);
        }
        let output = command.arg(&path).output().unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = fs::read_to_string(path.with_extension("s")).unwrap();
        assert!(
            asm.contains("__rt_array_sum_int"),
            "{target}: missing the reduction helper call\n{asm}"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

/// A pure constant-bound fill loop reserves capacity and uses the reserved append.
#[test]
fn test_array_fill_loop_reserves_constant_fill() {
    let source = r#"<?php
function fill_and_sum(): string {
    $data = [];
    $i = 0;
    while ($i < 50000) { $data[] = $i % 97; $i += 1; }
    $sum = 0;
    $j = 0;
    while ($j < count($data)) { $sum += $data[$j]; $j += 1; }
    return count($data) . ':' . $sum;
}
echo fill_and_sum(), "\n";
"#;
    for optimized_mode in [false, true] {
        assert_eq!(run(source, optimized_mode, true), "50000:2398830\n");
    }
    let optimized = optimized_ir(source);
    let body = function_body(&optimized, "fill_and_sum");
    assert!(
        body.contains("array_push_reserved"),
        "the constant fill must use the reserved append:\n{body}"
    );
    assert!(
        !body.contains("array_push v"),
        "no checked append may remain in the constant fill:\n{body}"
    );
    let fill_body = body
        .split("while.body")
        .nth(1)
        .expect("the fill loop body");
    assert!(
        !fill_body.contains("concat_reset"),
        "the reserved fill keeps the loop concat-clean:\n{fill_body}"
    );
}

/// A runtime bound keeps the checked append because capacity cannot be reserved.
#[test]
fn test_array_fill_loop_keeps_runtime_bound() {
    let optimized = optimized_ir(BOUNDED_SUM);
    let body = function_body(&optimized, "sum_bounded");
    assert!(
        body.contains("array_push v"),
        "a runtime bound must keep the checked append:\n{body}"
    );
    assert!(
        !body.contains("array_push_reserved"),
        "a runtime bound must not reserve capacity:\n{body}"
    );
}

/// Every supported target emits the reserved append without the generic push helper.
#[test]
fn test_array_fill_loop_emits_on_all_supported_targets() {
    let source = r#"<?php
function fill(int $unused): int {
    $data = [];
    $i = 0;
    while ($i < 50000) { $data[] = $i % 97; $i += 1; }
    return count($data);
}
echo fill($argc), "\n";
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let dir = make_cli_test_dir("elephc_array_fill_loop_target");
        let path = dir.join("main.php");
        fs::write(&path, source).unwrap();
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--target", target]);
        if target.starts_with("ios-") {
            command.args(["--emit", "staticlib"]);
        }
        let output = command.arg(&path).output().unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = fs::read_to_string(path.with_extension("s")).unwrap();
        let body = asm
            .split("@block name=while.body")
            .find(|body| body.contains("op=array_push_reserved"))
            .unwrap_or_else(|| panic!("{target}: missing reserved fill body\n{asm}"));
        assert!(
            !body.contains("__rt_array_push_int") && !body.contains("__rt_array_push_reserved"),
            "{target}: the reserved append must not call a helper\n{body}"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

/// Every supported target emits the unchecked read as one direct payload load.
#[test]
fn test_array_get_bounds_emits_on_all_supported_targets() {
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let dir = make_cli_test_dir("elephc_array_get_bounds_target");
        let path = dir.join("main.php");
        fs::write(&path, BOUNDED_READ_SUM).unwrap();
        let mut command = elephc_cli_command(&dir);
        command.args(["--emit-asm", "--target", target]);
        if target.starts_with("ios-") {
            command.args(["--emit", "staticlib"]);
        }
        let output = command.arg(&path).output().unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = fs::read_to_string(path.with_extension("s")).unwrap();
        let body = asm
            .split("@block name=while.body")
            .find(|body| body.contains("op=array_get_unchecked"))
            .unwrap_or_else(|| panic!("{target}: missing unchecked read body\n{asm}"));
        assert!(
            !body.contains("__rt_warn_undefined_array_key") && !body.contains("__rt_hash_get"),
            "{target}: the bounded read kept a guard path\n{body}"
        );
        let direct_load = if target.contains("x86_64") {
            "mov rax, QWORD PTR [r11 + rax*8 + 24]"
        } else {
            "ldr x0, [x10, x0, lsl #3]"
        };
        assert!(
            body.contains(direct_load),
            "{target}: missing the direct payload load\n{body}"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}

/// An inclusive bound can read one past the end, so it keeps the warning and the null fallback.
#[test]
fn test_array_get_bounds_keeps_inclusive_bound_warning() {
    let out = compile_and_run_capture(
        r#"<?php
$data = [1, 2, 3];
$sum = 0;
for ($i = 0; $i <= count($data); $i++) {
    $sum += $data[$i];
}
echo $sum, "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "6\n");
    assert!(
        out.stderr.contains("Warning: Undefined array key 3"),
        "{}",
        out.stderr
    );
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
