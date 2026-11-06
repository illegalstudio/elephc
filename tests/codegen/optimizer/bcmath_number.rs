//! Purpose:
//! Checks internal object operators with EIR optimization enabled and disabled.
//!
//! Called from:
//! - The codegen optimizer integration group.
//!
//! Key details:
//! - Unused arithmetic can allocate, print through operands, or throw and must survive DCE.

use super::*;

/// Removes this fixture's temporary directory on success and assertion failures.
struct FixtureDirectory(std::path::PathBuf);

impl Drop for FixtureDirectory {
    /// Retires only the exact directory created for this fixture.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Preserves source evaluation order and catchable errors when the arithmetic result is unused.
#[test]
fn test_bcmath_number_optimizer_effects() {
    let source = r#"<?php
function left(): BcMath\Number { echo 'L'; return new BcMath\Number('1.20'); }
function right(): int { echo 'R'; return 0; }
try { left() / right(); }
catch (DivisionByZeroError $e) { echo ':', $e->getMessage(), '|'; }
try { new BcMath\Number('1') + 'invalid'; }
catch (ValueError $e) { echo $e->getMessage(), '|'; }
$zero = new BcMath\Number('0.00');
echo $zero ? 'wrong' : 'zero';
try { $zero + 'invalid'; }
catch (ValueError $e) { echo '|variable'; }
function unused_number_add(mixed $a, mixed $b): mixed { return $a + $b; }
try { unused_number_add($zero, []); }
catch (TypeError $e) { echo '|mixed-call'; }
"#;
    for mode in ["--ir-opt=on", "--ir-opt=off"] {
        let dir = FixtureDirectory(make_cli_test_dir("elephc_bcmath_number_optimizer"));
        let php = dir.0.join("main.php");
        fs::write(&php, source).expect("write Number optimizer fixture");
        let compile = elephc_cli_command(&dir.0).arg(mode).arg(&php).output().expect("compile Number fixture");
        assert!(compile.status.success(), "{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        let output = Command::new(dir.0.join("main")).output().expect("run Number fixture");
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "LR:Division by zero|Right string operand cannot be converted to BcMath\\Number|zero|variable|mixed-call", "{mode}");
    }
}

/// Every supported target emits the same typed decimal bridge and object operator calls.
#[test]
fn test_bcmath_number_supported_target_emitters() {
    let dir = FixtureDirectory(make_cli_test_dir("elephc_bcmath_number_targets"));
    let php = dir.0.join("main.php");
    fs::write(&php, r#"<?php
use BcMath\Number as Decimal;
$n = new Decimal('1.20');
$n += 2;
$pair = $n->divmod('.3');
echo $pair[1]->round(mode: RoundingMode::HalfEven)->value;
var_dump(empty($n), $n <=> 2);
$text = str_repeat('1', $argc);
echo $text % 2;
switch ($n) { case new Decimal('3.200'): echo 'equal'; }
function decimal_switch(mixed $label): void {
    switch (3) { case $label: echo 'integer'; break; default: break; }
    switch (3.0) { case $label: echo 'float'; break; default: break; }
    switch ($label) { case 8: break; case 3: echo 'mixed'; break; }
}
decimal_switch(new Decimal('3.00'));
var_dump(in_array($n, ['3.200']));
echo (new Decimal(1.5))->add(1.5)->value;
try { $n + INF; } catch (TypeError $e) { echo 'range'; }
"#).expect("write Number target fixture");
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let compile = elephc_cli_command(&dir.0).args(["--target", target, "--emit", "staticlib", "--emit-asm"])
            .arg(&php).output().expect("emit Number target assembly");
        assert!(compile.status.success(), "{target}: {}", String::from_utf8_lossy(&compile.stderr));
        let assembly = fs::read_to_string(dir.0.join("main.s")).expect("read Number target assembly");
        for symbol in ["elephc_bcmath_number_create", "elephc_bcmath_number_add", "elephc_bcmath_number_divmod", "elephc_bcmath_number_round", "__elephc_internal_object_compare"] {
            assert!(assembly.contains(symbol), "{target} missing {symbol}");
        }
    }
}

/// Force-enabled BCMath retains declarations for opaque eval source supplied after compilation.
#[test]
fn test_bcmath_number_opaque_eval_capability() {
    let dir = FixtureDirectory(make_cli_test_dir("elephc_bcmath_number_opaque_eval"));
    let php = dir.0.join("main.php");
    fs::write(&php, "<?php eval($argv[1]);").expect("write opaque eval fixture");
    let compile = elephc_cli_command(&dir.0).arg("--with-bcmath").arg(&php)
        .output().expect("compile opaque eval fixture");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = Command::new(dir.0.join("main"))
        .arg(r#"$n = new \BcMath\Number('1.20'); echo ($n + 2)->round(mode: \RoundingMode::HalfEven)->value, '|';
$n = new \BcMath\Number('2.50');
echo (new \BcMath\Number(1.5))->value, '|', $n->add(1.5)->value, '|', $n->compare(2.9), '|';
echo (new \BcMath\Number(false))->value, '|', (new \BcMath\Number(null))->value, '|';
var_dump(in_array(new \BcMath\Number('1.20'), [new \BcMath\Number('1.200')]));
switch (new \BcMath\Number('1.20')) { case '1.200': echo 'switch|'; break; }
try { $n + INF; } catch (\TypeError $e) { echo 'operator|'; }
try { $n->add(INF); } catch (\ValueError $e) { echo 'method'; }"#)
        .output().expect("run opaque eval Number fixture");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3|1|3.50|1|0|0|bool(true)\nswitch|operator|method");
}
