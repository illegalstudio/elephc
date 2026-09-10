//! Purpose:
//! Verifies public startup INI overrides through native mbstring and opaque eval.
//!
//! Called from:
//! - The string codegen test module through the real compiler CLI.
//!
//! Key details:
//! - PCRE2 comes from a managed test project and does not enable eval preg_* support.
//! - Tests cover settings, duplicate overrides, MIME diagnostics, and dependency selection.

use crate::support::*;
use std::process::Command;

/// Uses public INI wrappers to mutate shared mbstring/Core state and restore configured defaults.
#[test]
fn test_mbstring_public_ini_mutation_and_restore() {
    let source = r#"<?php
function exercise_ini(): void {
    echo ini_get("mbstring.language"), ":";
    echo ini_set("mbstring.language", "Japanese"), ":", mb_language(), ":", ini_get("mbstring.language"), "\n";
    ini_restore("mbstring.language");
    echo ini_get("mbstring.language"), ":", mb_language(), "\n";
    echo ini_get("display_errors"), ":", ini_set("display_errors", false), ":";
    echo strlen((string)ini_get("display_errors")), ":";
    ini_restore("display_errors");
    echo ini_get("display_errors"), ":", ini_set("max_input_vars", 5) === false, "\n";
    echo ini_get("mbstring.unknown") === false, ":", ini_set("mbstring.unknown", "x") === false, "\n";
}
exercise_ini();
"#;
    assert_eq!(run(source, &[], None).0, "neutral:neutral:Japanese:Japanese\nneutral:neutral\n1:1:0:1:1\n1:1\n");
}

/// Enumerates exact extension filters and merges shared rows with existing opcache values in key order.
#[test]
fn test_mbstring_public_ini_enumeration() {
    let source = r#"<?php
function exercise_ini(): void {
    ini_set("mbstring.language", "Japanese");
    $plain = (array)ini_get_all("mbstring", false);
    $details = (array)ini_get_all("mbstring");
    $language = (array)$details["mbstring.language"];
    echo $plain["mbstring.language"], ":", $language["global_value"], ":";
    echo $language["local_value"], ":", $language["access"], "\n";
    $core = (array)ini_get_all("core", false);
    echo $core["max_input_vars"], ":", $core["arg_separator.input"], "\n";
    $all = (array)ini_get_all(null, false);
    $previous = "";
    $ordered = true;
    foreach (array_keys($all) as $key) {
        if (strcmp($previous, (string)$key) > 0) { $ordered = false; }
        $previous = (string)$key;
    }
    echo $ordered, ":", $all["mbstring.language"], ":", $all["max_input_vars"], ":", $all["opcache.enable"], "\n";
    echo $core === $all, ":", $core["mbstring.language"], ":", $core["opcache.enable"], "\n";
    ini_restore("mbstring.language");
}
exercise_ini();
"#;
    assert_eq!(run(source, &[], None).0, "Japanese:neutral:Japanese:7\n1000:&\n1:Japanese:1000:1\n1:Japanese:1\n");
}

const STATE: &str = r#"
echo mb_internal_encoding(), ":", mb_http_output(), ":", mb_http_input("L"), ":";
echo mb_language(), ":", mb_substitute_character(), ":", mb_get_info("strict_detection"), ":";
$order = mb_detect_order();
if (is_array($order)) { echo implode(",", $order); }
echo "\n";
"#;

/// Lets opaque eval use injected public INI functions and restore the compiled startup value.
#[test]
fn test_mbstring_public_ini_native_and_eval() {
    let source = r#"<?php
echo ini_get("mbstring.language"), ":";
echo function_exists("ini_set"), function_exists("ini_restore"), function_exists("ini_get_all"), ":";
$source = (string)getenv("MB_STARTUP_CODE");
eval($source);
echo ini_get("mbstring.language"), ":", mb_language();
"#;
    let eval = r#"
echo ini_set("mbstring.language", "neutral"), ":", mb_language(), ":";
$rows = ini_get_all("mbstring", false);
echo $rows["mbstring.language"], ":";
ini_restore("mbstring.language");
"#;
    assert_eq!(run(source, &["mbstring.language=Japanese"], Some(eval)).0,
        "Japanese:111:Japanese:neutral:neutral:Japanese:Japanese");
}

/// Demonstrates configured character-versus-byte counting with the shipped startup example.
#[test]
fn test_mbstring_startup_example() {
    let source = include_str!("../../../examples/mbstring-config/main.php");
    for (settings, encoding, count) in [(vec![], "UTF-8", 4),
        (vec!["default_charset=8bit"], "8bit", 5)] {
        let output = run(source, &settings, None);
        assert_eq!(output.0, format!("Configured encoding: {encoding}\nLabel length: {count}\nConfigured language: neutral\nTemporary language: Japanese\nRestored language: neutral\n"));
    }
}

/// Shares CLI-configured state and subsequent native mutations with an opaque eval invocation.
#[test]
fn test_mbstring_startup_native_and_eval_settings() {
    let source = format!("<?php function show_startup(): void {{ {STATE} }} show_startup(); mb_internal_encoding('ASCII'); $startup_source = (string)getenv('MB_STARTUP_CODE'); eval($startup_source);");
    let eval = STATE.split("$order =").next().unwrap().to_owned()
        + "echo mb_detect_encoding('abc'), \"\\n\"; echo function_exists('preg_match') ? 'unexpected regex' : 'no preg';";
    let output = run(&source, &["default_charset=SJIS", "internal_encoding=8bit", "input_encoding=UTF-8",
        "mbstring.language=Japanese", "mbstring.language=neutral", "mbstring.detect_order=UTF-8,ASCII",
        "mbstring.substitute_character=none", "mbstring.strict_detection=1"],
        Some(&eval));
    assert_eq!(output.0, "8bit:SJIS:UTF-8:neutral:none:On:UTF-8,ASCII\nASCII:SJIS:UTF-8:neutral:none:On:UTF-8\nno preg");
    assert!(!output.1.contains("Warning:"), "{}", output.1);
}

/// Resolves empty core overrides through the configured charset and applies the final duplicate value.
#[test]
fn test_mbstring_startup_core_fallback() {
    let output = run("<?php echo mb_internal_encoding(), ':', mb_http_output(), ':', mb_http_input('L');",
        &["default_charset=ASCII", "default_charset=8bit", "internal_encoding=SJIS", "internal_encoding="], None);
    assert_eq!(output.0, "8bit:8bit:8bit");
}

/// Routes raw Core parser settings through executable startup validation before PHP entry.
#[test]
fn test_mbstring_startup_core_query_validation() {
    let output = run("<?php echo mb_strlen('abc');", &["max_input_vars=3cats",
        "max_input_nesting_level=-1", "arg_separator.input=", "display_errors=stderr"], None);
    assert_eq!(output.0, "3");
    let warning = "Warning: Invalid \"max_input_vars\" setting. Invalid quantity \"3cats\": unknown multiplier \"s\", interpreting as \"3\" for backwards compatibility\n";
    assert_eq!(output.1.matches(warning).count(), 1, "{}", output.1);
}

/// Validates the real MIME provider and preserves defaults after a rejected startup pattern.
#[test]
fn test_mbstring_startup_mime_configuration() {
    for (pattern, expected, warning) in [
        ("(?<=^application/)json", "(?<=^application/)json", false),
        ("[", r"^(text/|application/xhtml\+xml)", true),
    ] {
        let output = run("<?php echo mb_get_info('http_output_conv_mimetypes');",
            &[&format!("mbstring.http_output_conv_mimetypes={pattern}")], None);
        assert_eq!(output.0, expected);
        let diagnostic = "Warning: PHP Startup: [ (offset=1): missing terminating ] for character class\n";
        assert_eq!(output.1.matches(diagnostic).count(), usize::from(warning), "{}", output.1);
    }
}

/// Keeps optional dependencies out of unrelated programs and ignores unknown mbstring directives.
#[test]
fn test_mbstring_startup_link_scope() {
    for (source, setting) in [
        ("<?php echo strlen('abc');", "mbstring.language=Japanese"),
        ("<?php echo mb_strlen('abc');", "mbstring.unknown=1"),
    ] {
        let directory = make_cli_test_dir("mbstring_startup_scope");
        let php = directory.join("main.php");
        fs::write(&php, source).unwrap();
        let output = elephc_cli_command(&directory).args(["--ini", setting])
            .arg(&php).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let executed = Command::new(php.with_extension("")).output().unwrap();
        assert!(executed.status.success());
        assert_eq!(executed.stdout, b"3");
        fs::remove_dir_all(directory).unwrap();
    }
}

/// Releases temporary probe arguments while retaining a caller-owned function name for later reads.
#[test]
fn test_mbstring_startup_eval_capability_argument_ownership() {
    let calls = r#"
    echo function_exists("mb_strlen") ? "yes:" : "wrong:";
    echo function_exists($name) ? "yes:" : "wrong:";
    echo function_exists("missing_name") ? "wrong:" : "no:";
"#;
    let eval = " $name = \"mb_strlen\"; ".to_owned() + &calls.repeat(8) + "echo $name;";
    let output = run("<?php $source = (string)getenv('MB_STARTUP_CODE'); eval($source);",
        &["default_charset=8bit"], Some(&eval));
    assert_eq!(output.0, "yes:yes:no:".repeat(8) + "mb_strlen");
}

/// Compiles with managed PCRE2, runs the binary, and requires balanced native heap ownership.
fn run(source: &str, settings: &[&str], eval: Option<&str>) -> (String, String) {
    let directory = make_cli_test_dir("mbstring_startup");
    let php = directory.join("main.php");
    fs::write(&php, source).unwrap();
    let mut compiler = elephc_cli_command_with_managed_pcre2(&directory);
    for setting in settings { compiler.args(["--ini", setting]); }
    let output = compiler.arg("--heap-debug").arg(&php).output().unwrap();
    assert!(output.status.success(), "{}: {}", directory.display(), String::from_utf8_lossy(&output.stderr));
    let mut binary = Command::new(php.with_extension(""));
    if let Some(source) = eval { binary.env("MB_STARTUP_CODE", source); }
    let output = binary.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{}: {stdout}\n{stderr}", directory.display());
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {stderr}", directory.display());
    fs::remove_dir_all(directory).unwrap();
    (stdout, stderr)
}
