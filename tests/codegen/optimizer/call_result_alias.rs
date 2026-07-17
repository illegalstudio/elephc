//! Purpose:
//! Integration coverage for call-result/argument alias summaries with EIR
//! optimization enabled and disabled.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::call_result_alias`.
//!
//! Key details:
//! - Fresh method and inlined function results release unrelated inline arrays.
//! - True parameter passthrough remains valid, COW-safe, and heap-clean.

use super::*;

/// Compiles and runs the alias-summary fixture in one EIR optimizer mode.
fn run_call_alias_fixture(ir_opt: bool) -> (String, String) {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "elephc_call_result_alias_runtime_{}_{}",
        std::process::id(),
        id
    ));
    fs::create_dir_all(&dir).expect("failed to create call-result alias fixture directory");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
final class OptimizedAliasScanner {
    public function scan(array $delimiters): array {
        $tokens = [];
        $count = count($delimiters);
        for ($i = 0; $i < $count; $i++) {
            $tokens[] = $delimiters[$i];
        }
        return $tokens;
    }
}
final class OptimizedAliasChooser {
    public function choose(array $discard, array $keep): array {
        return $keep;
    }
}
function optimizedAliasCopy(array $source): array {
    $copy = [];
    $copy[] = $source[0];
    return $copy;
}
$scanner = new OptimizedAliasScanner();
$chooser = new OptimizedAliasChooser();
for ($i = 0; $i < 40; $i++) {
    $tokens = $scanner->scan(['//', '#']);
    $chosen = $chooser->choose(['drop'], ['keep']);
    $functionResult = optimizedAliasCopy(['function']);
}
$tokenCopy = $tokens;
$tokenCopy[] = 'tail';
$chosenCopy = $chosen;
$chosenCopy[] = 'tail';
echo $tokens[0] . ':' . count($tokens) . ':' . count($tokenCopy);
echo '|' . $chosen[0] . ':' . count($chosen) . ':' . count($chosenCopy);
echo '|' . $functionResult[0];
"#,
    )
    .expect("failed to write call-result alias PHP fixture");
    let mode = if ir_opt { "--ir-opt=on" } else { "--ir-opt=off" };
    let compile = elephc_cli_command(&dir)
        .arg("--heap-debug")
        .arg(mode)
        .arg(&php_path)
        .output()
        .expect("failed to compile call-result alias fixture");
    assert!(
        compile.status.success(),
        "fixture compilation failed with ir_opt={ir_opt}: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new(dir.join("main"))
        .output()
        .expect("failed to run call-result alias fixture");
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

/// Verifies precise alias cleanup is behaviorally identical in both optimizer modes.
#[test]
fn test_call_result_alias_cleanup_with_optimizer_on_and_off() {
    for ir_opt in [false, true] {
        let (stdout, stderr) = run_call_alias_fixture(ir_opt);
        assert_eq!(stdout, "//:2:3|keep:1:2|function");
        assert!(
            stderr.contains("HEAP DEBUG: leak summary: clean"),
            "expected clean heap with ir_opt={ir_opt}, got: {stderr}"
        );
    }
}
