//! Purpose:
//! Verifies inline container/property read cleanup with EIR optimization enabled and disabled.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::read_result_cleanup`.
//!
//! Key details:
//! - Scalar consumers and independent builtin results must balance read stabilizations.
//! - Constructor argument conversions and proven-independent call results release caller temporaries.

use super::*;

/// Compiles and runs the combined read-result ownership fixture in one optimizer mode.
fn run_read_result_cleanup_fixture(source: &str, ir_opt: bool) -> (String, String) {
    let dir = make_cli_test_dir("elephc_read_result_cleanup");
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write read-result cleanup fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("failed to compile read-result cleanup fixture");
    assert!(
        compile.status.success(),
        "fixture compilation failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run read-result cleanup fixture");
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

/// Keeps all three residual #540 ownership paths heap-clean in both optimizer modes.
#[test]
fn test_read_result_cleanup_with_optimizer_on_and_off() {
    let source = r#"<?php
class OptimizedResidualMetadata {
    public function __construct(public string $id, public string $title) {}
}
class OptimizedResidualPair {
    public function __construct(public string $left, public string $right) {}
}
class OptimizedResidualJoiner {
    public function join(string $left, string $right): string {
        $parts = [$left, $right];
        return implode('', $parts);
    }
}
function loadOptimizedResidualMetadata(int $n): ?OptimizedResidualMetadata {
    return new OptimizedResidualMetadata("id" . $n, "title");
}
function maybeOptimizedResidualPair(): ?OptimizedResidualPair {
    return new OptimizedResidualPair("left", "right");
}

$arraySum = 0;
$metadataSum = 0;
$joinedSum = 0;
$joiner = new OptimizedResidualJoiner();
for ($i = 0; $i < 40; $i++) {
    $fields = ["bench", "php", "1720000000", "0"];
    $arraySum += (int) $fields[3];
    $arraySum += (int) $fields[2];
    $arraySum += strlen(rawurldecode((string) $fields[0]));

    $metadata = loadOptimizedResidualMetadata($i);
    if ($metadata instanceof OptimizedResidualMetadata) {
        $metadata = new OptimizedResidualMetadata($metadata->id, $metadata->title);
        $metadataSum += strlen($metadata->id) + strlen($metadata->title);
    }

    $pair = maybeOptimizedResidualPair();
    if ($pair instanceof OptimizedResidualPair) {
        $pair = new OptimizedResidualPair("left", "right");
        $joined = $joiner->join($pair->left, $pair->right);
        $joinedSum += strlen($joined);
    }
}
echo $arraySum . '|' . $metadataSum . '|' . $joinedSum;
"#;

    for ir_opt in [false, true] {
        let (stdout, stderr) = run_read_result_cleanup_fixture(source, ir_opt);
        assert_eq!(
            stdout, "68800000200|350|360",
            "unexpected stdout with ir_opt={ir_opt}"
        );
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected a clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}
