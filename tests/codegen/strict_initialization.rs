//! Purpose:
//! Verifies initialized reads through the public CLI and real eval bridge.
//!
//! Called from:
//! - Focused codegen integration tests.
//!
//! Key details:
//! - Public admission, source policy and heap cleanup are exercised together.

use crate::support::*;

/// The CLI preserves initialized null and probes while rejecting missing ordinary reads.
#[test]
fn test_strict_initialization_public_cli_values_and_owners() {
    let source = r#"<?php declare(strict_types=0);
function optionalInitializedRead(bool $assign): void {
    if ($assign) { $value = null; }
    echo isset($value) ? 'P' : 'A', empty($value) ? 'E' : 'F', ':', $value ?? 'missing', '|';
    try { echo gettype($value); } catch (Error $error) { echo 'I'; }
    echo '|';
}
optionalInitializedRead(true); optionalInitializedRead(false);
function unsetFromLiteralEval(): void {
    $value = 'old'; eval('unset($value);');
    try { echo $value; } catch (Error $error) { echo 'I'; }
}
unsetFromLiteralEval();
$root = 'old';
function exposeInitializedRoot(): void { global $root; }
eval('unset($root);');
try { echo $root; } catch (Error $error) { echo 'R'; }
echo '|', '2' + 1;
"#;
    for optimization in ["--ir-opt=off", "--ir-opt=on"] {
        let output = StrictFixture::new(source).capture(&["--strict-initialization", "--php-only", optimization, "--heap-debug"]);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "AE:missing|NULL|AE:missing|I|IR|3");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert!(!stderr.contains("Undefined variable"), "{stderr}");
    }
    StrictFixture::new("<?php echo $uninitialized;").assert_error(&["--strict-initialization"],
        "--strict-initialization: variable $uninitialized is not initialized");
    let output = StrictFixture::new("<?php $value=1;unset($value);echo $value??'quiet';")
        .run(&["--strict-initialization", "--no-strict-initialization"]);
    assert_eq!(output, "quiet");
}

/// Runtime-provided source executes under the installed native initialization profile.
#[test]
fn test_strict_initialization_public_opaque_eval_values_and_owners() {
    let fragment = format!("{}{}",
        include_str!("../fixtures/strict/initialized_frame_reads.php").strip_prefix("<?php").unwrap(),
        include_str!("../fixtures/strict/initialized_recursive_getters.php").strip_prefix("<?php").unwrap());
    let fragment = fragment.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $fragment=$argc>0?'{fragment}':'';eval($fragment);");
    let output = StrictFixture::new(&source).capture(&["--strict-initialization", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout),
        "P:7:7|A:missing:I|NULL:fallback|AE:fallback|NULL|integer|3|0|2|ready|A:none|P:5|0:0|1:1|9|I|9|4|I|4|I|I|P|6:8|boolean:double:string:3|E:1|ready:2|right:2|text:2.5|other:1:1|value|");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    assert!(!stderr.contains("Undefined variable"), "{stderr}");
}

/// PHP-only entries retain initialization checks in extension-enabled tagless LFC includes.
#[test]
fn test_strict_initialization_public_lfc_and_example() {
    let fixture = StrictFixture::new("<?php require 'setting.lfc';");
    fixture.file("setting.lfc", "int $value=1;if($argc>0){unset($value);}try{echo $value;}catch(Error $error){echo 'L';}");
    let output = fixture.capture(&["--strict-initialization", "--php-only", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "L");
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", String::from_utf8_lossy(&output.stderr));
    let output = StrictFixture::new(include_str!("../../examples/strict-initialization/main.php"))
        .capture(&["--strict-initialization", "--heap-debug"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout),
        "Status: ready\nValue: ready\nStatus: not loaded\nThe setting must be initialized before reading it.\nNULL\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "{}", String::from_utf8_lossy(&output.stderr));
}
