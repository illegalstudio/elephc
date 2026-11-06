//! Purpose:
//! End-to-end and emitted-IR coverage for the concat scratch-state optimization
//! pass (`concat-reset`), verifying that statement-boundary `concat_reset`
//! instructions are removed or coalesced when dataflow proves `_concat_off` is
//! already at the frame base or post-dominated by a later reset across
//! concat-inert operations, and preserved across calls, output, destructors,
//! closures, exceptions, `eval`, and string-producing loops.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::concat_reset`
//!
//! Key details:
//! - Compares `--ir-opt=on` and `--ir-opt=off` on both textual EIR (`--emit-ir`)
//!   and native execution output using runtime-dependent inputs (`$argc`).

use super::*;

/// Owns one isolated fixture directory, cleaning it up on drop.
struct Fixture(std::path::PathBuf);

impl Fixture {
    /// Writes the PHP source into a fresh fixture directory.
    fn new(source: &str) -> Self {
        let fixture = Self(make_cli_test_dir("elephc_concat_reset_opt"));
        fs::write(fixture.0.join("main.php"), source).expect("write concat_reset fixture");
        fixture
    }

    /// Compiles the fixture with an explicit optimizer mode and extra CLI flags.
    fn compile(&self, optimized: bool, options: &[&str]) -> std::process::Output {
        let mut command = elephc_cli_command(&self.0);
        command.args(options);
        command.arg(if optimized {
            "--ir-opt=on"
        } else {
            "--ir-opt=off"
        });
        let output = command
            .arg(self.0.join("main.php"))
            .output()
            .expect("compile concat_reset fixture");
        assert!(
            output.status.success(),
            "compile failed (optimized={optimized}, options={options:?}):\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    /// Runs the compiled fixture with optional command-line arguments.
    fn run(&self, args: &[&str]) -> String {
        let output = run_binary_with_args(&self.0.join("main"), &self.0, args);
        assert!(
            output.status.success(),
            "run failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 output")
    }

    /// Returns the textual EIR for the named function under the given optimizer mode.
    fn function_ir(&self, optimized: bool, name: &str) -> String {
        let text = String::from_utf8(self.compile(optimized, &["--emit-ir"]).stdout)
            .expect("UTF-8 textual EIR");
        text.split_once(&format!("  function {name}("))
            .expect("function present in textual EIR")
            .1
            .split("\n  function ")
            .next()
            .unwrap()
            .to_owned()
    }

    /// Compiles and runs the fixture with `--ir-opt=on` and `--ir-opt=off`,
    /// asserting identical stdout against `expected`.
    fn assert_equiv(&self, args: &[&str], expected: &str) {
        self.compile(false, &[]);
        let plain = self.run(args);
        self.compile(true, &[]);
        let optimized = self.run(args);
        assert_eq!(optimized, plain, "optimizer changed observable output");
        assert_eq!(optimized, expected, "output did not match expected PHP output");
    }
}

impl Drop for Fixture {
    /// Removes the temporary fixture directory after the test finishes.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Counts `concat_reset` instruction lines in a textual EIR slice.
fn count_ir_concat_resets(ir: &str) -> usize {
    ir.lines()
        .filter(|line| line.trim_start().starts_with("concat_reset"))
        .count()
}

/// Arithmetic-only `for` and `while` loops inside a pure scalar function eliminate
/// every `concat_reset` instruction across entry, loop headers, loop bodies, and exit.
#[test]
fn test_concat_reset_eliminated_in_arithmetic_loops() {
    let source = r#"<?php
function sum_squares(int $n): int {
    $acc = 0;
    for ($i = 1; $i <= $n; $i++) {
        $sq = $i * $i;
        $acc = $acc + $sq;
    }
    $k = $n;
    while ($k > 0) {
        $acc = $acc + $k;
        $k = $k - 1;
    }
    return $acc;
}
echo sum_squares($argc + 4), "\n";
"#;
    let fixture = Fixture::new(source);
    let plain_ir = fixture.function_ir(false, "sum_squares");
    let opt_ir = fixture.function_ir(true, "sum_squares");
    assert!(
        count_ir_concat_resets(&plain_ir) >= 6,
        "expected unoptimized function to have statement-boundary concat_resets:\n{plain_ir}"
    );
    assert_eq!(
        count_ir_concat_resets(&opt_ir),
        0,
        "expected all concat_resets to be eliminated in arithmetic-only function:\n{opt_ir}"
    );
    fixture.assert_equiv(&[], "70\n");
    fixture.assert_equiv(&["extra"], "112\n");
}

/// A string concatenation before an arithmetic loop keeps a single `concat_reset`
/// after the string statement to reclaim the temporary scratch buffer, and removes
/// all `concat_reset` instructions inside the subsequent arithmetic loop.
#[test]
fn test_concat_reset_kept_once_after_concat_then_eliminated_in_arith_loop() {
    let source = r#"<?php
function build_and_count(int $n): string {
    $prefix = "n=" . $n;
    $total = 0;
    for ($i = 0; $i < $n; $i++) {
        $step = ($i * 3) + 1;
        $total = $total + $step;
    }
    return $prefix . ":" . $total;
}
echo build_and_count($argc + 3), "\n";
"#;
    let fixture = Fixture::new(source);
    let opt_ir = fixture.function_ir(true, "build_and_count");
    assert_eq!(
        count_ir_concat_resets(&opt_ir),
        1,
        "expected exactly one concat_reset after initial concat:\n{opt_ir}"
    );
    fixture.assert_equiv(&[], "n=4:22\n");
}

/// Loops that allocate scratch strings on each iteration coalesce redundant
/// scalar-statement resets inside the loop down to a single `concat_reset` per
/// iteration so the 64 KiB scratch arena is rewound once per iteration without
/// overflowing into heap allocations.
#[test]
fn test_concat_reset_preserved_in_string_concat_loop() {
    let source = r#"<?php
function churn_strings(int $n): int {
    $len = 0;
    for ($i = 0; $i < $n; $i++) {
        $s = "item-" . $i . "-pad";
        if ($i === $n - 1) {
            $len = strlen($s);
        }
    }
    return $len;
}
echo churn_strings($argc + 4999), "\n";
"#;
    let fixture = Fixture::new(source);
    let plain_ir = fixture.function_ir(false, "churn_strings");
    let opt_ir = fixture.function_ir(true, "churn_strings");
    assert!(
        count_ir_concat_resets(&plain_ir) >= 4,
        "expected multiple statement-boundary concat_resets before optimization:\n{plain_ir}"
    );
    assert_eq!(
        count_ir_concat_resets(&opt_ir),
        1,
        "expected redundant in-loop concat_resets to coalesce to exactly 1 per iteration:\n{opt_ir}"
    );
    fixture.assert_equiv(&[], "13\n");
}

/// By-value `foreach` iteration over a concrete `int[]` array preserves clean
/// concat state and eliminates every `concat_reset` inside the loop body.
#[test]
fn test_concat_reset_eliminated_in_concrete_array_foreach_loop() {
    let source = r#"<?php
function sum_items(int $n): int {
    $items = [$n, $n + 1, $n + 2];
    $sum = 0;
    foreach ($items as $k => $v) {
        $weighted = $v * ($k + 1);
        $sum = $sum + $weighted;
    }
    return $sum;
}
echo sum_items($argc + 1), "\n";
"#;
    let fixture = Fixture::new(source);
    let plain_ir = fixture.function_ir(false, "sum_items");
    let opt_ir = fixture.function_ir(true, "sum_items");
    assert!(
        count_ir_concat_resets(&plain_ir) >= 4,
        "expected unoptimized foreach to contain statement-boundary concat_resets:\n{plain_ir}"
    );
    assert_eq!(
        count_ir_concat_resets(&opt_ir),
        0,
        "expected all concat_resets to be eliminated in concrete int[] foreach:\n{opt_ir}"
    );
    fixture.assert_equiv(&[], "20\n");
}

/// Preserves concat scratch correctness across user calls, output (`echo`),
/// `try`/`catch` exception handlers, uncaught propagating exceptions, object
/// destructors, and closures capturing objects with destructors, as well as `eval`.
#[test]
fn test_concat_reset_preserves_calls_output_exceptions_destructors_closures_and_eval() {
    let source = r#"<?php
class ScratchProbe {
    public string $tag;
    public function __construct(string $tag) {
        $this->tag = $tag;
    }
    public function __destruct() {
        $msg = "[dtor:" . $this->tag . "]";
        echo $msg;
    }
}

function throw_unhandled(int $x): string {
    $prefix = "throw=" . $x;
    if ($x > 0) {
        throw new Exception($prefix . ":boom");
    }
    return $prefix;
}

function run_complex(int $n): string {
    $a = "start:" . $n;
    echo $a, "|";

    $probe = new ScratchProbe("p" . $n);
    $probe = new ScratchProbe("q" . $n);

    $cap = new ScratchProbe("cap" . $n);
    $fn = function (int $v) use ($cap): string {
        return $cap->tag . ":" . $v;
    };
    echo $fn($n), "|";
    $fn = function (int $v): string {
        return "plain:" . $v;
    };
    echo $fn($n), "|";

    try {
        echo throw_unhandled($n);
    } catch (Exception $e) {
        echo $e->getMessage(), "|";
    }

    $eval_out = eval('return "ev:" . (3 + 4);');
    return $eval_out;
}

echo run_complex($argc + 2), "\n";
"#;
    let fixture = Fixture::new(source);
    fixture.assert_equiv(
        &[],
        "start:3|[dtor:p3]cap3:3|plain:3|throw=3:boom|[dtor:q3][dtor:cap3]ev:7\n",
    );
}

/// A branch that nearly fills the 64 KiB `_concat_buf` before executing a
/// warning-producing float-key conversion (`FToI` with `FloatKeyDiagnostic`) and
/// out-of-bounds array read (`ArrayGet`) must keep the `concat_reset` between the
/// large concat and the warning even when the sibling branch leaves scratch dirty
/// and the merge block starts with another `concat_reset`.
#[test]
fn test_concat_reset_preserves_reset_before_warning_after_near_full_scratch_branch() {
    let source = r#"<?php
function warn_after_near_full_scratch(int $n, string $pad): string {
    $arr = [10, 20];
    $big = "";
    $x = 0;
    $y = 0;
    if ($n > 0) {
        $big = $pad . ($n > 0 ? "x" : "y");
        $fk = $n + 0.5;
        $x = $arr[$fk];
        $y = $arr[$n + 99];
    } else {
        $big = "else:" . $n;
    }
    $after = "done:" . $n;
    return strlen($big) . ":" . $x . ":" . ($y ?? -1) . ":" . $after;
}
$pad = str_repeat("x", 65535);
echo warn_after_near_full_scratch($argc, $pad), "\n";
"#;
    let fixture = Fixture::new(source);
    let opt_ir = fixture.function_ir(true, "warn_after_near_full_scratch");
    assert!(
        count_ir_concat_resets(&opt_ir) >= 3,
        "expected concat_reset before FToI(FloatKeyDiagnostic), before ArrayGet, and at merge to survive:\n{opt_ir}"
    );
    fixture.assert_equiv(&[], "65536:20:-1:done:1\n");
}

/// Releasing an array constructed via `Op::HashSpread` (`[...$src]`) whose elements
/// have a `__destruct` method that leaves `_concat_off` at the end of `_concat_buf`
/// must not be treated as concat-inert; the `concat_reset` after `unset($copy)` must
/// survive so subsequent `__rt_itoa` writes do not advance past `_concat_buf` and its
/// 24-byte BSS padding into `_php_diagnostic_file_len` / `_php_diagnostic_file_ptr`.
#[test]
fn test_concat_reset_preserves_reset_after_hash_spread_destructor_release() {
    let source = r#"<?php
class SpreadScratchDtor {
    public string $pad;
    public int $n;

    public function __construct(string $pad, int $n) {
        $this->pad = $pad;
        $this->n = $n;
    }

    public function __destruct() {
        echo ($this->pad . ($this->n > 0 ? "Z" : "W"))[65535];
    }
}

function run_hash_spread_dtor(int $n, string $pad): string {
    $arr = [10, 20];
    $src = ["k" => new SpreadScratchDtor($pad, $n)];
    $copy = [...$src];
    unset($src);
    unset($copy);
    $idx = (int)((string)($n + 1000000000000000000))
        + (int)((string)($n + 2000000000000000000))
        - 3000000000000000000
        + 99;
    $missing = $arr[$idx];
    return ":" . $idx . ":" . ($missing ?? -1);
}

$pad = str_repeat("x", 65535);
echo run_hash_spread_dtor($argc, $pad), "\n";
"#;
    let fixture = Fixture::new(source);
    let opt_ir = fixture.function_ir(true, "run_hash_spread_dtor");
    assert!(
        count_ir_concat_resets(&opt_ir) >= 4,
        "expected concat_reset after unset($copy) to survive in optimized IR:\n{opt_ir}"
    );
    fixture.assert_equiv(&[], "Z:101:-1\n");
}
