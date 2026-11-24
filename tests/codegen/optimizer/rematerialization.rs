//! Purpose:
//! Executes constant rematerialization and immediate operand regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core rematerialization`.
//!
//! Key details:
//! - Runtime argc prevents constant folding of tested operations.
//! - Both optimizer modes and both allocation strategies must agree.

use super::*;

/// Owns the source and outputs of a CLI fixture, cleaning them even on a failed assertion.
struct Fixture(std::path::PathBuf);

impl Fixture {
    /// Writes source into a fresh test directory.
    fn new(source: &str) -> Self {
        let fixture = Self(make_cli_test_dir("elephc_rematerialization"));
        fs::write(fixture.0.join("main.php"), source).unwrap();
        fixture
    }

    /// Compiles with explicit optimization and register-allocation policies.
    fn compile(&self, optimized: bool, linear: bool, extra: &[&str]) -> std::process::Output {
        let output = elephc_cli_command(&self.0)
            .args(["--target", target().as_str()])
            .arg(if optimized { "--ir-opt=on" } else { "--ir-opt=off" })
            .arg(if linear { "--regalloc=linear" } else { "--regalloc=stack" })
            .args(extra).arg(self.0.join("main.php")).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        output
    }

    /// Runs the executable with zero or one additional argument.
    fn run(&self, extra: bool) -> String {
        let args: &[&str] = if extra { &["extra"] } else { &[] };
        let output = run_binary_with_args(&self.0.join("main"), &self.0, args);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Fixture {
    /// Removes only this test's isolated source and compilation outputs.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Wide masks, signed boundaries, and reversed comparisons preserve every integer bit.
#[test]
fn test_rematerialization_integer_boundaries_all_modes() {
    let constants = [0i64, 1, -1, 4095, 4096, 4097, 4095 << 12, -4096,
        i32::MIN as i64, i32::MAX as i64, 1 << 31, 0xffff_ffff,
        0x00ff_00ff_00ff_00ff, 0x1234_5678_9abc_def0, i64::MIN, i64::MAX];
    let mut source = String::from("<?php\n$x = (int) ($argc - 4);\n");
    for n in constants {
        let literal = if n == i64::MIN { "PHP_INT_MIN".to_owned() } else { n.to_string() };
        source.push_str(&format!(
            "echo ($x & {literal}), ':', ($x | {literal}), ':', ($x ^ {literal}), ':', \
             ({literal} < $x ? 1 : 0), ':', ($x <= {literal} ? 1 : 0), ':', \
             ({literal} > $x ? 1 : 0), ':', ($x >= {literal} ? 1 : 0), ':', \
             ({literal} == $x ? 1 : 0), ':', ($x != {literal} ? 1 : 0), \"\\n\";\n"));
    }
    let fixture = Fixture::new(&source);
    for optimized in [false, true] {
        for linear in [false, true] {
            fixture.compile(optimized, linear, &[]);
            for extra in [false, true] {
                let x = if extra { -2 } else { -3 };
                let expected: String = constants.iter().map(|&n| format!("{}:{}:{}:{}:{}:{}:{}:{}:{}\n",
                    x & n, x | n, x ^ n, i32::from(n < x), i32::from(x <= n),
                    i32::from(n > x), i32::from(x >= n), i32::from(n == x), i32::from(x != n))).collect();
                assert_eq!(fixture.run(extra), expected, "optimized={optimized}, linear={linear}");
            }
        }
    }
}

/// Immediate arithmetic retains PHP's checked overflow conversion and operand order.
#[test]
fn test_rematerialization_arithmetic_and_overflow() {
    let fixture = Fixture::new(r#"<?php
$x = (int) ($argc - 4);
echo (int) ($x + 4096), ':', (int) (4096 + $x), ':', (int) ($x - -4096), ':', (int) (4096 - $x), ':', (int) ($x * 3), "\n";
$maximum = (int) (PHP_INT_MAX - ($argc - 1));
$minimum = (int) (PHP_INT_MIN + ($argc - 1));
echo (int) ($maximum + 1), ':', (int) (1 + $maximum), ':', (int) ($minimum - 1), ':', (int) ($minimum * -1), "\n";
"#);
    for optimized in [false, true] {
        for linear in [false, true] {
            fixture.compile(optimized, linear, &[]);
            assert_eq!(fixture.run(false), "4093:4093:4093:4099:-9\n-9223372036854775808:-9223372036854775808:-9223372036854775808:-9223372036854775808\n");
        }
    }
}

/// Register arithmetic takes the single-branch fast path or preserves overflow conversion.
#[test]
fn test_rematerialization_checked_register_overflow_and_sum() {
    let fixture = Fixture::new(r#"<?php
$sum = 0;
for ($i = 0; $i < ($argc + 31); $i++) { $sum = (int) ($sum + $i); }
$negative = (int) (-$argc);
echo $sum, ':', (int) (PHP_INT_MAX + $argc), ':', (int) (PHP_INT_MIN - $argc), ':', (int) (PHP_INT_MIN * $negative), "\n";
"#);
    for optimized in [false, true] {
        for linear in [false, true] {
            fixture.compile(optimized, linear, &[]);
            assert_eq!(fixture.run(false), "496:-9223372036854775808:-9223372036854775808:-9223372036854775808\n");
        }
    }
}

/// Constants survive loop edges, calls, branches, exception handlers, and float operations.
#[test]
fn test_rematerialization_control_flow_and_scalar_kinds() {
    let fixture = Fixture::new(r#"<?php
function identity(float $x): float { return $x; }
$sum = 0;
for ($i = 0; $i < ($argc + 3); $i++) {
    $sum = (int) (($sum + 4096) & 65535);
    if (($i & 1) == 0) { $sum = (int) ($sum ^ 0x123456789abcdef0); }
}
echo $sum, ':', fdiv(1.0, identity(-0.0)) < 0.0 ? 'negzero' : 'bad', ':';
$float = identity(1.25);
echo $float + 2.5, ':', true ? 'true' : 'bad', ':';
$value = $argc > 1 ? null : 9;
echo $value === null ? 'null' : 'value', ':';
try { if ($argc > 0) { throw new Exception('ok'); } }
catch (Exception $e) { echo (int) ($argc + 4096); }
"#);
    for optimized in [false, true] {
        for linear in [false, true] {
            fixture.compile(optimized, linear, &[]);
            for extra in [false, true] {
                let argc = if extra { 2 } else { 1 };
                let mut sum = 0i64;
                for i in 0..argc + 3 {
                    sum = (sum + 4096) & 65535;
                    if i & 1 == 0 { sum ^= 0x1234_5678_9abc_def0; }
                }
                assert_eq!(fixture.run(extra), format!("{sum}:negzero:3.75:true:{}:{}",
                    if extra { "null" } else { "value" }, argc + 4096));
            }
        }
    }
}

/// The example demonstrates immediate loop bounds and masks on each supported emitter.
#[test]
fn test_rematerialization_example_and_target_assembly() {
    let fixture = Fixture::new(include_str!("../../../examples/constant-rematerialization/main.php"));
    for optimized in [false, true] {
        fixture.compile(optimized, true, &[]);
        assert_eq!(fixture.run(false), "samples=8; checksum=33884\n");
    }
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut options = vec!["--emit-asm", "--target", target];
        if target.starts_with("ios-") { options.extend(["--emit", "staticlib"]); }
        fixture.compile(true, true, &options);
        let asm = fs::read_to_string(fixture.0.join("main.s")).unwrap();
        let arm = target != "linux-x86_64";
        assert!(asm.contains(if arm { "#0xff" } else { "and rax, 255" }), "{target}: {asm}");
        assert!(asm.contains(if arm { "cmp x0, #8" } else { "cmp rax, 8" }), "{target}: {asm}");
    }
}
