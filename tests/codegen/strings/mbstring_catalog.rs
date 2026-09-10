//! Purpose:
//! Verifies the public mbstring encoding catalog through native and opaque eval calls.
//!
//! Called from:
//! - The focused codegen string suite.
//!
//! Key details:
//! - Expected names and order come from the independent PHP reflection fixture.
//! - Mutated results must remain independent from later calls and caller copies.

use crate::support::*;

/// Wraps source for native execution or opaque Magician execution without changing its PHP literals.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php namespace EncodingCatalog; {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Preserves the complete PHP catalog order, namespaced lookup, callable results, and independent arrays.
#[test]
fn test_mbstring_list_encodings_catalog() {
    let baseline: serde_json::Value = serde_json::from_str(
        include_str!("../../../scripts/mbstring/php_surface.json")).unwrap();
    let names: Vec<_> = baseline["encoding_order"].as_array().unwrap().iter()
        .map(|name| name.as_str().unwrap()).collect();
    let expected = format!("{}\nBASE64:changed:BASE64:{}:SJIS", names.join("\n"), names.len());
    let body = r#"
mb_internal_encoding("SJIS");
$names = Mb_LiSt_EnCoDiNgS();
foreach ($names as $name) { echo (string)$name, "\n"; }
$copy = $names;
$copy[0] = "changed";
$list = mb_list_encodings(...);
$fresh = $list();
echo (string)$names[0], ":", (string)$copy[0], ":", (string)$fresh[0], ":",
    count(call_user_func("mb_list_encodings")), ":", mb_internal_encoding();
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), expected);
    }
}

/// Evaluates excess supplied arguments before returning a catchable zero-arity PHP diagnostic.
#[test]
fn test_mbstring_list_encodings_eval_arity() {
    let body = r#"
function catalog_argument(): int { echo "argument\n"; return 1; }
try { mb_list_encodings(catalog_argument()); }
catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
echo count(mb_list_encodings());
"#;
    assert_eq!(compile_and_run(&program(body, true)),
        "argument\nmb_list_encodings() expects exactly 0 arguments, 1 given\n79");
}

/// Releases repeated catalog results while reused mutation operands isolate array ownership.
#[test]
fn test_mbstring_list_encodings_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 24] {
            let calls = "$names = mb_list_encodings(); $names[$index] = $replacement;\n".repeat(count);
            let source = program(&format!("$index = 0; $replacement = \"changed\"; {calls}echo count($names);"), eval);
            let output = compile_and_run_with_gc_stats(&source);
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, "79");
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "encoding catalog retained owners; eval={eval}");
    }
}

/// Enables mbstring runtime symbols when a callable descriptor is the only reference.
#[test]
fn test_mbstring_catalog_callable_only_requirements() {
    for body in [
        "$list = mb_list_encodings(...); echo count($list());",
        "$name = $argc > 0 ? 'mb_list_encodings' : 'mb_encoding_aliases'; echo count(call_user_func($name));",
    ] {
        assert_eq!(compile_and_run(&format!("<?php {body}")), "79");
    }
}

/// Makes opaque string callbacks available when the CLI explicitly enables the mbstring bridge.
#[test]
fn test_mbstring_catalog_forced_runtime_callable() {
    let dir = make_cli_test_dir("elephc_mbstring_forced_callable");
    let php = dir.join("main.php");
    std::fs::write(&php,
        "<?php $name = (string)getenv('ELEPHC_MBSTRING_CALLBACK'); echo count(call_user_func($name));").unwrap();
    let compiled = elephc_cli_command_with_oniguruma(&dir).arg("--with-mbstring").arg(&php).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let executed = std::process::Command::new(php.with_extension(""))
        .env("ELEPHC_MBSTRING_CALLBACK", "mb_list_encodings").output().unwrap();
    assert!(executed.status.success(), "{}", String::from_utf8_lossy(&executed.stderr));
    assert_eq!(executed.stdout, b"79");
    std::fs::remove_dir_all(dir).unwrap();
}
