//! Purpose:
//! Executes late machine-layer regressions: constant-multiply strength reduction, fused
//! compare-and-branch lowering, redundant move/jump cleanup, and cold overflow layout.
//!
//! Called from:
//! - `cargo test --test codegen_tests machine_peephole`.
//!
//! Key details:
//! - Runtime argc keeps every tested value unknown to the AST and EIR folders.
//! - Behavior must match in both optimizer modes and both allocation strategies; assembly
//!   shape is checked on every supported target through `--emit-asm`.

use super::*;

/// Owns the source and outputs of a CLI fixture, cleaning them even on a failed assertion.
struct Fixture(std::path::PathBuf);

impl Fixture {
    /// Writes source into a fresh test directory.
    fn new(source: &str) -> Self {
        let fixture = Self(make_cli_test_dir("elephc_machine_peepholes"));
        fs::write(fixture.0.join("main.php"), source).unwrap();
        fixture
    }

    /// Compiles with explicit optimization and register-allocation policies.
    fn compile(&self, optimized: bool, linear: bool, extra: &[&str]) {
        let output = elephc_cli_command(&self.0)
            .args(["--target", target().as_str()])
            .arg(if optimized { "--ir-opt=on" } else { "--ir-opt=off" })
            .arg(if linear { "--regalloc=linear" } else { "--regalloc=stack" })
            .args(extra)
            .arg(self.0.join("main.php"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }

    /// Emits assembly for one explicit target and returns it.
    fn assembly(&self, target_name: &str) -> String {
        let mut options = vec!["--emit-asm", "--target", target_name];
        if target_name.starts_with("ios-") {
            options.extend(["--emit", "staticlib"]);
        }
        let output = elephc_cli_command(&self.0)
            .args(&options)
            .arg(self.0.join("main.php"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        fs::read_to_string(self.0.join("main.s")).unwrap()
    }

    /// Runs the executable with `extra` additional arguments.
    fn run(&self, extra: usize) -> String {
        let args = vec!["x"; extra];
        let output = run_binary_with_args(&self.0.join("main"), &self.0, &args);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    /// Compiles in every optimizer/allocator mode and checks the output for each argc.
    fn assert_all_modes(&self, expected: impl Fn(i64) -> String) {
        for optimized in [false, true] {
            for linear in [false, true] {
                self.compile(optimized, linear, &[]);
                for extra in 0..3 {
                    let argc = extra as i64 + 1;
                    assert_eq!(self.run(extra), expected(argc), "optimized={optimized} linear={linear} argc={argc}");
                }
            }
        }
    }
}

impl Drop for Fixture {
    /// Removes only this test's isolated source and compilation outputs.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Returns the text of one emitted function, between its `@fn` and `@endfn` markers.
fn function_text<'a>(asm: &'a str, name: &str) -> &'a str {
    let start = asm.find(&format!("@fn name={name} ")).expect("function start marker");
    let end = start + asm[start..].find(&format!("@endfn name={name}")).expect("function end marker");
    &asm[start..end]
}

/// Constants covering every strength-reduction shape plus ones that keep the multiplier.
const MULTIPLIERS: [i64; 22] = [2, 3, 5, 8, 9, 10, 12, 16, 17, 31, 33, 63, 96, 4096, -1, -2, -7, -8, -31, -33, 11, 1000];

/// Strength-reduced constant multiplies equal real multiplication for signed operands.
#[test]
fn test_machine_peephole_strength_reduction_matches_multiply() {
    let mut source = String::from(
        "<?php\nfunction products(int $seed): string {\n    $x = ($seed & 0xFFFF) - 40000;\n    $out = '';\n",
    );
    for constant in MULTIPLIERS {
        source.push_str(&format!("    $out .= ($x * {constant}) . ',';\n"));
    }
    source.push_str("    return $out;\n}\necho products($argc * 21845), \"\\n\";\n");
    let fixture = Fixture::new(&source);
    fixture.assert_all_modes(|argc| {
        let x = ((argc * 21845) & 0xFFFF) - 40000;
        let mut line: String = MULTIPLIERS.iter().map(|c| format!("{},", x * c)).collect();
        line.push('\n');
        line
    });
}

/// Every integer predicate branches correctly on fused flags, in both edge orders.
#[test]
fn test_machine_peephole_fused_compare_branches_all_predicates() {
    let source = r#"<?php
function count_hits(int $n): string {
    $out = '';
    foreach (['lt', 'le', 'gt', 'ge', 'eq', 'ne'] as $kind) {
        $hits = 0;
        for ($i = -3; $i <= 3; $i++) {
            if ($kind === 'lt') { if ($i < $n) { $hits++; } }
            if ($kind === 'le') { if ($i <= $n) { $hits++; } else { $hits += 10; } }
            if ($kind === 'gt') { if (!($i > $n)) { $hits++; } }
            if ($kind === 'ge') { if ($i >= $n) { $hits++; } }
            if ($kind === 'eq') { if ($i == $n) { $hits++; } else { $hits += 100; } }
            if ($kind === 'ne') { if ($i != $n) { $hits++; } }
        }
        $out .= $kind . '=' . $hits . ' ';
    }
    $down = 0;
    for ($j = $n + 5; $j > $n; $j--) { $down++; }
    return $out . 'down=' . $down;
}
echo count_hits($argc - 2), "\n";
"#;
    let fixture = Fixture::new(source);
    fixture.assert_all_modes(|argc| {
        let n = argc - 2;
        let range = -3..=3i64;
        let lt = range.clone().filter(|i| *i < n).count();
        let le = range.clone().map(|i| if i <= n { 1 } else { 10 }).sum::<i64>();
        let gt = range.clone().filter(|i| !(*i > n)).count();
        let ge = range.clone().filter(|i| *i >= n).count();
        let eq = range.clone().map(|i| if i == n { 1 } else { 100 }).sum::<i64>();
        let ne = range.filter(|i| *i != n).count();
        format!("lt={lt} le={le} gt={gt} ge={ge} eq={eq} ne={ne} down=5\n")
    });
}

/// Loop-carried scalars of every kind survive direct block-argument moves, including swaps.
#[test]
fn test_machine_peephole_block_argument_moves_and_swaps() {
    let source = r#"<?php
function walk(int $n): string {
    $a = 1;
    $b = 2;
    $f = 0.5;
    $flag = false;
    for ($i = 0; $i < $n; $i++) {
        $t = $a;
        $a = $b;
        $b = $t + $i;
        $f = $f * 2.0;
        $flag = !$flag;
    }
    return $a . ' ' . $b . ' ' . $f . ' ' . ($flag ? 'y' : 'n');
}
echo walk($argc + 3), "\n";
"#;
    let fixture = Fixture::new(source);
    fixture.assert_all_modes(|argc| {
        let (mut a, mut b, mut f, mut flag) = (1i64, 2i64, 0.5f64, false);
        for i in 0..argc + 3 {
            let t = a;
            a = b;
            b = t + i;
            f *= 2.0;
            flag = !flag;
        }
        format!("{a} {b} {f} {}\n", if flag { 'y' } else { 'n' })
    });
}

/// The checked overflow path still promotes like PHP after moving out of line.
#[test]
fn test_machine_peephole_cold_overflow_path_still_promotes() {
    let source = r#"<?php
function probe(int $base): void {
    for ($i = 0; $i < 2; $i++) {
        var_dump($base * 3 + $i);
        $n = $base + $i;
        var_dump($n);
    }
}
probe($argc);
probe(PHP_INT_MAX - $argc);
"#;
    let fixture = Fixture::new(source);
    fixture.assert_all_modes(|argc| {
        let big = i64::MAX - argc;
        let promoted = php_float(big as f64 * 3.0);
        format!(
            "int({})\nint({argc})\nint({})\nint({})\nfloat({promoted})\nint({big})\nfloat({promoted})\nint({})\n",
            3 * argc,
            3 * argc + 1,
            argc + 1,
            big + 1
        )
    });
}

/// Formats a float like PHP's `var_dump` for the large magnitudes used above.
fn php_float(value: f64) -> String {
    let text = format!("{value:E}");
    let (mantissa, exponent) = text.split_once('E').unwrap();
    let mantissa = if mantissa.contains('.') { mantissa.to_string() } else { format!("{mantissa}.0") };
    format!("{mantissa}E+{exponent}")
}

/// The example runs in every mode and has the documented late machine shape on every target.
#[test]
fn test_machine_peephole_example_and_target_assembly() {
    let fixture = Fixture::new(include_str!("../../../examples/machine-peepholes/main.php"));
    fixture.assert_all_modes(|argc| {
        let mut acc = 0i64;
        for i in 0..1000 * argc {
            acc = (acc * 31 + i) & 0xFF_FFFF;
        }
        let x = (1234 * argc) & 0xFFFF;
        format!("hash={acc}\nscaled={} {} {} {}\n", x * 31, x * 10, x * -8, x * 11)
    });
    for target_name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let asm = fixture.assembly(target_name);
        let arm = target_name != "linux-x86_64";

        let scaled = function_text(&asm, "scaled");
        let shift_sub = if arm { ["lsl x10, x0, #5", "sub x0, x10, x0"] } else { ["shl rax, 5", "sub rax, r10"] };
        for instruction in shift_sub {
            assert!(scaled.contains(instruction), "{target_name}: {scaled}");
        }
        assert!(scaled.contains(if arm { "neg x0, x0, lsl #3" } else { "neg rax" }), "{target_name}: {scaled}");
        assert!(scaled.contains(if arm { "mul x0, " } else { "imul rax, 11" }), "{target_name}: {scaled}");

        let hash = function_text(&asm, "rolling_hash");
        // The loop test branches on flags: no boolean is materialized for it.
        assert!(!hash.contains(if arm { "cset " } else { "setl " }), "{target_name}: {hash}");
        assert!(hash.contains(if arm { "b.ge " } else { "jge " }), "{target_name}: {hash}");
        // The overflow path enters the cold tail with one conditional branch and sits after
        // every block, so the loop body's in-range path falls straight through.
        let cold = hash.find("cold paths kept out of the hot fallthrough").expect("cold tail");
        let overflow = hash.find("checked_numeric_chain_overflow").expect("overflow branch");
        assert!(overflow < cold, "{target_name}: {hash}");
        assert!(hash[overflow..cold].lines().all(|line| !line.ends_with(':') || !line.contains("overflow")), "{target_name}: {hash}");
        assert!(hash[cold..].contains("__rt_php_float_to_int"), "{target_name}: {hash}");
        // The out-of-line code keeps its PHP source location for debug line info.
        assert!(hash[cold..].contains("@src line=14 "), "{target_name}: {hash}");
        // No self-moves and no jump straight to the following label survive the peepholes.
        let lines: Vec<&str> = hash.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
        for (index, line) in lines.iter().enumerate() {
            if let Some((dst, src)) = line.strip_prefix("mov ").and_then(|rest| rest.split_once(", ")) {
                assert!(dst != src || dst.starts_with('w') || dst.starts_with('e'), "{target_name}: {line}");
            }
            let jump = if arm { line.strip_prefix("b ") } else { line.strip_prefix("jmp ") };
            if let (Some(label), Some(next)) = (jump, lines.get(index + 1)) {
                assert_ne!(*next, format!("{label}:"), "{target_name}: redundant jump in {hash}");
            }
        }
    }
}
