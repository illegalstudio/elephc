//! Purpose:
//! Verifies scalar indexed-array overwrites across unique, COW, growth, and reference shapes.
//!
//! Called from:
//! - `cargo test` through the codegen integration harness.
//!
//! Key details:
//! - Both optimizer modes must match PHP and finish with balanced heap ownership.
//! - Unique in-bounds writes may use the runtime fast path.
//! - Shared arrays must still split, while reference aliases must observe the same mutation.

use crate::support::*;

/// Scalar writes preserve PHP values while selecting fast and fallback ownership paths.
#[test]
fn test_scalar_array_set_fast_path_preserves_cow_growth_and_references() {
    let source = r#"<?php
$ints = [1, 2, 3];
$ints[1] = 9;
$floats = [1.5, 2.5];
$floats[0] = 3.75;
$bools = [true, false];
$bools[1] = true;
$source = [10.5, 20.5];
$copy = $source;
$copy[0] = 99.5;
$grow = [1.0];
$grow[1] = 2.0;
$refs = [4.0, 5.0];
$alias =& $refs;
$refs[1] = 8.0;
for ($i = 0; $i < 1000; $i++) {
    $floats[$i % 2] += 0.25;
}
echo implode(',', $ints), '|', implode(',', $floats), '|';
echo ($bools[0] ? '1' : '0'), ($bools[1] ? '1' : '0'), '|';
echo implode(',', $source), '|', implode(',', $copy), '|';
echo implode(',', $grow), '|', implode(',', $alias);
"#;
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = make_cli_test_dir("elephc_scalar_set_fast_path");
        let php = dir.join("main.php");
        fs::write(&php, source).expect("write scalar array mutation fixture");
        let compile = elephc_cli_command(&dir).args([mode, "--heap-debug"])
            .arg(&php).output().expect("compile scalar array mutation fixture");
        if !compile.status.success() {
            fs::remove_dir_all(&dir).expect("remove failed scalar array mutation fixture");
            panic!("{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        }
        let output = run_binary(&dir.join("main"), &dir);
        let oracle = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| {
            Command::new("php").arg("-n").arg(&php).output().expect("run PHP oracle")
        });
        fs::remove_dir_all(&dir).expect("remove scalar array mutation fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, b"1,9,3|128.75,127.5|11|10.5,20.5|99.5,20.5|1,2|4,8");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        if let Some(php) = oracle {
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "PHP mismatch with {mode}");
        }
    }
}
