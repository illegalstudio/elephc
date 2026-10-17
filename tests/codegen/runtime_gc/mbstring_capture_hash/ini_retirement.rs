//! Purpose:
//! Checks native string identity lifetime while Mixed wrappers are released and reused.
//!
//! Called from:
//! - The runtime GC codegen integration suite through the real compiler CLI.
//!
//! Key details:
//! - Both optimizer modes must retain string children, aliases and pending exceptions.
//! - Core INI directives make these fixtures executable with a minimal PHP oracle.

use crate::support::*;

/// Runs one identity-lifetime fixture with both optimizer modes and balanced heap ownership.
fn check_ini_retirement(source: &str, expected: &str) {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let directory = make_cli_test_dir("mbstring_mixed_ini_retirement");
        let php = directory.join("main.php");
        fs::write(&php, source).expect("write INI retirement fixture");
        let compiled = elephc_cli_command(&directory).args([mode, "--heap-debug"])
            .arg(&php).output().expect("compile INI retirement fixture");
        if !compiled.status.success() {
            fs::remove_dir_all(&directory).expect("remove failed INI retirement fixture");
            panic!("{mode}: {}", String::from_utf8_lossy(&compiled.stderr));
        }
        let output = run_binary(&directory.join("main"), &directory);
        let oracle = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| {
            Command::new("php").arg("-n").arg(&php).output()
        });
        fs::remove_dir_all(&directory).expect("remove INI retirement fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, expected.as_bytes(), "{mode}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        if let Some(oracle) = oracle {
            let oracle = oracle.expect("run PHP INI retirement oracle");
            assert!(oracle.status.success(), "{}", String::from_utf8_lossy(&oracle.stderr));
            assert_eq!(output.stdout, oracle.stdout, "PHP mismatch with {mode}");
        }
    }
}

/// A string child survives discarded Mixed aliases and allocator reuse with embedded NUL bytes.
#[test]
fn test_mbstring_mixed_ini_retirement_preserves_string_children() {
    check_ini_retirement(r#"<?php
function identityOwner(mixed $value): mixed { return $value; }
$original = ini_get('display_errors');
for ($iteration = 0; $iteration < 12; $iteration++) {
    ini_set('display_errors', "a\0b");
    $value = ini_get('display_errors');
    $boxed = identityOwner($value);
    $alias = $boxed;
    unset($value); unset($boxed);
    $numbers = [identityOwner(1), identityOwner(1.5), identityOwner(true), identityOwner(null)];
    unset($numbers);
    echo bin2hex((string)$alias), ';';
    unset($alias);
}
ini_set('display_errors', $original);
unset($original);
echo 'done';
"#, &("610062;".repeat(12) + "done"));
}

/// Empty and binary string identities remain distinct through nested arrays and by-reference aliases.
#[test]
fn test_mbstring_mixed_ini_retirement_preserves_nested_aliases() {
    check_ini_retirement(r#"<?php
function identityOwner(mixed $value): mixed { return $value; }
$original = ini_get('display_errors');
for ($iteration = 0; $iteration < 8; $iteration++) {
    $empty = identityOwner('');
    $binary = identityOwner(str_repeat('x', 1) . "\0y");
    $values = [$empty, [$binary]];
    $copy = $values;
    $reference =& $copy;
    unset($values); unset($copy); unset($empty); unset($binary);
    echo strlen((string)$reference[0]), ':', bin2hex((string)$reference[1][0]), ';';
    unset($reference);
}
ini_set('display_errors', $original);
unset($original);
echo 'done';
"#, &("0:780079;".repeat(8) + "done"));
}

/// String and scalar wrapper retirement cannot consume an outer pending PHP exception.
#[test]
fn test_mbstring_mixed_ini_retirement_preserves_pending_exception() {
    check_ini_retirement(r#"<?php
function identityOwner(mixed $value): mixed { return $value; }
$original = ini_get('display_errors');
ini_set('display_errors', 'kept');
try {
    try { throw new RuntimeException('outer'); }
    finally {
        $values = [identityOwner(ini_get('display_errors')), identityOwner(7), identityOwner(null)];
        $copy = $values;
        unset($values);
        echo $copy[0], ':';
        unset($copy);
    }
} catch (RuntimeException $error) { echo $error->getMessage(); }
ini_set('display_errors', $original);
unset($original);
gc_collect_cycles();
"#, "kept:outer");
}
