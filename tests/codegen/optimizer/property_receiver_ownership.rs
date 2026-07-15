//! Purpose:
//! Verifies property-receiver ownership cleanup with EIR optimization enabled and disabled.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::property_receiver_ownership`.
//!
//! Key details:
//! - A concrete object loaded from a Mixed frame slot carries an owned unbox retain
//!   that must be released after the property result has been stabilized.

use super::*;

/// Compiles and runs one heap-debug fixture with the requested EIR optimizer mode.
fn run_property_receiver_fixture(source: &str, ir_opt: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_property_receiver_ownership");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write property receiver fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("failed to compile property receiver fixture");
    assert!(
        compile.status.success(),
        "fixture compilation failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run property receiver fixture");
    assert!(
        output.status.success(),
        "fixture execution failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    let _ = fs::remove_dir_all(&dir);
    (stdout, stderr)
}

/// Confirms issue #540 root cause 4 stays heap-clean in both EIR optimizer modes.
#[test]
fn test_property_receiver_cleanup_with_optimizer_on_and_off() {
    let source = r#"<?php
class Box {
    public string $named = "named";
    public string $dynamic = "dynamic";
    public string $safe = "safe";
    public int $count = 1;
}

$box = null;
$named = "";
$dynamic = null;
$safe = null;
$property = "dynamic";
for ($i = 0; $i < 40; $i++) {
    $box = new Box();
    $named = $box->named;
    $dynamic = $box->{$property};
    $safe = $box?->safe;
}
echo $named;
echo ":";
echo $dynamic;
echo ":";
echo $safe;
"#;

    for ir_opt in [false, true] {
        let (stdout, stderr) = run_property_receiver_fixture(source, ir_opt);
        assert_eq!(
            stdout, "named:dynamic:safe",
            "unexpected stdout with ir_opt={ir_opt}"
        );
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected a clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}
