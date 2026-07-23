//! Purpose:
//! Integration tests for the optional `eval()` bridge.
//! Covers language-construct visibility, conditional bridge linking, scope
//! synchronization, dynamic declarations, EvalIR execution, and supported
//! builtin dispatch through end-to-end codegen.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval` through Rust's test harness.
//!
//! Key details:
//! - Fixtures exercise the native/EvalIR boundary rather than the frozen legacy
//!   AST backend, and many cases assert post-barrier native visibility.

use crate::support::*;

/// Asserts an eval failure stayed inside the C ABI/error-status contract.
fn assert_no_rust_panic_leaked(stderr: &str) {
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("thread '"),
        "eval failure leaked Rust panic diagnostics: {stderr}"
    );
}

/// Asserts a scope-aware literal eval was lowered as an internal EIR function without Magician.
fn assert_scope_eir_aot_without_bridge(
    user_asm: &str,
    runtime_asm: &str,
    required_libraries: &[String],
    context: &str,
) {
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with eval scope"),
        "{context} should call the scope-aware internal EIR function:\n{user_asm}"
    );
    assert!(
        user_asm.contains("@fn name=__eir@evalaot_scope_")
            && user_asm.contains("__elephc_eval_scope_set"),
        "{context} should contain the lowered EIR scope function and writes:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("compiled local scalar")
            && !user_asm.contains("compiled direct local read/write"),
        "{context} should not use legacy eval AOT mini-backend markers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute")
            && !runtime_asm.contains("__elephc_eval_execute"),
        "{context} should not reference the interpreter bridge"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "{context} should not link elephc_magician: {required_libraries:?}"
    );
}

/// Verifies `eval` is resolved as a language construct, not a PHP-visible callable function.
#[test]
fn test_eval_is_not_function_exists_or_callable() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("eval") ? "1" : "0";
echo is_callable("eval") ? "1" : "0";
"#,
    );
    assert_eq!(out, "00");
}

/// Verifies an unsupported literal `eval()` still references the bridge symbol and requests libelephc-magician.
#[test]
fn test_eval_codegen_requires_eval_bridge() {
    let dir = make_cli_test_dir("elephc_magician_bridge_asm");
    // Indexed writes into a caller array still require bridge semantics;
    // read-only foreach fragments now compile through the scope-read AOT path.
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $items = [1]; eval('$items[0] = 7; foreach ($items as $x) { echo $x; }');",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "user assembly should call the eval bridge:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT fallback"),
        "unsupported literal eval should be marked as a bridge fallback:\n{user_asm}"
    );
    assert!(
        user_asm
            .contains("eval literal AOT fallback: array/iterable semantics need bridge fallback"),
        "unsupported array-write literal eval should explain the fallback reason:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_context_new"),
        "user assembly should create a persistent eval context:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_context_free"),
        "user assembly should free the persistent eval context:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "required libraries should include elephc_magician: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a static literal `strlen()` call inside eval is folded into EIR-function AOT.
#[test]
fn test_literal_eval_static_strlen_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_strlen_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php echo eval('return strlen(\"abcd\");');",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval strlen should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval strlen should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval strlen should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval strlen should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval strlen should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies foldable static builtins inside eval use EIR AOT without magician.
#[test]
fn test_literal_eval_static_scalar_builtins_use_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_scalar_builtins_aot");
    let source = r#"<?php
namespace EvalAotBuiltinFold;
echo eval('return STRLEN("ab") + InTvAl("40") + ABS(-2);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static scalar builtins should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static scalar builtins should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval static scalar builtins should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval static scalar builtins should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval static scalar builtins should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "44");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies additional pure static builtins fold before the eval fragment lowers to EIR AOT.
#[test]
fn test_literal_eval_more_static_builtins_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_more_static_builtins_aot");
    let source = r#"<?php
namespace EvalAotMoreBuiltinFold;
echo eval('echo STRTOUPPER("ab") . ":" . strtolower("CD") . ":" . strrev("abc");
echo ":" . strval(42) . ":" . floatval("1.5") . ":";
echo is_int(7) . is_string("x") . is_bool(false) . is_float(1.25) . is_null(null);
echo ":" . GETTYPE(1.25) . ":" . gettype(null) . ":";
echo IS_SCALAR("x") . is_scalar(null);
return max(4, 9, 2) + min(4, 9, 2) + ord("A") + strlen(chr(65));');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval pure static builtins should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval pure static builtins should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval pure static builtins should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval pure static builtins should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval pure static builtins should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "AB:cd:cba:42:1.5:11111:double:NULL:177");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static string slice/repeat calls fold before literal eval EIR AOT lowering.
#[test]
fn test_literal_eval_static_string_builtins_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_string_builtins_aot");
    let source = r#"<?php
namespace EvalAotStringBuiltinFold;
echo eval('return SUBSTR("abcdef", 2)
    . ":" . substr("abcdef", 1, 3)
    . ":" . substr("abc", 9)
    . ":" . str_repeat("xy", 3)
    . ":" . strlen(str_repeat("z", 4));');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static string builtins should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static string builtins should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static string builtins should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static string builtins should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static string builtins should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "cdef:bcd::xyxyxy:4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies ASCII text-normalization static calls fold before literal eval EIR lowering.
#[test]
fn test_literal_eval_static_ascii_text_builtins_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_ascii_text_builtins_aot");
    let source = r#"<?php
namespace EvalAotAsciiTextBuiltinFold;
echo eval('return "[" . UCFIRST("eval") . "]"
    . "[" . lcfirst("LOUD") . "]"
    . "[" . trim("  hi\n") . "]"
    . "[" . ltrim("  left ") . "]"
    . "[" . rtrim(" right  ") . "]"
    . "[" . chop("tail  ") . "]";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static ASCII text builtins should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static ASCII text builtins should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static ASCII text builtins should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static ASCII text builtins should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static ASCII text builtins should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "[Eval][lOUD][hi][left ][ right][tail]");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static string predicate calls fold before literal eval EIR lowering.
#[test]
fn test_literal_eval_static_string_predicates_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_string_predicates_aot");
    let source = r#"<?php
namespace EvalAotStringPredicateFold;
echo eval('return (STR_CONTAINS("abcdef", "cd") ? "C" : "bad")
    . (str_contains("abcdef", "xy") ? "bad" : "N")
    . (str_starts_with("abcdef", "ab") ? "S" : "bad")
    . (str_ends_with("abcdef", "ef") ? "E" : "bad")
    . (str_contains("abcdef", "") ? "E" : "bad");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static string predicates should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static string predicates should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static string predicates should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static string predicates should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static string predicates should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "CNSEE");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies pure numeric static builtins fold before literal eval EIR AOT lowering.
#[test]
fn test_literal_eval_numeric_static_builtins_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_numeric_static_builtins_aot");
    let source = r#"<?php
namespace EvalAotNumericBuiltinFold;
echo eval('return FLOOR(3.7) + ceil(2.1) + sqrt(16) + round(2.5);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval numeric static builtins should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval numeric static builtins should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only numeric static builtins should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only numeric static builtins should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only numeric static builtins should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "13");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static `count()` calls on literal arrays fold before literal eval EIR AOT lowering.
#[test]
fn test_literal_eval_static_array_count_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_array_count_aot");
    let source = r#"<?php
namespace EvalAotArrayCountFold;
echo eval('return COUNT([1, 2, 3])
    + count(["a" => 1, "b" => 2])
    + count(["1" => "a", 1 => "b", "01" => "c"])
    + count([1, [2, 3], ["x" => 4]])
    + count([true => "yes", 1 => "one", false => "no", 0 => "zero"])
    + count([null => "empty", "" => "blank", "name" => "Ada"])
    + count([2.0 => "two", 2 => "int", -2.0 => "minus", -2 => "intminus"]);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static array count should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static array count should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static array count should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static array count should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static array count should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "16");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies runtime `count()` calls on eval-created local arrays use EIR AOT.
#[test]
fn test_literal_eval_local_array_count_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_local_array_count_aot");
    let source = r#"<?php
echo eval('$items = [1, 2, 3]; $map = ["a" => 1, "b" => 2]; return count($items) + count($map);');
echo ":" . count($items) . ":" . count($map);
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval runtime count on local arrays should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval runtime count on local arrays should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "runtime count on eval-created arrays should flush created locals through scope helpers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_context_new"),
        "scope-only runtime count on local arrays should not create an eval bridge context:\n{user_asm}"
    );
    assert!(
        runtime_asm.contains("__elephc_eval_scope_set"),
        "scope-only runtime count on local arrays should emit core eval scope helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "scope-only runtime count on local arrays should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "5:3:2");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `count()` can consume caller-scope arrays through direct-param EIR AOT.
#[test]
fn test_literal_eval_count_scope_read_array_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_count_scope_read_array_aot");
    let source = r#"<?php
$items = [1, 2, 3];
$map = ["a" => 1, "b" => 2];
echo eval('return count($items) . ":" . count($map);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "count over caller-scope arrays should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "count over caller-scope arrays should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "count over caller-scope arrays should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "count over caller-scope arrays should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3:2");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies named `count()` with default mode can use direct-param EIR AOT.
#[test]
fn test_literal_eval_count_named_default_mode_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_count_named_default_mode_aot");
    let source = r#"<?php
$items = [1, 2, 3];
echo eval('return count(value: $items) . ":" . count(value: $items, mode: 0);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "named count() with default mode should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "named count() with default mode should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "named count() with default mode should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "named count() with default mode should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3:3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `count()` on a caller scalar keeps the bridge fallback instead of direct AOT.
#[test]
fn test_literal_eval_count_scope_read_scalar_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_count_scope_read_scalar_bridge");
    let source = r#"<?php
$n = 42;
echo eval('return count($n);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "count over a caller scalar should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "count over a caller scalar should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies recursive `count()` mode stays on the bridge until EIR models it.
#[test]
fn test_literal_eval_count_named_recursive_mode_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_count_named_recursive_mode_bridge");
    let source = r#"<?php
$items = [1, [2]];
echo eval('return count(value: $items, mode: 1);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "recursive count() mode should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "recursive count() mode should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static `array_key_exists()` calls fold before literal eval EIR AOT lowering.
#[test]
fn test_literal_eval_static_array_key_exists_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_array_key_exists_aot");
    let source = r#"<?php
namespace EvalAotArrayKeyExistsFold;
echo eval('return (ARRAY_KEY_EXISTS("name", ["name" => null]) ? "Y" : "bad")
    . (array_key_exists("missing", ["name" => 1]) ? "bad" : "N")
    . (array_key_exists("1", [1 => "one"]) ? "I" : "bad")
    . (array_key_exists(2, ["0" => "a", "01" => "b", 2 => "c"]) ? "K" : "bad")
    . (array_key_exists(0, ["x", "y"]) ? "Z" : "bad")
    . (array_key_exists(2, ["x", "y"]) ? "bad" : "O")
    . (array_key_exists(true, [1 => "yes"]) ? "T" : "bad")
    . (array_key_exists(false, [0 => "no"]) ? "F" : "bad")
    . (array_key_exists(null, ["" => "empty"]) ? "E" : "bad")
    . (array_key_exists(2.0, [2.0 => "two"]) ? "D" : "bad")
    . (array_key_exists(-2.0, [-2.0 => "minus"]) ? "M" : "bad");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static array_key_exists should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static array_key_exists should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static array_key_exists should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static array_key_exists should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static array_key_exists should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "YNIKZOTFEDM");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `array_key_exists()` can inspect caller-scope arrays through direct-param AOT.
#[test]
fn test_literal_eval_array_key_exists_scope_read_array_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_array_key_exists_scope_read_array_aot");
    let source = r#"<?php
$items = ["zero", "one"];
$map = ["name" => null, "age" => 42];
echo eval('return (array_key_exists(1, $items) ? "I" : "bad")
    . ":" . (array_key_exists(3, $items) ? "bad" : "N")
    . ":" . (array_key_exists("name", $map) ? "A" : "bad")
    . ":" . (array_key_exists("missing", $map) ? "bad" : "M");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "array_key_exists over caller-scope arrays should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "array_key_exists over caller-scope arrays should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "array_key_exists over caller-scope arrays should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "array_key_exists over caller-scope arrays should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "I:N:A:M");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies caller-scope `array_key_exists()` supports null and integral float keys in EIR AOT.
#[test]
fn test_literal_eval_array_key_exists_scope_read_null_and_float_keys_use_eir_aot() {
    let dir =
        make_cli_test_dir("elephc_literal_eval_array_key_exists_scope_read_null_float_keys_aot");
    let source = r#"<?php
$items = ["zero", "one"];
$map = ["" => "empty", -2 => "minus", 2 => "two"];
echo eval('return (array_key_exists(1.0, $items) ? "F" : "bad")
    . ":" . (array_key_exists(null, $map) ? "N" : "bad")
    . ":" . (array_key_exists(-2.0, $map) ? "M" : "bad");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "array_key_exists with null/integral float keys should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "array_key_exists with null/integral float keys should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "array_key_exists with null/integral float keys should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "array_key_exists with null/integral float keys should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "F:N:M");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies named args for EIR-safe runtime builtins stay on the eval AOT path.
#[test]
fn test_literal_eval_named_runtime_builtins_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_named_runtime_builtins_aot");
    let source = r#"<?php
$items = ["zero", "one"];
$flag = true;
$map = ["name" => "Ada"];
echo eval('return count(value: $items)
    . ":" . (boolval(value: $flag) ? "B" : "bad")
    . ":" . (array_key_exists(array: $map, key: "name") ? "Y" : "bad")
    . ":" . (array_key_exists(key: "missing", array: $map) ? "bad" : "N");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "named runtime builtins should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "named runtime builtins should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "named runtime builtins should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "named runtime builtins should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "2:B:Y:N");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static spread args for EIR-safe runtime builtins stay on the eval AOT path.
#[test]
fn test_literal_eval_static_spread_runtime_builtins_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_spread_runtime_builtins_aot");
    let source = r#"<?php
$items = ["zero", "one"];
$flag = true;
$map = ["name" => "Ada"];
echo eval('return count(...["value" => $items])
    . ":" . (boolval(...["value" => $flag]) ? "B" : "bad")
    . ":" . (array_key_exists(...["array" => $map, "key" => "name"]) ? "Y" : "bad")
    . ":" . (array_key_exists(...["key" => "missing", "array" => $map]) ? "bad" : "N");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "static-spread runtime builtins should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "static-spread runtime builtins should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "static-spread runtime builtins should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "static-spread runtime builtins should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "2:B:Y:N");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies dynamic spread args for runtime builtins keep the eval bridge fallback.
#[test]
fn test_literal_eval_dynamic_spread_runtime_builtin_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_dynamic_spread_runtime_builtin_bridge");
    let source = r#"<?php
$items = ["zero", "one"];
$args = ["value" => $items];
echo eval('return count(...$args);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "dynamic-spread runtime builtin should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "dynamic-spread runtime builtin should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `array_key_exists()` on a caller scalar keeps the bridge fallback.
#[test]
fn test_literal_eval_array_key_exists_scope_read_scalar_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_array_key_exists_scope_read_scalar_bridge");
    let source = r#"<?php
$n = 42;
echo eval('return array_key_exists(1, $n);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "array_key_exists over a caller scalar should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "array_key_exists over a caller scalar should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fractional float keys stay on the bridge so PHP's deprecation is preserved.
#[test]
fn test_literal_eval_array_key_exists_fractional_float_key_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_array_key_exists_fractional_float_key_bridge");
    let source = r#"<?php
$items = ["zero", "one", "two"];
echo eval('return array_key_exists(2.7, $items);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "fractional float key array_key_exists should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "fractional float key array_key_exists should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies string-key probes on caller indexed arrays use EIR AOT with PHP key semantics.
#[test]
fn test_literal_eval_array_key_exists_string_key_indexed_scope_uses_eir_aot() {
    let dir = make_cli_test_dir("elephc_literal_eval_array_key_exists_string_indexed_scope_aot");
    let source = r#"<?php
$items = ["zero", "one"];
echo eval('return (array_key_exists("1", $items) ? "Y" : "bad")
    . ":" . (array_key_exists("x", $items) ? "bad" : "N")
    . ":" . (array_key_exists("01", $items) ? "bad" : "Z");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "string-key array_key_exists over a caller indexed array should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "string-key array_key_exists over a caller indexed array should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "string-key array_key_exists over a caller indexed array should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "string-key array_key_exists over a caller indexed array should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "Y:N:Z");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies immediate reads from literal arrays lower through eval EIR AOT.
#[test]
fn test_literal_eval_static_array_literal_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_array_literal_read_aot");
    let source = r#"<?php
echo eval('return [1, 2, 3][1];');
echo ":";
echo eval('return ["name" => "Ada"]["name"];');
echo ":";
echo eval('return isset(["name" => "Ada"]["name"]) ? "Y" : "bad";');
echo ":";
echo eval('return isset(["name" => null]["name"]) ? "bad" : "N";');
echo ":";
echo eval('return isset(["name" => "Ada"]["missing"]) ? "bad" : "M";');
echo ":";
echo eval('return empty(["name" => ""]["name"]) ? "E" : "bad";');
echo ":";
echo eval('return empty(["name" => "Ada"]["name"]) ? "bad" : "V";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static array reads should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static array reads should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static array reads should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static array reads should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static array reads should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "2:Ada:Y:N:M:E:V");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static array literals returned from eval lower through EIR AOT.
#[test]
fn test_literal_eval_static_array_literal_return_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_array_literal_return_aot");
    let source = r#"<?php
$items = eval('return ["a", "b"];');
echo $items[0] . $items[1];
echo ":";
$map = eval('return ["left" => "L", "right" => "R"];');
echo $map["left"] . $map["right"];
echo ":";
$rows = eval('return [[10, 20], ["name" => "Ada"]];');
echo $rows[0][1] . ":" . $rows[1]["name"];
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static array returns should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static array returns should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static array returns should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static array returns should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static array returns should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "ab:LR:20:Ada");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static associative array auto keys can lower through eval EIR AOT.
#[test]
fn test_literal_eval_static_array_next_auto_key_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_array_next_key_aot");
    let source = r#"<?php
echo eval('return [2 => "two", "tail"][3];');
echo ":";
echo eval('return [-2 => "minus", "tail"][-1];');
echo ":";
echo eval('return ["2" => "two", "tail"][3];');
echo ":";
echo eval('return ["02" => "two", "tail"][0];');
echo ":";
echo eval('return [true => "yes", "tail"][2];');
echo ":";
echo eval('return [false => "no", "tail"][1];');
echo ":";
echo eval('return [null => "empty"][""];');
echo ":";
echo eval('return [null => "empty", "tail"][0];');
echo ":";
echo eval('return [2.0 => "two", "tail"][3];');
echo ":";
echo eval('return [-2.0 => "minus", "tail"][-1];');
echo ":";
echo eval('return array(2 => "two", "tail")[3];');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "static array next-key reads should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "static array next-key reads should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static array next-key reads should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static array next-key reads should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static array next-key reads should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(
        out,
        "tail:tail:tail:tail:tail:tail:empty:tail:tail:tail:tail"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fractional float array keys stay on the bridge because PHP emits a precision warning.
#[test]
fn test_eval_static_array_fractional_float_key_uses_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_eval_static_array_fractional_float_key_bridge");
    let source = r#"<?php
echo eval('return [2.7 => "two", "tail"][3];');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT fallback"),
        "fractional float array key should keep the explicit literal eval fallback marker:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "fractional float array key should execute through the bridge:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "fractional float array key fallback should link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "tail");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static array writes into eval scope lower through EIR AOT.
#[test]
fn test_literal_eval_static_array_scope_write_uses_eir_aot_scope_helpers() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_array_scope_write_aot");
    let source = r#"<?php
eval('$items = ["a", "b"];');
echo $items[1];
echo ":";
eval('$map = ["name" => "Ada"];');
echo $map["name"];
echo ":";
eval('$legacy = array("x", "y");');
echo $legacy[0] . $legacy[1];
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "static array scope writes should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "static array scope writes should flush through eval scope set:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "static array scope writes should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        runtime_asm.contains("__elephc_eval_scope_set"),
        "static array scope writes should emit core eval scope helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "scope-only static array eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "b:Ada:xy");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies named/static-spread builtin args normalize before literal eval AOT folding.
#[test]
fn test_literal_eval_static_builtin_named_args_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_builtin_named_args_aot");
    let source = r#"<?php
namespace EvalAotNamedBuiltinFold;
echo eval('return strlen(string: "ab")
    + strlen(...["string" => "cd"])
    + count(value: [1, 2, 3])
    + (array_key_exists(array: ["name" => null], key: "name") ? 10 : 0)
    + (str_contains(needle: "x", haystack: "xyz") ? 20 : 0)
    + strlen(str_repeat(times: 3, string: "q"))
    + strlen(substr(length: 2, string: "abcdef", offset: 1));');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static builtin named args should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static builtin named args should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static builtin named args should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static builtin named args should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static builtin named args should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "42");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies foldable static builtins work in scope-aware EIR statement bodies.
#[test]
fn test_literal_eval_static_scalar_builtins_in_local_body_use_aot() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_scalar_builtins_local_body_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php echo eval('$x = intval(\"42\"); echo $x + 1; return abs(-10);');",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval static scalar builtins in a local body",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "4310");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a static user function with scalar args can be called from literal eval AOT.
#[test]
fn test_literal_eval_static_user_function_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_function_aot");
    let source = r#"<?php
function inc(int $x): int {
    return $x + 1;
}
echo eval('return inc(41);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static function call should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static function call should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval static function call should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval static function call should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval static function call should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "42");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static user-function named args are accepted by literal eval EIR AOT.
#[test]
fn test_literal_eval_static_user_function_named_args_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_function_named_args_aot");
    let source = r#"<?php
function join_named(string $left, string $right, bool $bang): string {
    return $left . ":" . $right . ($bang ? "!" : ".");
}
echo eval('return join_named(bang: true, right: "B", left: "A");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static function named args should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static function named args should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval static function named args should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval static function named args should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval static function named args should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A:B!");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static spread args for scalar user functions are accepted by literal eval EIR AOT.
#[test]
fn test_literal_eval_static_user_function_static_spread_args_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_function_static_spread_args_aot");
    let source = r#"<?php
function join_static_spread(string $left, string $right, bool $bang = false): string {
    return $left . ":" . $right . ($bang ? "!" : ".");
}
echo eval('return join_static_spread(...["right" => "B", "left" => "A"])
    . ":" . join_static_spread(...["left" => "C", "right" => "D", "bang" => true]);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static function static-spread args should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static function static-spread args should not call the bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static-spread function eval should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static-spread function eval should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static-spread function eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A:B.:C:D!");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies dynamic spread args for scalar user functions keep the eval bridge fallback.
#[test]
fn test_literal_eval_static_user_function_dynamic_spread_args_keep_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_function_dynamic_spread_args_bridge");
    let source = r#"<?php
function join_dynamic_spread(string $left, string $right): string {
    return $left . ":" . $right;
}
$args = ["left" => "A", "right" => "B"];
echo eval('return join_dynamic_spread(...$args);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "dynamic-spread static function call should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "dynamic-spread static function call should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static user-function scalar defaults are accepted by literal eval EIR AOT.
#[test]
fn test_literal_eval_static_user_function_defaults_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_function_defaults_aot");
    let source = r#"<?php
function greet_default(string $name, string $suffix = "!", bool $loud = true): string {
    return $name . $suffix . ($loud ? "Y" : "n");
}
echo eval('return greet_default("Ada") . ":" . greet_default(loud: false, name: "Lin");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static function defaults should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static function defaults should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval static function defaults should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval static function defaults should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval static function defaults should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "Ada!Y:Lin!n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies literal eval AOT can call scalar user functions beyond integer-only signatures.
#[test]
fn test_literal_eval_static_scalar_user_functions_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_scalar_functions_aot");
    let source = r#"<?php
function eval_label(string $s, bool $ok): string {
    return $s . ":" . ($ok ? "T" : "F");
}
function eval_scale(float $x): float {
    return $x + 2.25;
}
echo eval('echo eval_label("Hi", true); return eval_scale(1.5);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval scalar static function calls should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval scalar static function calls should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only scalar static function eval should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only scalar static function eval should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only scalar static function eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "Hi:T3.75");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies public typed static methods can be called from literal eval EIR AOT.
#[test]
fn test_literal_eval_static_method_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_method_aot");
    let source = r#"<?php
class EvalAotStaticMethodBox {
    public static function join(string $left, string $right, bool $bang = false): string {
        return $left . ":" . $right . ($bang ? "!" : ".");
    }

    public static function inc(int $x): int {
        return $x + 1;
    }
}
echo eval('return EvalAotStaticMethodBox::join("A", "B")
    . "|" . EvalAotStaticMethodBox::join(right: "D", left: "C", bang: true)
    . "|" . EvalAotStaticMethodBox::inc(41);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval static method calls should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval static method calls should not call the bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static method eval should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static method eval should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static method eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A:B.|C:D!|42");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies public static methods without declared scalar signatures keep the bridge fallback.
#[test]
fn test_literal_eval_untyped_static_method_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_method_untyped_bridge");
    let source = r#"<?php
class EvalAotUntypedStaticMethodBox {
    public static function add($left, $right) {
        return $left + $right;
    }
}
echo eval('return EvalAotUntypedStaticMethodBox::add(1, 2);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "untyped static method calls should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "untyped static method calls should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static `call_user_func()` builtin callbacks can use literal eval EIR AOT.
#[test]
fn test_literal_eval_static_call_user_func_builtin_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_call_user_func_builtin_aot");
    let source = r#"<?php
$s = "abcd";
echo eval('return call_user_func("strlen", $s)
    . ":" . call_user_func_array("strlen", ["string" => $s])
    . ":" . call_user_func("strtoupper", "az");');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "static call_user_func builtin callbacks should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "static call_user_func builtin callbacks should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "static call_user_func builtin callbacks should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "static call_user_func builtin callbacks should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "4:4:AZ");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static `call_user_func*()` user callbacks can use literal eval EIR AOT.
#[test]
fn test_literal_eval_static_call_user_func_user_function_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_call_user_func_user_function_aot");
    let source = r#"<?php
function eval_cuf_join(string $left, string $right, bool $bang = false): string {
    return $left . ":" . $right . ($bang ? "!" : ".");
}
echo eval('return call_user_func("eval_cuf_join", "A", "B")
    . "|" . call_user_func_array("eval_cuf_join", ["right" => "D", "left" => "C", "bang" => true])
    . "|" . call_user_func(eval_cuf_join(...), "E", "F", true)
    . "|" . call_user_func_array(eval_cuf_join(...), ["right" => "H", "left" => "G"]);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "static call_user_func user callbacks should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "static call_user_func user callbacks should not call the bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "static call_user_func user callbacks should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "static call_user_func user callbacks should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "static call_user_func user callbacks should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A:B.|C:D!|E:F!|G:H.");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static method callbacks can use literal eval EIR AOT.
#[test]
fn test_literal_eval_static_method_callbacks_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_method_callbacks_aot");
    let source = r#"<?php
class EvalAotStaticMethodCallbackBox {
    public static function join(string $left, string $right, bool $bang = false): string {
        return $left . ":" . $right . ($bang ? "!" : ".");
    }

    public static function inc(int $x): int {
        return $x + 1;
    }
}
echo eval('return call_user_func("EvalAotStaticMethodCallbackBox::join", "A", "B")
    . "|" . call_user_func(["EvalAotStaticMethodCallbackBox", "join"], "C", "D", true)
    . "|" . call_user_func_array("EvalAotStaticMethodCallbackBox::join", ["right" => "F", "left" => "E"])
    . "|" . call_user_func_array(["EvalAotStaticMethodCallbackBox", "inc"], [41])
    . "|" . call_user_func([EvalAotStaticMethodCallbackBox::class, "join"], "G", "H")
    . "|" . call_user_func_array([EvalAotStaticMethodCallbackBox::class, "inc"], [9])
    . "|" . call_user_func(EvalAotStaticMethodCallbackBox::join(...), "I", "J", true)
    . "|" . call_user_func_array(EvalAotStaticMethodCallbackBox::join(...), ["right" => "L", "left" => "K"]);');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "static method callbacks should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "static method callbacks should not call the bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only static method callback eval should not reference eval helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only static method callback eval should not emit eval helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only static method callback eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A:B.|C:D!|E:F.|42|G:H.|10|I:J!|K:L.");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static method callbacks without declared scalar signatures keep the bridge fallback.
#[test]
fn test_literal_eval_untyped_static_method_callback_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_method_callback_untyped_bridge");
    let source = r#"<?php
class EvalAotUntypedStaticMethodCallbackBox {
    public static function add($left, $right) {
        return $left + $right;
    }
}
echo eval('return call_user_func_array(EvalAotUntypedStaticMethodCallbackBox::add(...), [1, 2]);');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "untyped static method callbacks should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "untyped static method callbacks should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies dynamic `call_user_func()` callbacks keep the eval bridge fallback.
#[test]
fn test_literal_eval_dynamic_call_user_func_callback_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_call_user_func_dynamic_callback_bridge");
    let source = r#"<?php
$fn = "strlen";
echo eval('return call_user_func($fn, "abcd");');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "dynamic call_user_func callback should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "dynamic call_user_func callback should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a scalar literal eval return is emitted directly without the interpreter entry point.
#[test]
fn test_literal_eval_scalar_return_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_aot_return_asm");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php echo eval('return 7;');",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled"),
        "scalar literal eval should use the AOT path:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "scalar literal eval return should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "scalar literal eval should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only scalar literal eval should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only scalar literal eval should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only scalar literal eval should not link elephc_magician: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies literal eval returns PHP null for explicit null and fallthrough.
#[test]
fn test_literal_eval_null_return_and_fallthrough_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_null_return_eir_aot");
    let source = r#"<?php
echo eval('return null;') === null ? "N" : "bad";
echo ":";
$fallthrough = eval('echo "body";');
echo ":" . ($fallthrough === null ? "N" : "bad");
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 2,
        "null/fallthrough literal evals should use internal EIR AOT functions:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "null/fallthrough literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only null/fallthrough literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only null/fallthrough literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only null/fallthrough literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "N:body:N");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies ternary expressions inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_ternary_expressions_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_ternary_eir_aot");
    let source = r#"<?php
echo eval('return true ? "yes" : "no";');
echo ":";
echo eval('return false ?: "fallback";');
echo ":";
echo eval('return strlen("abc") == 3 ? "len" : "bad";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 2,
        "ternary literal evals should use internal EIR AOT functions:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "ternary literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only ternary literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only ternary literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only ternary literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "yes:fallback:len");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies null coalesce inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_null_coalesce_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_null_coalesce_eir_aot");
    let source = r#"<?php
echo eval('return null ?? "literal";');
echo ":";
echo eval('return "set" ?? "bad";');
echo ":";
$a = "caller";
echo eval('return $a ?? "fallback";');
echo ":";
echo eval('return $missing ?? "missing";');
echo ":";
$n = null;
echo eval('return $n ?? "nullcaller";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 4,
        "null coalesce literal evals should use internal EIR AOT functions:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "null coalesce caller reads should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "null coalesce literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only null coalesce literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only null coalesce literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only null coalesce literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "literal:set:caller:missing:nullcaller");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies match expressions inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_match_expression_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_match_eir_aot");
    let source = r#"<?php
echo eval('return match ("1") { 1 => "int", "1" => "string", default => "other" };');
echo ":";
$x = 3;
echo eval('return match ($x) { 1, 2 => "small", 3 => "three", default => "other" };');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 2,
        "match literal evals should use internal EIR AOT functions:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "match literal eval caller reads should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "match literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only match literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only match literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only match literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "string:three");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies strict equality operators inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_strict_equality_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_strict_equality_eir_aot");
    let source = r#"<?php
echo eval('return ("10" === 10 ? "bad" : "S") . ":" . (true !== false ? "D" : "bad");');
echo ":";
$a = 10;
echo eval('return $a === 10 ? "same" : "bad";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 2,
        "strict-equality literal evals should use internal EIR AOT functions:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "strict-equality caller reads should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "strict-equality literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only strict-equality literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only strict-equality literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only strict-equality literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "S:D:same");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies logical xor inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_logical_xor_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_logical_xor_eir_aot");
    let source = r#"<?php
echo eval('return (true xor false) ? "T" : "bad";');
echo ":";
$flag = true;
echo eval('return ($flag xor true) ? "bad" : "F";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 2,
        "logical xor literal evals should use internal EIR AOT functions:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "logical xor caller reads should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "logical xor literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only logical xor literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only logical xor literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only logical xor literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "T:F");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies scalar casts inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_scalar_casts_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_scalar_casts_eir_aot");
    let source = r#"<?php
echo eval('return (int)"41" + 1;');
echo ":";
echo eval('return (string)7 . ":" . ((bool)"0" ? "bad" : "F") . ":" . (float)"1.25";');
echo ":";
$a = "40";
echo eval('return (int)$a + 2;');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 3,
        "scalar-cast literal evals should use internal EIR AOT functions:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "scalar-cast caller reads should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "scalar-cast literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only scalar-cast literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only scalar-cast literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only scalar-cast literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "42:7:F:1.25:42");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies division and exponentiation inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_division_and_pow_use_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_division_pow_eir_aot");
    let source = r#"<?php
echo eval('return 9 / 2;');
echo ":";
echo eval('return 2 ** 3 ** 2;');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 2,
        "division and exponentiation literal evals should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "division and exponentiation literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_context_new"),
        "native-only division/pow literal evals should not create an eval context:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_execute"),
        "division and exponentiation literal evals should not emit the eval bridge runtime:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "division and exponentiation literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "4.5:512");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compound division/modulo inside literal eval uses scope-aware EIR AOT.
#[test]
fn test_literal_eval_division_modulo_assign_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_division_modulo_assign_eir_aot");
    let source = r#"<?php
echo eval('$x = 20; $x /= 2; $x %= 6; return $x;');
echo ":" . $x;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "division/modulo assignment literal eval",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "4:4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies bitwise and shift operators inside literal eval lower through scope-aware EIR AOT.
#[test]
fn test_literal_eval_bitwise_shift_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_bitwise_shift_eir_aot");
    let source = r#"<?php
eval('echo (5 & 3) . ":" . (5 | 3) . ":" . (5 ^ 3) . ":" . (~0) . ":" . (1 << 4) . ":" . (-16 >> 2);
$x = 6; $x &= 3; echo ":" . $x;
$x = 4; $x |= 1; echo "," . $x;
$x = 7; $x ^= 3; echo "," . $x;
$x = 1; $x <<= 5; echo "," . $x;
$x = 64; $x >>= 3; echo "," . $x;');
echo ":" . $x;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "bitwise/shift literal eval",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "1:7:6:-1:16:-4:2,5,4,32,8:8");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies spaceship comparisons inside literal eval can lower through EIR AOT.
#[test]
fn test_literal_eval_spaceship_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_spaceship_eir_aot");
    let source = r#"<?php
echo eval('return 1 <=> 2;');
echo ":";
echo eval('return 2.5 <=> 2.5;');
echo ":";
$a = 12;
echo eval('return $a <=> 10;');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm
            .matches("eval literal AOT compiled EIR function")
            .count()
            >= 3,
        "spaceship literal evals should use EIR AOT:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "spaceship caller reads should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "spaceship literal evals should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only spaceship literal evals should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only spaceship literal evals should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only spaceship literal evals should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "-1:0:1");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies float literal eval returns can lower through an internal EIR AOT function.
#[test]
fn test_literal_eval_float_return_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_float_return_eir_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php echo eval('return 1.5 + 2.25;');",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "float literal eval return should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "float literal eval return should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only float literal eval should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only float literal eval should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only float literal eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3.75");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies no-scope literal eval control flow can lower through an internal EIR AOT function.
#[test]
fn test_literal_eval_if_without_scope_uses_eir_aot_function() {
    let dir = make_cli_test_dir("elephc_literal_eval_if_eir_aot");
    let source = r#"<?php
eval('if (true) { echo "yes"; } else { echo "no"; }');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval if without scope should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval if without scope should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval if should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval if should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval if should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "yes");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies scalar literal eval stores synchronize through a scope-aware EIR function.
#[test]
fn test_literal_eval_scalar_store_uses_scope_eir_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_aot_store_asm");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $existing = 1; eval('$created = \"yes\"; $existing = \"changed\";'); echo $created . ':' . $existing;",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "scalar literal eval store",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "yes:changed");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a literal eval return can read a caller local through the AOT scope path.
#[test]
fn test_literal_eval_scope_read_return_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_aot_scope_read_return_asm");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $a = 10; $unused = 99; echo eval('return $a + 20;'); echo ':' . $unused;",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled"),
        "literal eval scope read return should use the AOT path:\n{user_asm}"
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "literal eval scope read return should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "literal eval scope read return should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "literal eval scope read return should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "literal eval scope read return should not link elephc_magician: {required_libraries:?}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval scope read return should not call the interpreter bridge:\n{user_asm}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "30:99");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies runtime-safe static builtins can consume caller-scope values through EIR AOT.
#[test]
fn test_literal_eval_strlen_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_strlen_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$s = "abcd";
echo eval('return strlen($s);');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "strlen over a caller-scope value should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "strlen over a caller-scope value should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "strlen over a caller-scope value should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "strlen over a caller-scope value should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `intval()` can consume caller-scope values through direct-param EIR AOT.
#[test]
fn test_literal_eval_intval_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_intval_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$s = "42";
echo eval('return intval($s) + 8;');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "intval over a caller-scope value should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "intval over a caller-scope value should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "intval over a caller-scope value should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "intval over a caller-scope value should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "50");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `boolval()` can consume caller-scope values through direct-param EIR AOT.
#[test]
fn test_literal_eval_boolval_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_boolval_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$s = "0";
echo eval('return boolval($s) ? "bad" : "ok";');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "boolval over a caller-scope value should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "boolval over a caller-scope value should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "boolval over a caller-scope value should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "boolval over a caller-scope value should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `strval()` can consume caller-scope values through direct-param EIR AOT.
#[test]
fn test_literal_eval_strval_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_strval_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$s = false;
echo eval('return "[" . strval($s) . "]";');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "strval over a caller-scope value should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "strval over a caller-scope value should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "strval over a caller-scope value should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "strval over a caller-scope value should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "[]");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies scalar type probes can consume caller-scope values through direct-param EIR AOT.
#[test]
fn test_literal_eval_type_probes_scope_read_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_type_probes_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$i = 42;
$f = 1.5;
$b = false;
$n = null;
$s = "hi";
$items = [1, 2];
$o = json_decode("{}");
echo eval('return gettype($i) . ":" .
    gettype($items) . ":" .
    gettype($o) . ":" .
    (is_integer($i) ? "I" : "bad") .
    (is_double($f) ? "D" : "bad") .
    (is_bool($b) ? "B" : "bad") .
    (is_null($n) ? "N" : "bad") .
    (is_scalar($s) ? "S" : "bad") .
    (is_scalar($items) ? "bad" : "A") .
    (is_scalar($o) ? "bad" : "O") .
    (is_string($s) ? "T" : "bad");');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "type probes over caller-scope values should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "type probes over caller-scope values should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "type probes over caller-scope values should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "type probes over caller-scope values should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "integer:array:object:IDBNSAOT");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_object()` can consume caller-scope values through direct-param EIR AOT.
#[test]
fn test_literal_eval_is_object_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_is_object_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$o = json_decode("{}");
$i = 42;
echo eval('return (is_object($o) ? "O" : "bad") . ":" . (is_object($i) ? "bad" : "I");');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "is_object over caller-scope values should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "is_object over caller-scope values should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "is_object over caller-scope values should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "is_object over caller-scope values should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "O:I");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_array()` can inspect caller-scope arrays through direct-param EIR AOT.
#[test]
fn test_literal_eval_is_array_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_is_array_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$items = [1, 2];
$n = 42;
echo eval('return (is_array($items) ? "A" : "bad") . ":" . (is_array($n) ? "bad" : "N");');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "is_array over caller-scope values should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "is_array over caller-scope values should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_context_new"),
        "is_array over caller-scope values should not allocate an eval bridge context:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_execute"),
        "is_array over caller-scope values should not emit the interpreter bridge:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "is_array over caller-scope values should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A:N");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_iterable()` can inspect caller-scope arrays and Iterator objects directly.
#[test]
fn test_literal_eval_is_iterable_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_is_iterable_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class EvalAotDirectIterator implements Iterator {
    private int $i = 0;
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
    public function valid(): bool { return $this->i < 0; }
    public function rewind(): void { $this->i = 0; }
}
$items = [1, 2];
$iterator = new EvalAotDirectIterator();
$n = 42;
echo eval('return (is_iterable($items) ? "A" : "bad") .
    (is_iterable($iterator) ? "I" : "bad") .
    (is_iterable($n) ? "bad" : "N");');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "is_iterable over caller-scope values should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "is_iterable over caller-scope values should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_context_new"),
        "is_iterable over caller-scope values should not allocate an eval bridge context:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_execute"),
        "is_iterable over caller-scope values should not emit the interpreter bridge:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "is_iterable over caller-scope values should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "AIN");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_numeric()` and `is_resource()` use direct-param EIR AOT for scope reads.
#[test]
fn test_literal_eval_numeric_resource_probes_scope_read_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_numeric_resource_probe_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = "42";
$s = "abc";
$h = fopen("php://memory", "r+");
echo eval('return (is_numeric($n) ? "N" : "bad") .
    (is_numeric($s) ? "bad" : "S") . ":" .
    (is_resource($h) ? "H" : "bad");');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "numeric/resource probes over caller-scope values should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "numeric/resource probes over caller-scope values should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "numeric/resource probes over caller-scope values should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "numeric/resource probes over caller-scope values should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "NS:H");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies IEEE float predicates can use eval-local numeric values without the bridge.
#[test]
fn test_literal_eval_float_predicates_local_values_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_float_predicates_local_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
echo eval('$nan = NAN; $inf = INF; $num = 2.5;
return (is_nan($nan) ? "N" : "bad") .
    (is_infinite($inf) ? "I" : "bad") .
    (is_finite($num) ? "F" : "bad") .
    (is_finite($inf) ? "bad" : "f");');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "float predicates over eval-local values should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "float predicates over eval-local values should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "float predicate locals should flush created variables through scope helpers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_context_new"),
        "float predicates over eval-local values should not create an eval bridge context:\n{user_asm}"
    );
    assert!(
        runtime_asm.contains("__elephc_eval_scope_set"),
        "float predicate locals should emit core eval scope helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "float predicates over eval-local values should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "NIFf");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies IEEE float predicates can use numeric caller-scope values without the bridge.
#[test]
fn test_literal_eval_float_predicates_scope_numeric_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_float_predicates_scope_numeric_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$nan = NAN;
$inf = INF;
$num = 2.5;
$i = 7;
echo eval('return (is_nan($nan) ? "N" : "bad") .
    (is_infinite($inf) ? "I" : "bad") .
    (is_finite($num) ? "F" : "bad") .
    (is_finite($i) ? "i" : "bad");');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "float predicates over numeric caller values should use direct-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "float predicates over numeric caller values should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_scope_get"),
        "float predicates over numeric caller values should not use eval scope helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_execute"),
        "float predicates over numeric caller values should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "float predicates over numeric caller values should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "NIFi");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies float predicates do not use direct params for caller strings that may TypeError.
#[test]
fn test_literal_eval_float_predicates_scope_string_use_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_float_predicate_scope_string_fallback");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$s = "abc";
echo eval('return is_finite($s) ? "bad" : "ok";');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT fallback"),
        "float predicate over caller string should stay on bridge fallback:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "float predicate over caller string should call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_context_new"),
        "float predicate fallback should create an eval bridge context:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "float predicate fallback should link elephc_magician: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_array()` can inspect eval-local arrays without bridge support.
#[test]
fn test_literal_eval_is_array_local_array_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_is_array_local_array_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
echo eval('$a = [1, 2]; return is_array($a) ? "A" : "bad";');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "is_array over eval-local arrays should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "is_array over eval-local arrays should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "is_array over eval-local arrays should flush created locals through scope helpers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_context_new"),
        "is_array over eval-local arrays should not create an eval bridge context:\n{user_asm}"
    );
    assert!(
        runtime_asm.contains("__elephc_eval_scope_set"),
        "is_array over eval-local arrays should emit core eval scope helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "is_array over eval-local arrays should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_iterable()` can inspect eval-local arrays without bridge support.
#[test]
fn test_literal_eval_is_iterable_local_array_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_is_iterable_local_array_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
echo eval('$a = [1, 2]; return is_iterable($a) ? "T" : "bad";');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "is_iterable over eval-local arrays should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "is_iterable over eval-local arrays should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "is_iterable over eval-local arrays should flush created locals through scope helpers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_context_new"),
        "is_iterable over eval-local arrays should not create an eval bridge context:\n{user_asm}"
    );
    assert!(
        runtime_asm.contains("__elephc_eval_scope_set"),
        "is_iterable over eval-local arrays should emit core eval scope helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "is_iterable over eval-local arrays should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "T");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `floatval()` can consume caller-scope values through direct-param EIR AOT.
#[test]
fn test_literal_eval_floatval_scope_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_floatval_scope_read_aot");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$s = "1.5";
echo eval('return floatval($s) + 2.25;');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "floatval over a caller-scope value should use direct read-param EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "floatval over a caller-scope value should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "floatval over a caller-scope value should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "floatval over a caller-scope value should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3.75");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies float caller locals can be passed into read-only literal eval AOT without scope helpers.
#[test]
fn test_literal_eval_float_scope_read_uses_direct_params_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_float_scope_read_direct_params");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $a = 1.5; echo eval('return $a + 2.25;');",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "float scope-read literal eval should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "float scope-read literal eval should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "float scope-read literal eval should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "float scope-read literal eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3.75");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a literal eval integer read-modify-write uses scope-aware EIR AOT.
#[test]
fn test_literal_eval_scope_read_write_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_aot_scope_read_write_asm");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $x = 1; eval('$x = $x + 1;'); echo $x;",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval integer read-modify-write",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "2");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies float read-modify-write literal eval uses scope-aware EIR AOT.
#[test]
fn test_literal_eval_float_scope_read_write_uses_scope_eir_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_float_scope_read_write_eir");
    let (user_asm, runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $x = 20.0; eval('$x = $x / 2;'); echo $x;",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "float literal eval read-modify-write",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "10");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies local `isset()` and `empty()` probes lower through scope-aware EIR AOT.
#[test]
fn test_literal_eval_local_isset_empty_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_local_isset_empty_aot");
    let source = r#"<?php
eval('$zero = 0;
$blank = "";
$value = "x";
$nullish = null;
echo isset($zero, $blank) ? "I" : "i";
echo isset($nullish) ? "N" : "n";
echo empty($zero) ? "Z" : "z";
echo empty($blank) ? "B" : "b";
echo empty($value) ? "V" : "v";
echo empty($nullish) ? "L" : "l";');
echo ":" . $value;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval local isset/empty",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "InZBvL:x");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies caller-scope `isset()` and `empty()` probes use direct read-param EIR AOT.
#[test]
fn test_literal_eval_scope_isset_empty_uses_direct_params_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_scope_isset_empty_direct_params");
    let source = r#"<?php
$zero = 0;
$blank = "";
$nullish = null;
eval('echo isset($missing) ? "bad" : "m";
echo isset($nullish) ? "bad" : "n";
echo isset($zero, $blank) ? "I" : "bad";
echo empty($missing) ? "M" : "bad";
echo empty($nullish) ? "N" : "bad";
echo empty($zero) ? "Z" : "bad";
echo empty($blank) ? "B" : "bad";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with direct read params"),
        "literal eval scope isset/empty should use direct read params:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "literal eval scope isset/empty should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "literal eval scope isset/empty should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "literal eval scope isset/empty should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "mnIMNZB");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `print` expressions inside literal eval lower through scope-aware EIR AOT.
#[test]
fn test_literal_eval_print_expression_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_print_expr_aot");
    let source = r#"<?php
eval('$x = print "A";
echo ":";
echo print "B";
echo ":" . $x;');
echo ":" . $x;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval print expression",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "A:B1:1:1");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies error-suppressed scalar expressions use scope-aware EIR AOT.
#[test]
fn test_literal_eval_error_suppress_expression_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_error_suppress_expr_aot");
    let source = r#"<?php
eval('echo @strlen("ab");
$x = @intval("4");
echo ":" . $x;');
echo ":" . $x;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval error suppression",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "2:4:4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a literal eval while loop uses a scope-aware internal EIR function.
#[test]
fn test_literal_eval_local_while_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_local_while_aot");
    let source = r#"<?php
eval('$sum = 0;
$i = 1;
while ($i <= 10) {
    $sum += $i;
    $i += 1;
}
echo $sum;');
echo ":" . $sum;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval while loop",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "55:55");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `do/while` loops inside literal eval can lower through the EIR AOT path.
#[test]
fn test_literal_eval_do_while_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_do_while_aot");
    let source = r#"<?php
eval('$i = 0;
do {
    $i = $i + 1;
    if ($i == 2) { continue; }
    echo $i;
} while ($i < strlen("abc"));');
echo ":" . $i;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval do/while loop",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "13:3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `for` loops inside literal eval can lower through the EIR AOT path.
#[test]
fn test_literal_eval_for_loop_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_for_loop_aot");
    let source = r#"<?php
eval('for ($i = 0; $i < strlen("abcde"); $i = $i + 1) {
    if ($i == 1) { continue; }
    if ($i == 3) { break; }
    echo $i;
}');
echo ":" . $i;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval for loop",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "02:3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies local increment/decrement expressions inside literal eval lower through EIR AOT.
#[test]
fn test_literal_eval_local_inc_dec_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_local_inc_dec_aot");
    let source = r#"<?php
eval('$i = 1;
$i++;
++$i;
$i--;
--$i;
echo $i;
for ($j = 0; $j < 3; $j++) { echo $j; }
for ($k = 3; $k > 0; --$k) { echo $k; }');
echo ":" . $i . ":" . $j . ":" . $k;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval local increment/decrement",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "1012321:1:3:0");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies supported `switch` statements lower through scope-aware EIR AOT.
#[test]
fn test_literal_eval_switch_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_switch_aot");
    let source = r#"<?php
eval('$x = 2;
switch ($x) {
    case strlen("ab"): echo "2"; break;
    default: echo "d";
}
$x = 3;
switch ($x) {
    case 2: echo "F"; break;
    default: echo "D";
}');
echo ":" . $x;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval switch",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "2D:3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies default-before-case switch fallthrough lowers through scope-aware EIR AOT.
#[test]
fn test_literal_eval_switch_default_before_case_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_switch_default_before_case_aot");
    let source = r#"<?php
eval('$x = 2;
switch ($x) {
    default: echo "d";
    case 2: echo "2"; break;
}
$x = 3;
switch ($x) {
    default: echo "D";
    case 2: echo "F"; break;
}');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "default-before-case literal eval switch",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "2DF");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies no-scope default-before-case switch evals lower through an EIR function.
#[test]
fn test_literal_eval_switch_default_before_case_no_scope_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_switch_default_before_case_eir_aot");
    let source = r#"<?php
eval('switch (3) {
    default: echo "D";
    case 2: echo "F"; break;
}
switch (2) {
    default: echo "d";
    case 2: echo "f"; break;
}');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "no-scope default-before-case switch should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("eval literal AOT compiled local scalar"),
        "no-scope default-before-case switch should not use the legacy eval mini-backend:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "no-scope default-before-case switch should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "no-scope default-before-case switch should not reference eval runtime helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "no-scope default-before-case switch should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "no-scope default-before-case switch should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "DFf");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `continue` inside a literal-eval switch preserves PHP target levels.
#[test]
fn test_literal_eval_switch_continue_uses_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_switch_continue_aot");
    let source = r#"<?php
eval('for ($i = 0; $i < 3; $i = $i + 1) {
    switch ($i) {
        case 0:
            echo "a";
            continue;
        case 1:
            echo "b";
            continue 2;
        default:
            echo "c";
    }
    echo "d";
}');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "literal eval switch continue",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "adbcd");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies foreach over static arrays lowers through EIR AOT without the interpreter bridge.
#[test]
fn test_literal_eval_static_foreach_uses_eir_aot_scope_helpers() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_foreach_aot");
    let source = r#"<?php
	eval('foreach ([1, 2, 3] as $item) { echo $item; }');
	echo ":" . $item;
echo "|";
eval('foreach (["a" => 1, "b" => 2] as $key => $value) { echo $key . ":" . $value . ";"; }');
echo ":" . $key . ":" . $value;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with eval scope"),
        "static foreach eval should use the scope-aware EIR AOT function path:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "static foreach eval should synchronize loop variables through eval scope helpers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "static foreach eval should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_execute"),
        "static foreach eval should not emit the eval execute runtime bridge:\n{runtime_asm}"
    );
    assert!(
        runtime_asm.contains("__elephc_eval_scope_set"),
        "static foreach eval should emit core eval scope helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "scope-only static foreach eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "123:3|a:1;b:2;:b:2");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies static empty foreach in eval uses AOT without publishing loop locals.
#[test]
fn test_literal_eval_static_empty_foreach_uses_eir_aot_without_scope_helpers() {
    let dir = make_cli_test_dir("elephc_literal_eval_static_empty_foreach_aot");
    let source = r#"<?php
	$kept = "keep";
	eval('foreach ([] as $kept) { echo "bad"; }');
	echo $kept;
	"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "static empty foreach eval should use EIR AOT:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "static empty foreach eval should not use eval bridge/scope helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "static empty foreach eval should not emit eval bridge/scope runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "static empty foreach eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "keep");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies foreach over caller-scope arrays uses scope-aware EIR AOT without the bridge.
#[test]
fn test_literal_eval_foreach_scope_array_uses_eir_aot_scope_helpers() {
    let dir = make_cli_test_dir("elephc_literal_eval_foreach_scope_array_aot");
    let source = r#"<?php
$items = ["a", "b"];
$item = "old";
eval('foreach ($items as $item) { echo $item; }');
echo ":" . $item . "|";
$pairs = ["x" => 10, "y" => 20];
$key = "old-key";
$value = 0;
eval('foreach ($pairs as $key => $value) { echo $key . ":" . $value . ";"; }');
echo ":" . $key . ":" . $value . "|";
$empty = [];
$kept = "keep";
eval('foreach ($empty as $kept) { echo "bad"; }');
echo $kept;
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function with eval scope"),
        "foreach over caller-scope arrays should use the scope-aware EIR AOT function path:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_get"),
        "foreach over caller-scope arrays should read the source through eval scope helpers:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "foreach over caller-scope arrays should synchronize loop variables through eval scope helpers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "foreach over caller-scope arrays should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_execute"),
        "foreach over caller-scope arrays should not emit the eval execute runtime bridge:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "scope-only foreach over caller-scope arrays should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "ab:b|x:10;y:20;:y:20|keep");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies foreach over a caller scalar keeps the bridge fallback.
#[test]
fn test_literal_eval_foreach_scope_scalar_keeps_bridge_fallback() {
    let dir = make_cli_test_dir("elephc_literal_eval_foreach_scope_scalar_bridge");
    let source = r#"<?php
$n = 42;
eval('foreach ($n as $item) { echo $item; }');
"#;
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "foreach over a caller scalar should keep the interpreter bridge fallback:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "foreach over a caller scalar should link elephc_magician for fallback: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the prime-sum benchmark fragment uses scope-aware EIR AOT.
#[test]
fn test_literal_eval_prime_loop_uses_aot_without_execute_bridge() {
    let dir = make_cli_test_dir("elephc_literal_eval_prime_loop_aot");
    let source = r#"<?php
eval('$sum = 0;
$n = 2;
while ($n <= 100000) {
    $is_prime = true;
    $d = 2;
    while ($d * $d <= $n) {
        if ($n % $d == 0) {
            $is_prime = false;
            break;
        }
        $d += 1;
    }
    if ($is_prime) {
        $sum += $n;
    }
    $n += 1;
}
echo $sum;');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert_scope_eir_aot_without_bridge(
        &user_asm,
        &runtime_asm,
        &required_libraries,
        "prime loop literal eval",
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "454396537");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies dynamic eval code is not marked as a literal AOT candidate.
#[test]
fn test_dynamic_eval_does_not_emit_literal_aot_marker() {
    let dir = make_cli_test_dir("elephc_dynamic_eval_no_aot_marker");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $code = '$x = 1;'; eval($code);",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        user_asm.contains("__elephc_eval_execute"),
        "dynamic eval should still call the eval bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("eval literal AOT"),
        "dynamic eval must not be marked as a literal AOT path:\n{user_asm}"
    );
    assert!(
        required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "dynamic eval should still require elephc_magician: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies programs without `eval` do not link or reference the optional eval bridge.
#[test]
fn test_non_eval_program_does_not_request_eval_bridge() {
    let dir = make_cli_test_dir("elephc_no_eval_bridge_asm");
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo 1 + 2;", &dir, 8_388_608, false, false);
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "non-eval user assembly should not reference eval bridge:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "non-eval runtime assembly should not reference eval bridge:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "non-eval required libraries should not include elephc_magician: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies non-eval builtin probes stay static and do not request the eval bridge.
#[test]
fn test_non_eval_builtin_probes_remain_static_without_eval_bridge() {
    let dir = make_cli_test_dir("elephc_no_eval_builtin_static_contract");
    let source = r#"<?php
namespace EvalNoBridgeContract;
echo function_exists("strlen") ? "F" : "f";
echo function_exists("STRLEN") ? "C" : "c";
echo ":";
echo STRLEN("abcd");
echo ":";
echo \strlen("xy");
echo ":";
echo ChOp("value\f");
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "non-eval builtin contract should not reference eval bridge:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "non-eval builtin contract runtime should not reference eval bridge:\n{runtime_asm}"
    );
    assert_eq!(
        required_libraries
            .iter()
            .filter(|lib| lib.as_str() == "elephc_magician")
            .count(),
        0,
        "non-eval builtin contract should not link elephc_magician: {required_libraries:?}"
    );

    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "FC:4:2:value");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies multiple eval calls still request the optional bridge exactly once.
#[test]
fn test_eval_runtime_feature_links_magician_once() {
    let dir = make_cli_test_dir("elephc_eval_bridge_once");
    // Dynamic eval arguments force the interpreter bridge; literal fragments
    // would compile through the AOT paths without linking magician at all.
    let (_user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        "<?php $one = '$a = 1;'; $two = '$b = 2;'; eval($one); eval($two); eval('$c = $a + $b;');",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert_eq!(
        required_libraries
            .iter()
            .filter(|lib| lib.as_str() == "elephc_magician")
            .count(),
        1,
        "eval bridge should be linked exactly once: {required_libraries:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies literal eval can execute scalar echo fragments through the AOT path.
#[test]
fn test_eval_scalar_echo_executes_through_aot() {
    let out = compile_and_run("<?php eval('echo \"x\";');");
    assert_eq!(out, "x");
}

/// Verifies comma echo lists and print statements inside literal eval use EIR AOT.
#[test]
fn test_literal_eval_echo_comma_and_print_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_echo_comma_print_aot");
    let source = r#"<?php
eval('echo "a", "b", "c";');
echo ":";
eval('print "x";');
echo ":";
echo eval('return print "y";');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval echo comma/print should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval echo comma/print should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval echo comma/print should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval echo comma/print should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval echo comma/print should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "abc:x:y1");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies eval output uses scalar fast paths while preserving object string contexts.
#[test]
fn test_eval_output_fast_path_preserves_scalar_and_object_echo() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalOutputFastPathBox {
    public function __toString() { return "obj"; }
}
echo 12; echo ":";
echo 2.5; echo ":";
echo true; echo ":";
echo false; echo ":";
echo new EvalOutputFastPathBox(); echo ":";
print 7;');
"#,
    );
    assert_eq!(out, "12:2.5:1::obj:7");
}

/// Verifies eval `print_r()` writes supported values and returns true.
#[test]
fn test_eval_dispatches_print_r_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('print_r("x"); echo ":";
print_r(value: false); echo ":";
print_r([1, 2]); echo ":";
$call = call_user_func("print_r", true);
$spread = call_user_func_array("print_r", ["value" => "z"]);
echo ":" . ($call ? "call" : "bad") . ":" . ($spread ? "spread" : "bad") . ":";
echo function_exists("print_r");');
"#,
    );
    assert_eq!(out, "x::Array\n(\n    [0] => 1\n    [1] => 2\n)\n:1z:call:spread:1");
}

/// Verifies eval `var_dump()` writes PHP-style diagnostics and returns null.
#[test]
fn test_eval_dispatches_var_dump_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('var_dump(42);
var_dump("hi");
var_dump(false);
var_dump(null);
var_dump([10, 20]);
var_dump(["x" => true]);
$call = call_user_func("var_dump", 3.5);
$spread = call_user_func_array("var_dump", ["value" => "z"]);
echo ($call === null ? "call-null" : "bad") . ":" . ($spread === null ? "spread-null" : "bad") . ":";
echo function_exists("var_dump");');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(42)\n",
            "string(2) \"hi\"\n",
            "bool(false)\n",
            "NULL\n",
            "array(2) {\n",
            "  [0]=>\n",
            "  int(10)\n",
            "  [1]=>\n",
            "  int(20)\n",
            "}\n",
            "array(1) {\n",
            "  [\"x\"]=>\n",
            "  bool(true)\n",
            "}\n",
            "float(3.5)\n",
            "string(1) \"z\"\n",
            "call-null:spread-null:1",
        )
    );
}

/// Verifies eval `var_dump()` prints eval-declared and generated object class names.
#[test]
fn test_eval_var_dump_prints_object_class_names() {
    let out = compile_and_run(
        r#"<?php
class EvalAotDumpBox {}
eval('class EvalDynamicDumpBox {}
var_dump(new EvalDynamicDumpBox());
var_dump(new EvalAotDumpBox());');
"#,
    );
    // Object ids are runtime handles and vary per run; normalize them before
    // comparing the PHP-style dump shape.
    let normalized: String = {
        let mut result = String::new();
        let mut chars = out.chars().peekable();
        while let Some(ch) = chars.next() {
            result.push(ch);
            if ch == '#' {
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
                result.push('N');
            }
        }
        result
    };
    assert_eq!(
        normalized,
        "object(EvalDynamicDumpBox)#N (0) {\n}\nobject(EvalAotDumpBox)#N (0) {\n}\n"
    );
}

/// Verifies eval fragments with comments and parser-lowered `__LINE__` use EIR AOT.
#[test]
fn test_literal_eval_comments_and_line_magic_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_comments_line_aot");
    let source = r#"<?php
echo eval("// leading\n# hash\n/* block\ncomment */ return __LINE__;");
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval comments and __LINE__ should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval comments and __LINE__ should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval comments and __LINE__ should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval comments and __LINE__ should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval comments and __LINE__ should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies eval coerces null to an empty fragment and returns null.
#[test]
fn test_eval_null_argument_is_empty_fragment() {
    let out = compile_and_run("<?php echo eval(null);");
    assert_eq!(out, "");
}

/// Verifies non-string scalar eval arguments are coerced before runtime parsing.
#[test]
fn test_eval_integer_argument_is_coerced_then_parse_checked() {
    let err = compile_and_run_expect_failure("<?php eval(123);");
    assert!(
        err.contains("Parse error: eval() fragment is invalid"),
        "stderr did not contain eval parse diagnostic: {err}"
    );
}

/// Verifies literal eval can execute simple integer arithmetic through the AOT scalar path.
#[test]
fn test_eval_scalar_add_executes_through_aot() {
    let out = compile_and_run("<?php eval('echo 2 + 3 * 4 - 5;');");
    assert_eq!(out, "9");
}

/// Verifies eval integer store expressions can execute through the unboxed temporary path.
#[test]
fn test_eval_unboxed_integer_store_expression_executes_through_bridge() {
    let out = compile_and_run(
        "<?php eval('$value = 1; $i = 4; $value = ($value * 3 + $i) % 1000003; echo $value;');",
    );
    assert_eq!(out, "7");
}

/// Verifies cached straight-line eval fragments use the prepared linear path while loops fall back.
#[test]
fn test_eval_linear_cached_fragment_and_evalir_fallback_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
$sum = 0;
$i = 0;
$fragment = '$sum = $sum + 3;';
while ($i < 5) {
    eval($fragment);
    $i += 1;
}
echo $sum;
echo ":";
echo eval('$x = 1; $x = $x + 2; return $x;');
echo ":";
echo eval('$x = 0; while ($x < 3) { $x = $x + 1; } return $x;');
"#,
    );
    assert_eq!(out, "15:3:3");
}

/// Verifies eval division and modulo execute through target-specific bridge wrappers.
#[test]
fn test_eval_division_modulo_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return 9 / 2;');
echo ":";
echo eval('return 10 % 4;');
echo ":";
eval('$x = 20; $x /= 2; $x %= 6; echo $x;');
"#,
    );
    assert_eq!(out, "4.5:2:4");
}

/// Verifies eval exponentiation executes through the target-specific bridge wrapper.
#[test]
fn test_eval_exponentiation_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return 2 ** 3 ** 2;');
echo ":";
echo eval('return -2 ** 2;');
echo ":";
eval('$x = 2; $x **= 3; echo $x;');
"#,
    );
    assert_eq!(out, "512:-4:8");
}

/// Verifies eval integer bitwise and shift operators execute through bridge wrappers.
#[test]
fn test_eval_bitwise_shift_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo (5 & 3) . ":" . (5 | 3) . ":" . (5 ^ 3) . ":" . (~0) . ":" . (1 << 4) . ":" . (-16 >> 2);');
echo ":";
eval('$x = 6; $x &= 3; echo $x; echo ","; $x = 4; $x |= 1; echo $x; echo ","; $x = 7; $x ^= 3; echo $x; echo ","; $x = 1; $x <<= 5; echo $x; echo ","; $x = 64; $x >>= 3; echo $x;');
"#,
    );
    assert_eq!(out, "1:7:6:-1:16:-4:2,5,4,32,8");
}

/// Verifies the eval bridge routes concatenation through runtime string helpers.
#[test]
fn test_eval_scalar_concat_executes_through_bridge() {
    let out = compile_and_run("<?php eval('echo \"a\" . \"b\";');");
    assert_eq!(out, "ab");
}

/// Verifies eval comparison operators return boxed booleans through the bridge.
#[test]
fn test_eval_scalar_comparisons_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo 2 < 3; echo 3 <= 3; echo 4 > 3; echo 4 >= 4; echo 5 != 6; echo 7 == 7;');
"#,
    );
    assert_eq!(out, "111111");
}

/// Verifies eval spaceship comparisons return boxed -1/0/1 integers.
#[test]
fn test_eval_spaceship_executes() {
    let out = compile_and_run(
        r#"<?php
eval('echo 1 <=> 2; echo ":"; echo 2 <=> 2; echo ":"; echo 3 <=> 2;');
"#,
    );
    assert_eq!(out, "-1:0:1");
}

/// Verifies loose scalar equality in eval handles strings and null/empty-string rules.
#[test]
fn test_eval_scalar_loose_equality_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo "a" == "a"; echo "a" != "b"; echo "" == null; echo "10" == 10; echo "foo" != 0; echo "10" == "1e1";');
"#,
    );
    assert_eq!(out, "111111");
}

/// Verifies strict scalar equality in eval preserves PHP type identity.
#[test]
fn test_eval_scalar_strict_equality_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('echo "10" == 10; echo "10" === 10; echo "10" === "10"; echo "10" !== 10; echo null === null;');
"#,
    );
    assert_eq!(out, "1111");
}

/// Verifies eval logical operators short-circuit before evaluating unsupported RHS calls.
#[test]
fn test_eval_logical_operators_short_circuit() {
    let out = compile_and_run(
        r#"<?php
echo "a" . eval('return false && missing_eval_rhs();') . "b";
echo ":";
echo eval('return true || missing_eval_rhs();');
"#,
    );
    assert_eq!(out, "ab:1");
}

/// Verifies eval supports PHP logical keyword operators with PHP precedence.
#[test]
fn test_eval_logical_keyword_operators_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return (false || true and false) ? "T" : "F";');
echo ":";
echo eval('return (true xor false) ? "T" : "F";');
echo ":";
echo eval('return (true xor true) ? "T" : "F";');
echo ":";
echo eval('return true or missing_eval_rhs();');
"#,
    );
    assert_eq!(out, "F:T:F:1");
}

/// Verifies eval logical negation returns PHP boolean cells through the bridge.
#[test]
fn test_eval_logical_not_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return !false;');
echo ":";
echo eval('return !"x";');
"#,
    );
    assert_eq!(out, "1:");
}

/// Verifies eval ternary operators short-circuit and return the selected branch.
#[test]
fn test_eval_ternary_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return true ? "yes" : missing_eval_rhs();');
echo ":";
echo eval('return false ? missing_eval_rhs() : "no";');
echo ":";
echo eval('return "x" ?: "fallback";');
echo ":";
echo eval('return false ?: "fallback";');
"#,
    );
    assert_eq!(out, "yes:no:x:fallback");
}

/// Verifies eval null coalescing returns defaults only for missing or null values.
#[test]
fn test_eval_null_coalesce_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
echo eval('return $missing ?? "fallback";');
echo ":";
echo eval('$x = null; return $x ?? "null-fallback";');
echo ":";
echo eval('return "set" ?? missing_eval_rhs();');
"#,
    );
    assert_eq!(out, "fallback:null-fallback:set");
}

/// Verifies eval unary numeric operators execute through runtime numeric helpers.
#[test]
fn test_eval_unary_numeric_operators_execute_through_bridge() {
    let out = compile_and_run("<?php echo eval('return -5 + +2;');");
    assert_eq!(out, "-3");
}

/// Verifies eval simple variable compound assignments execute through existing value hooks.
#[test]
fn test_eval_compound_assignment_executes_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 2; $x += 3; $x *= 4; $x -= 5; $s = "v"; $s .= $x; echo $s;');
echo ":";
eval('for ($i = 0; $i < 3; $i += 1) { echo $i; }');
"#,
    );
    assert_eq!(out, "v15:012");
}

/// Verifies eval simple variable increment and decrement statements execute in loops.
#[test]
fn test_eval_inc_dec_statements_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('$i = 1; $i++; ++$i; $i--; --$i; echo $i;');
echo ":";
eval('for ($j = 0; $j < 3; $j++) { echo $j; }');
echo ":";
eval('for ($k = 3; $k > 0; --$k) { echo $k; }');
"#,
    );
    assert_eq!(out, "1:012:321");
}

/// Verifies eval if/else branches use PHP truthiness and update the caller scope.
#[test]
fn test_eval_if_else_updates_scope() {
    let out = compile_and_run(
        r#"<?php
$flag = "0";
eval('if ($flag) { $result = "then"; } else { $result = "else"; }');
echo $result;
"#,
    );
    assert_eq!(out, "else");
}

/// Verifies eval elseif chains execute the first truthy branch.
#[test]
fn test_eval_elseif_updates_scope() {
    let out = compile_and_run(
        r#"<?php
eval('if (false) { $result = "a"; } elseif (true) { $result = "b"; } else { $result = "c"; }');
echo $result;
"#,
    );
    assert_eq!(out, "b");
}

/// Verifies eval accepts PHP's separate `else if` spelling.
#[test]
fn test_eval_else_if_updates_scope() {
    let out = compile_and_run(
        r#"<?php
eval('if (false) { $result = "a"; } else if (true) { $result = "b"; } else { $result = "c"; }');
echo $result;
"#,
    );
    assert_eq!(out, "b");
}

/// Verifies eval accepts braceless single-statement control-flow bodies.
#[test]
fn test_eval_braceless_control_flow_bodies() {
    let out = compile_and_run(
        r#"<?php
$flag = false;
eval('if ($flag) echo "a"; else echo "b"; while (false) echo "x"; do echo "d"; while (false);');
"#,
    );
    assert_eq!(out, "bd");
}

/// Verifies eval while loops repeatedly execute against the materialized scope.
#[test]
fn test_eval_while_updates_scope() {
    let out = compile_and_run(
        r#"<?php
$i = 3;
eval('while ($i) { echo $i; $i = $i - 1; }');
echo $i;
"#,
    );
    assert_eq!(out, "3210");
}

/// Verifies eval do/while loops execute the body before checking the condition.
#[test]
fn test_eval_do_while_runs_body_before_condition() {
    let out = compile_and_run(
        r#"<?php
$i = 0;
eval('do { echo $i; $i = $i + 1; } while (false);');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "0:1");
}

/// Verifies eval switch supports matching, default fallback, and fallthrough.
#[test]
fn test_eval_switch_matches_default_and_fallthrough() {
    let out = compile_and_run(
        r#"<?php
eval('$x = 2; switch ($x) { default: echo "d"; case 2: echo "2"; break; } $x = 3; switch ($x) { default: echo "D"; case 2: echo "F"; break; }');
"#,
    );
    assert_eq!(out, "2DF");
}

/// Verifies eval match expressions use strict comparisons and lazy result arms.
#[test]
fn test_eval_match_expression_dispatches_strict_arms() {
    let out = compile_and_run(
        r#"<?php
eval('$x = "1";
echo match ($x) { 1 => "int", "1" => "string", default => "other" };
echo ":";
echo match (3) { 1, 2 => missing(), default => "fallback" };');
"#,
    );
    assert_eq!(out, "string:fallback");
}

/// Verifies break and continue control a loop interpreted inside eval.
#[test]
fn test_eval_break_and_continue_control_loop() {
    let out = compile_and_run(
        r#"<?php
$i = 3;
eval('while ($i) { $i = $i - 1; if ($i) { continue; } echo "done"; break; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "done:0");
}

/// Verifies `for` loops inside eval run init, body, update, and condition in order.
#[test]
fn test_eval_for_loop_updates_scope() {
    let out = compile_and_run(
        r#"<?php
eval('for ($i = 3; $i; $i = $i - 1) { echo $i; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "321:0");
}

/// Verifies `continue` inside an eval `for` loop still runs the update clause.
#[test]
fn test_eval_for_continue_runs_update() {
    let out = compile_and_run(
        r#"<?php
eval('for ($i = 3; $i; $i = $i - 1) { if ($i - 1) { continue; } echo "done"; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "done:0");
}

/// Verifies eval `for` conditions can use ordered comparisons.
#[test]
fn test_eval_for_loop_uses_less_than_condition() {
    let out = compile_and_run(
        r#"<?php
eval('for ($i = 0; $i < 3; $i = $i + 1) { echo $i; }');
echo ":" . $i;
"#,
    );
    assert_eq!(out, "012:3");
}

/// Verifies value-only foreach loops inside eval iterate indexed array values.
#[test]
fn test_eval_foreach_iterates_indexed_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach ([1, 2, 3] as $item) { echo $item; }');
echo ":" . $item;
"#,
    );
    assert_eq!(out, "123:3");
}

/// Verifies key-value foreach loops inside eval expose indexed array positions.
#[test]
fn test_eval_foreach_iterates_indexed_keys_and_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach ([10, 20] as $key => $item) { echo $key . ":" . $item . ";"; }');
echo "|" . $key . ":" . $item;
"#,
    );
    assert_eq!(out, "0:10;1:20;|1:20");
}

/// Verifies eval foreach can iterate an indexed array from the caller scope.
#[test]
fn test_eval_foreach_reads_scope_array() {
    let out = compile_and_run(
        r#"<?php
$items = eval('return ["a", "b"];');
eval('foreach ($items as $item) { echo $item; }');
"#,
    );
    assert_eq!(out, "ab");
}

/// Verifies break and continue control value-only foreach loops inside eval.
#[test]
fn test_eval_foreach_honors_break_and_continue() {
    let out = compile_and_run(
        r#"<?php
eval('foreach ([1, 2, 3] as $item) { if ($item == 1) { continue; } echo $item; break; }');
echo ":" . $item;
"#,
    );
    assert_eq!(out, "2:2");
}

/// Verifies foreach inside eval drives AOT Iterator and IteratorAggregate objects.
#[test]
fn test_eval_foreach_iterates_aot_traversable_objects() {
    let out = compile_and_run(
        r#"<?php
class EvalAotForeachIterator implements Iterator {
    private int $i = 0;
    public function rewind(): void { echo "rewind:"; $this->i = 0; }
    public function valid(): bool { echo "valid" . $this->i . ":"; return $this->i < 2; }
    public function current(): mixed { echo "current" . $this->i . ":"; return "v" . $this->i; }
    public function key(): mixed { echo "key" . $this->i . ":"; return "k" . $this->i; }
    public function next(): void { echo "next" . $this->i . ":"; $this->i = $this->i + 1; }
}
class EvalAotForeachAggregate implements IteratorAggregate {
    public function getIterator(): Traversable { echo "agg:"; return new EvalAotForeachIterator(); }
}
eval('foreach (new EvalAotForeachIterator() as $key => $item) {
    echo $key . "=" . $item . ":";
    if ($item === "v0") { continue; }
    break;
}
echo "|";
foreach (new EvalAotForeachAggregate() as $item) {
    echo $item . ":";
}');
"#,
    );
    assert_eq!(
        out,
        "rewind:valid0:current0:key0:k0=v0:next0:valid1:current1:key1:k1=v1:|agg:rewind:valid0:current0:v0:next0:valid1:current1:v1:next1:valid2:"
    );
}

/// Verifies value-only foreach loops inside eval iterate associative array values.
#[test]
fn test_eval_foreach_iterates_assoc_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach (["a" => 1, "b" => 2] as $item) { echo $item; }');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies key-value foreach loops inside eval expose associative keys in insertion order.
#[test]
fn test_eval_foreach_iterates_assoc_keys_and_values() {
    let out = compile_and_run(
        r#"<?php
eval('foreach (["a" => 1, "b" => 2] as $key => $item) { echo $key . ":" . $item . ";"; }');
echo "|" . $key . ":" . $item;
"#,
    );
    assert_eq!(out, "a:1;b:2;|b:2");
}

/// Verifies eval indexed-array literals and reads execute through Mixed array helpers.
#[test]
fn test_eval_indexed_array_literal_and_read() {
    let out = compile_and_run("<?php echo eval('return [1, 2, 3][1];');");
    assert_eq!(out, "2");
}

/// Verifies legacy `array(...)` static reads normalize and lower through eval EIR AOT.
#[test]
fn test_literal_eval_legacy_array_literal_read_uses_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_legacy_array_read_aot");
    let source = r#"<?php
echo eval('return array("a", "b",)[1];');
echo ":";
echo eval('return array("name" => "Ada",)["name"];');
echo ":";
$rows = eval('return ARRAY(array(10, 20), array("name" => "Ada"));');
echo $rows[0][1] . ":" . $rows[1]["name"];
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "legacy array syntax static reads should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "legacy array syntax static reads should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only legacy array syntax static reads should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only legacy array syntax static reads should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only legacy array syntax static reads should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "b:Ada:20:Ada");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies legacy `array(...)` next-key assignment can read the local array through EIR AOT.
#[test]
fn test_literal_eval_legacy_array_literal_next_key_scope_assignment_uses_eir_aot_scope_helpers() {
    let dir = make_cli_test_dir("elephc_eval_legacy_array_next_key_aot_scope");
    let source = r#"<?php
eval('$items = array(2 => "two", "tail",); echo $items[3];');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "legacy array next-key eval assignment should use EIR AOT:\n{user_asm}"
    );
    assert!(
        user_asm.contains("__elephc_eval_scope_set"),
        "legacy array next-key eval assignment should write through eval scope helpers:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "legacy array next-key eval assignment should not execute through the bridge:\n{user_asm}"
    );
    assert!(
        runtime_asm.contains("__elephc_eval_scope_set"),
        "legacy array next-key eval assignment should emit core eval scope helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "scope-only legacy array next-key eval should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "tail");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies eval indexed-array writes mutate an array visible to native code.
#[test]
fn test_eval_indexed_array_write_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$items[0] = "a"; $items[1] = "b";');
echo $items[0] . $items[1];
"#,
    );
    assert_eq!(out, "ab");
}

/// Verifies eval mutates an existing native local array instead of replacing it with a fresh one.
#[test]
fn test_eval_mutates_existing_native_array_local() {
    let out = compile_and_run(
        r#"<?php
$items = ["a", "b"];
eval('$items[0] = "z"; $items[] = "c";');
echo $items[0] . ":" . $items[1] . ":" . $items[2] . ":" . count($items);
"#,
    );
    assert_eq!(out, "z:b:c:3");
}

/// Verifies eval array writes preserve PHP copy-on-write for by-value aliases.
#[test]
fn test_eval_array_write_preserves_native_by_value_alias() {
    let out = compile_and_run(
        r#"<?php
$items = ["a", "b"];
$snapshot = $items;
eval('$items[0] = "z"; $items[] = "c";');
echo $items[0] . ":" . $items[2] . ":" . count($items);
echo "|";
echo $snapshot[0] . ":" . count($snapshot);
"#,
    );
    assert_eq!(out, "z:c:3|a:2");
}

/// Verifies eval indexed-array append syntax writes the next visible element.
#[test]
fn test_eval_indexed_array_append_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$items = []; $items[] = "a"; $items[] = "b";');
echo $items[0] . ":" . $items[1] . ":" . count($items);
$existing = eval('return ["x"];');
eval('$existing[] = "y";');
echo ":" . $existing[1] . ":" . count($existing);
"#,
    );
    assert_eq!(out, "a:b:2:y:2");
}

/// Verifies eval associative-array append uses PHP's next automatic integer key.
#[test]
fn test_eval_assoc_array_append_uses_php_next_key() {
    let out = compile_and_run(
        r#"<?php
echo eval('$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];');
echo ":";
echo eval('$items = [2 => "two", "name" => "Ada"]; $items[] = "tail"; return $items[3];');
echo ":";
echo eval('$items = [-2 => "minus"]; $items[] = "tail"; return $items[-1];');
"#,
    );
    assert_eq!(out, "Grace:tail:tail");
}

/// Verifies eval can read a native Mixed array through runtime array helpers.
#[test]
fn test_eval_reads_native_mixed_array() {
    let out = compile_and_run(
        r#"<?php
$items = eval('return ["a", "b"];');
eval('echo $items[1];');
"#,
    );
    assert_eq!(out, "b");
}

/// Verifies eval can read string-keyed native associative arrays through Mixed helpers.
#[test]
fn test_eval_reads_native_assoc_array_string_key() {
    let out = compile_and_run(
        r#"<?php
$items = ["name" => "Ada"];
eval('echo $items["name"];');
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies eval can write string-keyed native associative arrays through Mixed helpers.
#[test]
fn test_eval_writes_native_assoc_array_string_key() {
    let out = compile_and_run(
        r#"<?php
$items = ["name" => "Ada"];
eval('$items["name"] = "Grace";');
echo $items["name"];
"#,
    );
    assert_eq!(out, "Grace");
}

/// Verifies eval can create and read associative array literals with string keys.
#[test]
fn test_eval_assoc_array_literal_and_string_key_read() {
    let out = compile_and_run(r#"<?php echo eval('return ["name" => "Ada"]["name"];');"#);
    assert_eq!(out, "Ada");
}

/// Verifies eval associative-array literals use PHP's next automatic key.
#[test]
fn test_eval_assoc_array_literal_unkeyed_entries_use_next_key() {
    let out = compile_and_run(
        r#"<?php
echo eval('return ["name" => "Ada", "Grace"][0];');
echo ":";
echo eval('return [2 => "two", "tail"][3];');
echo ":";
echo eval('return [-2 => "minus", "tail"][-1];');
echo ":";
echo eval('return ["2" => "two", "tail"][3];');
echo ":";
echo eval('return ["02" => "two", "tail"][0];');
echo ":";
echo eval('return [null => "empty"][""];');
echo ":";
echo eval('return [null => "empty", "tail"][0];');
echo ":";
echo eval('return [true => "yes", "tail"][2];');
echo ":";
echo eval('return [false => "no", "tail"][1];');
echo ":";
echo eval('return [2.7 => "two", "tail"][3];');
"#,
    );
    assert_eq!(out, "Grace:tail:tail:tail:tail:empty:tail:tail:tail:tail");
}

/// Verifies eval-created associative arrays remain visible to native code.
#[test]
fn test_eval_created_assoc_array_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$items = ["name" => "Ada"];');
echo $items["name"];
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies nested eval calls reuse the materialized caller scope.
#[test]
fn test_eval_nested_eval_uses_same_scope() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('eval("$x = $x + 4;");');
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies a nested eval return is the value of the inner eval expression.
#[test]
fn test_eval_nested_eval_return_value_is_expression_result() {
    let out = compile_and_run(r#"<?php echo eval('return eval("return 9;");');"#);
    assert_eq!(out, "9");
}

/// Verifies eval can dispatch simple builtin calls through its dynamic call path.
#[test]
fn test_eval_dispatches_simple_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo STRLEN("abcd") . ":" . \strlen("xy") . ":" . count([1, [2, 3], [4]]) . ":";
echo count([1, [2, 3], [4]], COUNT_RECURSIVE) . ":";
echo call_user_func("count", [1, [2]]) . ":";
echo call_user_func_array("count", ["value" => [1, [2]], "mode" => COUNT_RECURSIVE]) . ":";
echo defined("COUNT_RECURSIVE") ? "C" : "bad";');
"#,
    );
    assert_eq!(out, "4:2:3:6:2:3:C");
}

/// Verifies eval can dispatch raw pointer and buffer extension builtins through the bridge.
#[test]
fn test_eval_dispatches_raw_memory_builtins() {
    let out = compile_and_run(
        r#"<?php
echo eval('$buf = buffer_new(4);
$payload = ptr_offset($buf, 16);
echo buffer_len($buf) . ":";
ptr_set($payload, 123456789);
echo ptr_get($payload) . ":";
ptr_write8($payload, 255);
ptr_write8(ptr_offset($payload, 1), 1);
echo ptr_read8($payload) . "," . ptr_read8(ptr_offset($payload, 1)) . ":";
call_user_func_array("ptr_write16", ["pointer" => $payload, "value" => 4660]);
echo ptr_read16($payload) . ":";
ptr_write32($payload, 305419896);
echo ptr_read32($payload) . ":";
$written = ptr_write_string($payload, "GET /");
echo $written . ":" . ptr_read_string($payload, $written) . ":";
echo strlen(ptr_read_string($payload, 0));
buffer_free($buf);
echo ":" . (ptr_is_null($buf) ? "freed" : "live");
return ":" . function_exists("ptr_read16") . is_callable("ptr_write_string") . function_exists("buffer_new");');
"#,
    );
    assert_eq!(out, "4:123456789:255,1:4660:305419896:5:GET /:0:freed:111");
}

/// Verifies eval `count()` dispatches through `Countable` for generated/AOT objects.
#[test]
fn test_eval_counts_aot_countable_objects() {
    let out = compile_and_run(
        r#"<?php
class EvalAotCountableBag implements Countable {
    private int $n;
    public function __construct(int $n) { $this->n = $n; }
    public function count(): int { echo "count:"; return $this->n; }
}
$bag = new EvalAotCountableBag(5);
eval('echo count($bag); echo ":"; echo count($bag, COUNT_RECURSIVE); echo ":"; echo call_user_func_array("count", ["value" => $bag]);');
"#,
    );
    assert_eq!(out, "count:5:count:5:count:5");
}

/// Verifies eval dispatches `ArrayAccess` reads, writes, append, probes, and unset on AOT objects.
#[test]
fn test_eval_dispatches_aot_array_access_objects() {
    let out = compile_and_run(
        r#"<?php
class EvalAotArrayAccessBox implements ArrayAccess {
    public function offsetExists(mixed $offset): bool {
        echo "exists:" . $offset . ":";
        if ($offset === "missing") {
            return false;
        }
        return true;
    }
    public function offsetGet(mixed $offset): mixed {
        echo "get:" . $offset . ":";
        if ($offset === "empty") {
            return "";
        }
        return "v" . $offset;
    }
    public function offsetSet(mixed $offset, mixed $value): void {
        if ($offset === null) {
            echo "set:null:" . $value . ":";
        } else {
            echo "set:" . $offset . ":" . $value . ":";
        }
    }
    public function offsetUnset(mixed $offset): void {
        echo "unset:" . $offset . ":";
    }
}
$box = new EvalAotArrayAccessBox();
eval('$box["x"] = "1";
$box[] = "tail";
unset($box["drop"]);
if (isset($box["x"])) { echo "I:"; } else { echo "i:"; }
if (isset($box["missing"])) { echo "M:"; } else { echo "m:"; }
if (empty($box["empty"])) { echo "E:"; } else { echo "e:"; }
if (empty($box["missing"])) { echo "N:"; } else { echo "n:"; }
echo $box["y"];');
"#,
    );
    assert_eq!(
        out,
        "set:x:1:set:null:tail:unset:drop:exists:x:I:exists:missing:m:exists:empty:get:empty:E:exists:missing:N:get:y:vy"
    );
}

/// Verifies eval `json_encode()` serializes scalar, indexed, and associative values.
#[test]
fn test_eval_dispatches_json_encode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo json_encode(["a" => 1, "b" => "x/y"]) . ":";
echo json_encode([1, "q", true, null]) . ":";
echo call_user_func("json_encode", "a/b\"c") . ":";
echo call_user_func_array("json_encode", ["value" => ["k" => false], "flags" => 0, "depth" => 4]) . ":";
echo json_encode("a/b", JSON_UNESCAPED_SLASHES) . ":";
echo call_user_func_array("json_encode", ["value" => "x/y", "flags" => JSON_UNESCAPED_SLASHES]) . ":";
$accent = json_decode("\"\\u00e9\"");
$emoji = json_decode("\"\\ud83d\\ude00\"");
echo bin2hex(json_encode($accent . "/" . $emoji)) . ":";
echo bin2hex(json_encode($accent . "/" . $emoji, JSON_UNESCAPED_UNICODE)) . ":";
echo bin2hex(json_encode([$accent => $emoji])) . ":";
echo bin2hex(json_encode([$accent => $emoji], JSON_UNESCAPED_UNICODE)) . ":";
echo json_encode([1, 2], JSON_FORCE_OBJECT) . ":";
echo json_encode([], JSON_FORCE_OBJECT) . ":";
echo call_user_func_array("json_encode", ["value" => [1, 2], "flags" => JSON_FORCE_OBJECT]) . ":";
echo json_encode("<>&\"" . chr(39), JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT) . ":";
echo json_encode(["01", "+12", "1e3", " 7", "7x"], JSON_NUMERIC_CHECK) . ":";
echo json_encode([1.0, 2.5, -3.0], JSON_PRESERVE_ZERO_FRACTION) . ":";
echo (json_encode(INF) === false ? "false" : "json") . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo json_encode([1.5, INF, NAN], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":" . json_last_error_msg() . ":";
$bad = "a" . chr(128) . "b";
echo (json_encode($bad) === false ? "utf8-false" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_PARTIAL_OUTPUT_ON_ERROR)) . ":";
echo json_last_error() . ":";
echo json_encode($bad, JSON_INVALID_UTF8_IGNORE) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE | JSON_UNESCAPED_UNICODE)) . ":";
echo json_last_error() . ":";
echo json_encode(["k" . chr(128) => "v" . chr(128)], JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";
echo json_last_error() . ":";
json_encode(3.5);
echo json_last_error() . ":" . json_last_error_msg() . ":";
echo str_replace("\n", "|", json_encode(["a" => [1, 2]], JSON_PRETTY_PRINT)) . ":";
echo function_exists("json_encode");');
"#,
    );
    assert_eq!(
        out,
        r#"{"a":1,"b":"x\/y"}:[1,"q",true,null]:"a\/b\"c":{"k":false}:"a/b":"x/y":225c75303065395c2f5c75643833645c756465303022:22c3a95c2ff09f988022:7b225c7530306539223a225c75643833645c7564653030227d:7b22c3a9223a22f09f9880227d:{"0":1,"1":2}:{}:{"0":1,"1":2}:"\u003C\u003E\u0026\u0022\u0027":[1,12,1000,7,"7x"]:[1.0,2.5,-3.0]:false:7:Inf and NaN cannot be JSON encoded:[1.5,0,0]:7:Inf and NaN cannot be JSON encoded:utf8-false:5:6e756c6c:5:"ab":0:22615c75666666646222:0:2261efbfbd6222:0:{"":null}:5:0:No error:{|    "a": [|        1,|        2|    ]|}:1"#
    );
}

/// Verifies eval `json_decode()` materializes scalar, indexed, and associative values.
#[test]
fn test_eval_dispatches_json_decode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo json_decode("\"hello\"") . ":";
echo json_decode("42") . ":";
echo (json_decode("true") ? "T" : "bad") . ":";
echo (is_null(json_decode("null")) ? "NULL" : "bad") . ":";
$decoded = json_decode("{\"a\":1,\"b\":[\"x\",false]}", true);
echo $decoded["a"] . ":" . $decoded["b"][0] . ":" . ($decoded["b"][1] ? "bad" : "F") . ":";
$call = call_user_func("json_decode", "[3,4]");
echo $call[1] . ":";
$named = call_user_func_array("json_decode", ["json" => "{\"k\":\"v\"}", "associative" => true, "depth" => 4, "flags" => 0]);
echo $named["k"] . ":";
$badJson = "\"a" . chr(128) . "b\"";
echo (is_null(json_decode($badJson)) ? "utf8-null" : "bad") . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_IGNORE)) . ":";
echo json_last_error() . ":";
echo bin2hex(json_decode($badJson, true, 512, JSON_INVALID_UTF8_SUBSTITUTE)) . ":";
echo json_last_error() . ":";
$objSub = json_decode("{\"k" . chr(128) . "\":\"v" . chr(128) . "\"}", true, 512, JSON_INVALID_UTF8_SUBSTITUTE);
$objSubKeys = array_keys($objSub);
echo bin2hex($objSubKeys[0]) . "=" . bin2hex($objSub[$objSubKeys[0]]) . ":";
$objIgnore = json_decode("{\"k" . chr(128) . "\":\"v" . chr(128) . "\"}", true, 512, JSON_INVALID_UTF8_IGNORE);
$objIgnoreKeys = array_keys($objIgnore);
echo bin2hex($objIgnoreKeys[0]) . "=" . bin2hex($objIgnore[$objIgnoreKeys[0]]) . ":";
echo (is_null(json_decode("bad")) ? "BAD" : "wrong") . ":";
$big = json_decode("[9223372036854775808]", true, 512, JSON_BIGINT_AS_STRING);
echo json_decode("9223372036854775808", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo json_decode("-9223372036854775809", true, 512, JSON_BIGINT_AS_STRING) . ":";
echo gettype($big[0]) . ":" . $big[0] . ":";
echo call_user_func_array("json_decode", ["json" => "9223372036854775808", "associative" => true, "depth" => 512, "flags" => JSON_BIGINT_AS_STRING]) . ":";
echo function_exists("json_decode");');
"#,
    );
    assert_eq!(
        out,
        "hello:42:T:NULL:1:x:F:4:v:utf8-null:5:6162:0:61efbfbd62:0:6befbfbd=76efbfbd:6b=76:BAD:9223372036854775808:-9223372036854775809:string:9223372036854775808:9223372036854775808:1"
    );
}

/// Verifies eval `json_decode()` returns `stdClass` objects unless assoc is true.
#[test]
fn test_eval_dispatches_json_decode_stdclass_default() {
    let out = compile_and_run(
        r#"<?php
eval('$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo $object->a . ":" . $object->b->c . ":";
$objectFalse = json_decode("{\"z\":2}", false);
echo $objectFalse->z . ":";
$objectNull = json_decode("{\"n\":{\"m\":3}}", null);
echo $objectNull->n->m . ":";
$assoc = json_decode("{\"b\":{\"c\":\"y\"}}", true);
echo $assoc["b"]["c"] . ":";');
$object = eval('return json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");');
echo gettype($object) . ":" . $object->a . ":" . $object->b->c;
"#,
    );
    assert_eq!(out, "1:x:2:3:y:object:1:x");
}

/// Verifies eval `json_encode()` serializes stdClass dynamic properties.
#[test]
fn test_eval_dispatches_json_encode_stdclass_object() {
    let out = compile_and_run(
        r#"<?php
eval('$object = json_decode("{\"a\":1,\"b\":{\"c\":\"x\"}}");
echo json_encode($object) . ":";
echo str_replace("\n", "|", json_encode($object, JSON_PRETTY_PRINT)) . ":";
$empty = json_decode("{}");
echo json_encode($empty) . ":";
$empty->a = 7;
echo json_encode($empty);');
"#,
    );
    assert_eq!(
        out,
        r#"{"a":1,"b":{"c":"x"}}:{|    "a": 1,|    "b": {|        "c": "x"|    }|}:{}:{"a":7}"#
    );
}

/// Verifies eval `json_last_error*()` track JSON parse failures and success resets.
#[test]
fn test_eval_dispatches_json_last_error_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("bad");
echo json_last_error() . ":" . (json_last_error() === JSON_ERROR_SYNTAX ? "syntax" : "bad") . ":" . json_last_error_msg() . ":";
json_validate("[1]", 1);
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"ok\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("\"a" . chr(10) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"\\uD83D\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_decode("\"a" . chr(128) . "b\"");
echo json_last_error() . ":" . json_last_error_msg() . ":";
json_validate("[0]");
echo call_user_func("json_last_error") . ":" . call_user_func_array("json_last_error_msg", []) . ":";
echo function_exists("json_last_error") && function_exists("json_last_error_msg") && defined("JSON_ERROR_SYNTAX");');
"#,
    );
    assert_eq!(
        out,
        "0:No error:4:syntax:Syntax error near location 1:1:1:Maximum stack depth exceeded near location 1:1:0:No error:3:Control character error, possibly incorrectly encoded near location 1:3:10:Single unpaired UTF-16 surrogate in unicode escape near location 1:8:5:Malformed UTF-8 characters, possibly incorrectly encoded near location 1:3:0:No error:1"
    );
}

/// Verifies eval JSON throw flags raise catchable `JsonException` objects.
#[test]
fn test_eval_dispatches_json_throw_on_error() {
    let out = compile_and_run(
        r#"<?php
eval('try {
    json_decode("bad", true, 512, JSON_THROW_ON_ERROR);
    echo "bad";
} catch (Throwable) {
    echo "inner:";
}');
try {
    eval('json_decode("bad", true, 512, JSON_THROW_ON_ERROR);');
    echo "bad";
} catch (Throwable $e) {
    echo "outer:" . get_class($e) . ":" . $e->getCode() . ":" . (str_contains($e->getMessage(), "Syntax error") ? "syntax" : "bad") . ":";
}
try {
    eval('json_encode(INF, JSON_THROW_ON_ERROR);');
    echo "bad";
} catch (Throwable $e) {
    echo "encode:" . get_class($e) . ":" . $e->getCode() . ":" . $e->getMessage() . ":";
}
eval('echo json_encode(INF, JSON_THROW_ON_ERROR | JSON_PARTIAL_OUTPUT_ON_ERROR) . ":";');
eval('$json = chr(34) . "a" . chr(128) . "b" . chr(34); echo json_decode($json, true, 512, JSON_THROW_ON_ERROR | JSON_INVALID_UTF8_IGNORE) . ":";');
"#,
    );
    assert_eq!(
        out,
        "inner:outer:JsonException:4:syntax:encode:JsonException:7:Inf and NaN cannot be JSON encoded:0:ab:"
    );
}

/// Verifies eval `json_validate()` validates JSON syntax, depth, and dynamic calls.
#[test]
fn test_eval_dispatches_json_validate_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo (json_validate("{\"a\":[1,true,null,\"caf\\u00e9\"]}") ? "Y" : "N") . ":";
echo (json_validate("bad") ? "bad" : "N") . ":";
echo (json_validate("[1]", 1) ? "bad" : "D") . ":";
echo (call_user_func("json_validate", "\"x\"") ? "C" : "bad") . ":";
echo (call_user_func_array("json_validate", ["json" => "[[1]]", "depth" => 3, "flags" => 0]) ? "A" : "bad") . ":";
echo (json_validate("\"a" . chr(128) . "b\"", 512, JSON_INVALID_UTF8_IGNORE) ? "I" : "bad") . ":";
echo json_last_error() . ":";
echo (json_validate("bad", 512, JSON_INVALID_UTF8_IGNORE) ? "bad" : "S") . ":";
echo json_last_error() . ":";
echo function_exists("json_validate");');
"#,
    );
    assert_eq!(out, "Y:N:D:C:A:I:0:S:4:1");
}

/// Verifies eval direct builtin calls bind named arguments and spread arrays.
#[test]
fn test_eval_dispatches_named_and_spread_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(string: "abcd");
echo ":" . (array_key_exists(array: ["name" => 1], key: "name") ? "Y" : "N");
echo ":" . round(precision: 1, num: 3.14);
echo ":" . (str_contains(...["haystack" => "abc", "needle" => "b"]) ? "Y" : "N");');
"#,
    );
    assert_eq!(out, "4:Y:3.1:Y");
}

/// Verifies eval `ord()` returns the first byte and dispatches dynamically.
#[test]
fn test_eval_dispatches_ord_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo ord("A");
echo ":" . ord("");
echo ":" . call_user_func("ord", "B");
echo ":" . call_user_func_array("ord", ["C"]);
echo ":"; echo function_exists("ord");');
"#,
    );
    assert_eq!(out, "65:0:66:67:1");
}

/// Verifies eval array aggregate builtins iterate values and dispatch dynamically.
#[test]
fn test_eval_dispatches_array_aggregate_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo array_sum([1, 2, 3]);
echo ":" . array_product([2, 3, 4]);
echo ":" . array_sum([]);
echo ":" . array_product([]);
echo ":" . array_sum(["a" => 2, "b" => 5]);
echo ":" . call_user_func("array_sum", [3, 4]);
echo ":" . call_user_func_array("array_product", [[2, 5]]);
echo ":"; echo function_exists("array_sum"); echo function_exists("array_product");');
"#,
    );
    assert_eq!(out, "6:24:0:1:7:7:10:11");
}

/// Verifies eval `array_map()` applies callbacks and preserves source keys.
#[test]
fn test_eval_dispatches_array_map_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_map_double($value) { return $value * 2; }
$mapped = array_map("eval_map_double", [1, 2, 3]);
echo $mapped[0] . ":" . $mapped[2] . ":";
$assoc = array_map("strtoupper", ["a" => "x", "b" => "y"]);
echo $assoc["a"] . ":" . $assoc["b"] . ":";
$identity = array_map(null, ["k" => "v"]);
echo $identity["k"] . ":";
function eval_map_pair($left, $right) { return $left . "-" . ($right ?? "N"); }
$pairs = array_map("eval_map_pair", ["a" => "L", "b" => "R"], ["x" => "1"]);
echo $pairs[0] . ":" . $pairs[1] . ":";
$zipped = array_map(null, [1, 2], [3, 4]);
echo $zipped[0][0] . $zipped[0][1] . ":" . $zipped[1][0] . $zipped[1][1] . ":";
$call = call_user_func("array_map", "intval", ["7"]);
echo $call[0] . ":";
$multi_call = call_user_func("array_map", "eval_map_pair", ["Q"], ["9"]);
echo $multi_call[0] . ":";
$spread = call_user_func_array("array_map", ["callback" => "strval", "array" => [8]]);
echo $spread[0] . ":";
echo function_exists("array_map");');
"#,
    );
    assert_eq!(out, "2:6:X:Y:v:L-1:R-N:13:24:7:Q-9:8:1");
}

/// Verifies eval `array_reduce()` folds values through a string callback.
#[test]
fn test_eval_dispatches_array_reduce_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_reduce_sum($carry, $item) { return $carry + $item; }
echo array_reduce([1, 2, 3], "eval_reduce_sum", 10) . ":";
function eval_reduce_join($carry, $item) { return $carry . $item; }
echo array_reduce([4, 5], "eval_reduce_sum") . ":";
echo array_reduce(["a", "b"], "eval_reduce_join", "") . ":";
$named = array_reduce(array: [6, 7], callback: "eval_reduce_sum");
echo $named . ":";
$call = call_user_func("array_reduce", [4, 5], "eval_reduce_sum");
echo $call . ":";
$spread = call_user_func_array("array_reduce", ["array" => [2, 3], "callback" => "eval_reduce_sum", "initial" => 4]);
echo $spread . ":";
echo function_exists("array_reduce");');
"#,
    );
    assert_eq!(out, "16:9:ab:13:9:9:1");
}

/// Verifies eval `array_walk()` invokes string callbacks with value and key cells.
#[test]
fn test_eval_dispatches_array_walk_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_walk_show($value, $key) { echo $key . "=" . $value . ";"; }
$walk = ["a" => 2, "b" => 3];
echo array_walk($walk, "eval_walk_show") ? "T:" : "F:";
$call = call_user_func("array_walk", [4, 5], "eval_walk_show");
$spread = call_user_func_array("array_walk", ["array" => ["z" => 6], "callback" => "eval_walk_show"]);
echo function_exists("array_walk");');
"#,
    );
    assert_eq!(out, "a=2;b=3;T:0=4;1=5;z=6;1");
}

/// Verifies eval `array_pop()` and `array_shift()` mutate writable lvalue arguments.
#[test]
fn test_eval_dispatches_array_pop_shift_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [1, 2, 3];
echo array_pop($a) . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
echo array_shift(array: $b) . ":" . $b[0] . ":" . $b["y"] . ":" . $b[1] . ":";
$c = [4, 5];
echo call_user_func("array_pop", $c) . ":" . count($c) . ":" . $c[1] . ":";
$d = [6, 7];
echo call_user_func_array("array_shift", ["array" => $d]) . ":" . count($d) . ":" . $d[0] . ":";
class EvalArrayPopShiftPropertyBox {
    public array $items = ["p", "q"];
    public static array $staticItems = ["s", "t"];
}
$box = new EvalArrayPopShiftPropertyBox();
echo array_pop($box->items) . ":" . count($box->items) . ":" . $box->items[0] . ":";
$name = "items";
echo array_push($box->{$name}, "r") . ":" . $box->items[1] . ":";
echo array_shift(EvalArrayPopShiftPropertyBox::$staticItems) . ":" . EvalArrayPopShiftPropertyBox::$staticItems[0] . ":";
$class = "EvalArrayPopShiftPropertyBox";
$staticName = "staticItems";
echo array_unshift($class::${$staticName}, "u") . ":" . EvalArrayPopShiftPropertyBox::$staticItems[0] . ":" . EvalArrayPopShiftPropertyBox::$staticItems[1] . ":";
echo function_exists("array_pop") && function_exists("array_shift");');
"#,
    );
    assert_eq!(out, "3:2:2:1:2:3:4:5:2:5:6:2:6:q:1:p:2:r:s:t:2:u:t:1");
}

/// Verifies eval `array_push()` and `array_unshift()` mutate writable lvalue arguments.
#[test]
fn test_eval_dispatches_array_push_unshift_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [1];
echo array_push($a, 2, 3) . ":" . $a[2] . ":";
$b = ["x" => 1, 10 => 2];
echo array_push($b, "A") . ":" . $b["x"] . ":" . $b[11] . ":";
$c = [2, 3];
echo array_unshift($c, 0, 1) . ":" . $c[0] . ":" . $c[3] . ":";
$d = ["x" => 1, 10 => 2, "y" => 3];
echo array_unshift($d, "A") . ":" . $d[0] . ":" . $d["x"] . ":" . $d[1] . ":" . $d["y"] . ":";
$e = [5];
echo call_user_func("array_push", $e, 6) . ":" . count($e) . ":" . $e[0] . ":";
$f = [7];
echo call_user_func_array("array_unshift", [$f, 6]) . ":" . count($f) . ":" . $f[0] . ":";
class EvalArrayPushUnshiftPropertyBox {
    public array $items = ["p"];
    public static array $staticItems = ["s"];
}
$box = new EvalArrayPushUnshiftPropertyBox();
echo array_push($box->items, "q", "r") . ":" . $box->items[2] . ":";
$name = "items";
echo array_unshift($box->{$name}, "o") . ":" . $box->items[0] . ":" . $box->items[3] . ":";
echo array_push(EvalArrayPushUnshiftPropertyBox::$staticItems, "t") . ":" . EvalArrayPushUnshiftPropertyBox::$staticItems[1] . ":";
$class = "EvalArrayPushUnshiftPropertyBox";
$staticName = "staticItems";
echo array_unshift($class::${$staticName}, "r") . ":" . EvalArrayPushUnshiftPropertyBox::$staticItems[0] . ":" . EvalArrayPushUnshiftPropertyBox::$staticItems[2] . ":";
echo function_exists("array_push") && function_exists("array_unshift");');
"#,
    );
    assert_eq!(
        out,
        "3:3:3:1:A:4:0:3:4:A:1:2:3:2:1:5:2:1:7:3:r:4:o:r:2:t:3:r:t:1"
    );
}

/// Verifies first-class eval builtin callables preserve ref-like writeback targets.
#[test]
fn test_eval_first_class_ref_like_builtin_callables_write_back_lvalues() {
    let out = compile_and_run(
        r#"<?php
eval('$pop = array_pop(...);
$items = [1, 2, 3];
echo $pop($items) . ":" . count($items) . ":" . $items[1] . ":";
$sort = sort(...);
$sortable = [3, 1, 2];
echo $sort($sortable) . ":" . implode(",", $sortable) . ":";
$set = settype(...);
$value = "42";
echo $set($value, "integer") . ":" . gettype($value) . ":" . $value . ":";
class EvalFirstClassRefLikeBuiltinBox {
    public array $items = ["a"];
    public static array $staticItems = [2, 1];
}
$box = new EvalFirstClassRefLikeBuiltinBox();
$push = array_push(...);
echo $push($box->items, "b") . ":" . $box->items[1] . ":";
$rsort = rsort(...);
echo $rsort(EvalFirstClassRefLikeBuiltinBox::$staticItems) . ":" . EvalFirstClassRefLikeBuiltinBox::$staticItems[0] . EvalFirstClassRefLikeBuiltinBox::$staticItems[1];');
"#,
    );
    assert_eq!(out, "3:2:2:1:1,2,3:1:integer:42:2:b:1:21");
}

/// Verifies eval `call_user_func_array()` preserves ref-like builtin writeback aliases.
#[test]
fn test_eval_call_user_func_array_ref_like_builtin_callables_write_back_lvalues() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalCallArrayRefLikeBuiltinBox {
    public array $items = [3, 1, 2];
    public static array $staticItems = [1, 2];
    public mixed $value = "42";
    public static mixed $staticValue = "0";
}
$box = new EvalCallArrayRefLikeBuiltinBox();
$sort = sort(...);
echo call_user_func_array($sort, [&$box->items]) . ":" . implode(",", $box->items) . ":";
$rsort = rsort(...);
echo call_user_func_array($rsort, [&EvalCallArrayRefLikeBuiltinBox::$staticItems]) . ":" . implode(",", EvalCallArrayRefLikeBuiltinBox::$staticItems) . ":";
$set = settype(...);
echo call_user_func_array($set, ["var" => &$box->value, "type" => "integer"]) . ":" . gettype($box->value) . ":" . $box->value . ":";
$string = "settype";
echo call_user_func_array($string, [&EvalCallArrayRefLikeBuiltinBox::$staticValue, "bool"]) . ":" . gettype(EvalCallArrayRefLikeBuiltinBox::$staticValue) . ":" . (EvalCallArrayRefLikeBuiltinBox::$staticValue ? "true" : "false") . ":";
$push = array_push(...);
echo call_user_func_array($push, [&$box->items, 4]) . ":" . $box->items[3];');
"#,
    );
    assert_eq!(out, "1:1,2,3:1:2,1:1:integer:42:1:boolean:false:4:4");
}

/// Verifies eval `array_splice()` mutates writable lvalue arguments.
#[test]
fn test_eval_dispatches_array_splice_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [10, 20, 30, 40];
$removed = array_splice($a, 1, 2);
echo count($removed) . ":" . $removed[0] . ":" . $removed[1] . ":" . count($a) . ":" . $a[1] . ":";
$b = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$cut = array_splice(array: $b, offset: 1, length: 2);
echo $cut[0] . ":" . $cut["y"] . ":" . $b["x"] . ":" . $b[0] . ":";
$c = [1, 2, 3, 4];
$tail = call_user_func("array_splice", $c, -2, 1);
echo $tail[0] . ":" . count($c) . ":" . $c[2] . ":";
$d = [5, 6, 7];
$all = call_user_func_array("array_splice", ["array" => $d, "offset" => 1]);
echo count($all) . ":" . $all[0] . ":" . $all[1] . ":" . count($d) . ":";
$e = [1, 2, 3, 4];
$rep = array_splice($e, 1, 2, ["A", "B"]);
echo count($rep) . ":" . $rep[0] . ":" . $rep[1] . ":" . $e[0] . ":" . $e[1] . ":" . $e[2] . ":" . $e[3] . ":";
$f = ["x" => 1, 10 => 2, "y" => 3, 11 => 4];
$rep2 = array_splice(array: $f, offset: 1, length: 2, replacement: ["s" => "S", 9 => "N"]);
echo $rep2[0] . ":" . $rep2["y"] . ":" . $f["x"] . ":" . $f[0] . ":" . $f[1] . ":" . $f[2] . ":";
$g = [1, 2, 3];
$rep3 = array_splice($g, offset: 1, replacement: [9]);
echo count($rep3) . ":" . $rep3[0] . ":" . $rep3[1] . ":" . count($g) . ":" . $g[1] . ":";
$h = [1, 2, 3];
$removed2 = call_user_func_array("array_splice", ["array" => $h, "offset" => 1, "replacement" => [9]]);
echo count($removed2) . ":" . $removed2[0] . ":" . $removed2[1] . ":" . count($h) . ":" . $h[1] . ":";
class EvalArraySplicePropertyBox {
    public array $items = ["a", "b", "c"];
    public static array $staticItems = ["x", "y", "z"];
}
$box = new EvalArraySplicePropertyBox();
$propRemoved = array_splice($box->items, 1, 1, ["B"]);
echo count($propRemoved) . ":" . $propRemoved[0] . ":" . $box->items[1] . ":" . $box->items[2] . ":";
$name = "items";
$dynRemoved = array_splice($box->{$name}, 0, 1);
echo $dynRemoved[0] . ":" . count($box->items) . ":" . $box->items[0] . ":";
$staticRemoved = array_splice(EvalArraySplicePropertyBox::$staticItems, 1, 1);
echo $staticRemoved[0] . ":" . count(EvalArraySplicePropertyBox::$staticItems) . ":" . EvalArraySplicePropertyBox::$staticItems[1] . ":";
$class = "EvalArraySplicePropertyBox";
$staticName = "staticItems";
$dynStaticRemoved = array_splice($class::${$staticName}, 0, 1, ["w"]);
echo $dynStaticRemoved[0] . ":" . EvalArraySplicePropertyBox::$staticItems[0] . ":" . EvalArraySplicePropertyBox::$staticItems[1] . ":";
echo function_exists("array_splice");');
"#,
    );
    assert_eq!(
        out,
        "2:20:30:2:40:2:3:1:4:3:4:3:2:6:7:3:2:2:3:1:A:B:4:2:3:1:S:N:4:2:2:3:2:9:2:2:3:3:2:1:b:B:c:a:2:B:y:2:z:x:w:z:1"
    );
}

/// Verifies eval `sort()` and `rsort()` mutate writable lvalue arguments.
#[test]
fn test_eval_dispatches_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [3, 1, 2];
echo sort($a) . ":" . $a[0] . $a[1] . $a[2] . ":";
$b = ["banana", "apple", "cherry"];
echo rsort(array: $b) . ":" . $b[0] . ":" . $b[2] . ":";
$c = ["x" => 3, "y" => 1, "z" => 2];
sort($c);
echo $c[0] . $c[1] . $c[2] . ":";
$d = [3, 1, 2];
echo call_user_func("sort", $d) . ":" . $d[0] . $d[1] . $d[2] . ":";
$e = [1, 2, 3];
echo call_user_func_array("rsort", ["array" => $e]) . ":" . $e[0] . ":" . $e[2] . ":";
class EvalSortPropertyBox {
    public array $items = [3, 1, 2];
    public static array $staticItems = ["b", "a"];
}
$box = new EvalSortPropertyBox();
echo sort($box->items) . ":" . $box->items[0] . $box->items[1] . $box->items[2] . ":";
$name = "items";
echo rsort($box->{$name}) . ":" . $box->items[0] . ":" . $box->items[2] . ":";
echo sort(EvalSortPropertyBox::$staticItems) . ":" . EvalSortPropertyBox::$staticItems[0] . EvalSortPropertyBox::$staticItems[1] . ":";
$class = "EvalSortPropertyBox";
$staticName = "staticItems";
echo rsort($class::${$staticName}) . ":" . EvalSortPropertyBox::$staticItems[0] . EvalSortPropertyBox::$staticItems[1] . ":";
echo function_exists("sort") && function_exists("rsort");');
"#,
    );
    assert_eq!(
        out,
        "1:123:1:cherry:apple:123:1:312:1:1:3:1:123:1:3:1:1:ab:1:ba:1"
    );
}

/// Verifies eval key-preserving sort builtins mutate writable lvalue arguments.
#[test]
fn test_eval_dispatches_key_preserving_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = ["x" => 3, "y" => 1, "z" => 2];
echo asort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value; }
echo ":";
$b = ["x" => 1, "y" => 3, "z" => 2];
echo arsort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2, 3 => 4];
echo ksort($c) . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = ["b" => 1, "a" => 2, 3 => 4];
echo krsort($d) . ":";
foreach ($d as $key => $value) { echo $key . $value; }
echo ":";
$e = ["x" => 2, "y" => 1];
echo call_user_func("asort", $e) . ":" . $e["x"] . $e["y"] . ":";
$f = ["b" => 1, "a" => 2];
echo call_user_func_array("krsort", ["array" => $f]) . ":" . $f["b"] . $f["a"] . ":";
class EvalKeySortPropertyBox {
    public array $items = ["x" => 2, "y" => 1];
    public static array $staticItems = ["b" => 1, "a" => 2];
}
$box = new EvalKeySortPropertyBox();
echo asort($box->items) . ":";
foreach ($box->items as $key => $value) { echo $key . $value; }
echo ":";
$name = "items";
echo arsort($box->{$name}) . ":";
foreach ($box->items as $key => $value) { echo $key . $value; }
echo ":";
echo ksort(EvalKeySortPropertyBox::$staticItems) . ":";
foreach (EvalKeySortPropertyBox::$staticItems as $key => $value) { echo $key . $value; }
echo ":";
$class = "EvalKeySortPropertyBox";
$staticName = "staticItems";
echo krsort($class::${$staticName}) . ":";
foreach (EvalKeySortPropertyBox::$staticItems as $key => $value) { echo $key . $value; }
echo ":";
echo function_exists("asort") && function_exists("arsort") && function_exists("ksort") && function_exists("krsort");');
"#,
    );
    assert_eq!(
        out,
        "1:y1z2x3:1:y3z2x1:1:34a2b1:1:b1a234:1:21:1:12:1:y1x2:1:x2y1:1:a2b1:1:b1a2:1"
    );
}

/// Verifies eval natural sort builtins preserve keys and use natural string order.
#[test]
fn test_eval_dispatches_natural_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$a = ["img10", "img2", "img1"];
echo natsort($a) . ":";
foreach ($a as $key => $value) { echo $key . $value . ";"; }
echo ":";
$b = ["b" => "Img10", "a" => "img2", "c" => "IMG1"];
echo natcasesort(array: $b) . ":";
foreach ($b as $key => $value) { echo $key . $value . ";"; }
echo ":";
$c = ["x" => "b", "y" => "a"];
echo call_user_func("natsort", $c) . ":" . $c["x"] . $c["y"] . ":";
class EvalNaturalSortPropertyBox {
    public array $items = ["img10", "img2", "img1"];
    public static array $staticItems = ["b" => "Img10", "a" => "img2", "c" => "IMG1"];
}
$box = new EvalNaturalSortPropertyBox();
echo natsort($box->items) . ":";
foreach ($box->items as $key => $value) { echo $key . $value . ";"; }
echo ":";
$class = "EvalNaturalSortPropertyBox";
$staticName = "staticItems";
echo natcasesort($class::${$staticName}) . ":";
foreach (EvalNaturalSortPropertyBox::$staticItems as $key => $value) { echo $key . $value . ";"; }
echo ":";
echo function_exists("natsort") && function_exists("natcasesort");');
"#,
    );
    assert_eq!(
        out,
        "1:2img1;1img2;0img10;:1:cIMG1;aimg2;bImg10;:1:ba:1:2img1;1img2;0img10;:1:cIMG1;aimg2;bImg10;:1"
    );
}

/// Verifies eval `shuffle()` reindexes writable array lvalues.
#[test]
fn test_eval_dispatches_shuffle_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$a = ["x" => 1, "y" => 2];
echo shuffle($a) . ":" . (isset($a["x"]) ? "bad" : "reindexed") . ":" . count($a) . ":" . array_sum($a) . ":";
$b = ["x" => 1, "y" => 2];
echo call_user_func("shuffle", $b) . ":" . $b["x"] . $b["y"] . ":";
class EvalShufflePropertyBox {
    public array $items = ["x" => 1, "y" => 2];
    public static array $staticItems = ["a" => 3, "b" => 4];
}
$box = new EvalShufflePropertyBox();
echo shuffle($box->items) . ":" . (isset($box->items["x"]) ? "bad" : "prop") . ":" . count($box->items) . ":" . array_sum($box->items) . ":";
$class = "EvalShufflePropertyBox";
$staticName = "staticItems";
echo shuffle($class::${$staticName}) . ":" . (isset(EvalShufflePropertyBox::$staticItems["a"]) ? "bad" : "static") . ":" . count(EvalShufflePropertyBox::$staticItems) . ":" . array_sum(EvalShufflePropertyBox::$staticItems) . ":";
echo function_exists("shuffle");');
"#,
    );
    assert_eq!(out, "1:reindexed:2:3:1:12:1:prop:2:3:1:static:2:7:1");
}

/// Verifies eval user-comparator sort builtins call callbacks and mutate writable lvalues.
#[test]
fn test_eval_dispatches_user_sort_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('function eval_sort_cmp($left, $right) { echo "c"; return $left <=> $right; }
function eval_key_cmp($left, $right) { return strcmp($left, $right); }
function eval_sort_quiet_cmp($left, $right) { return $left <=> $right; }
$a = [3, 1, 2];
echo usort($a, "eval_sort_cmp") . ":";
foreach ($a as $value) { echo $value; }
echo ":";
$b = ["b" => 1, "a" => 3, "c" => 2];
echo uasort(array: $b, callback: "eval_sort_cmp") . ":";
foreach ($b as $key => $value) { echo $key . $value; }
echo ":";
$c = ["b" => 1, "a" => 2];
echo uksort($c, "eval_key_cmp") . ":";
foreach ($c as $key => $value) { echo $key . $value; }
echo ":";
$d = [2, 1];
echo call_user_func("usort", $d, "eval_sort_cmp") . ":" . $d[0] . $d[1] . ":";
class EvalUserSortPropertyBox {
    public array $items = [3, 1, 2];
    public static array $staticItems = ["b" => 1, "a" => 2];
}
$box = new EvalUserSortPropertyBox();
echo usort($box->items, "eval_sort_quiet_cmp") . ":";
foreach ($box->items as $value) { echo $value; }
echo ":";
$name = "items";
echo usort($box->{$name}, "eval_sort_quiet_cmp") . ":" . $box->items[0] . $box->items[2] . ":";
$class = "EvalUserSortPropertyBox";
$staticName = "staticItems";
echo uksort($class::${$staticName}, "eval_key_cmp") . ":";
foreach (EvalUserSortPropertyBox::$staticItems as $key => $value) { echo $key . $value; }
echo ":";
echo function_exists("usort") && function_exists("uasort") && function_exists("uksort");');
"#,
    );
    assert_eq!(out, "ccc1:123:ccc1:b1c2a3:1:a2b1:c1:21:1:123:1:13:1:a2b1:1");
}

/// Verifies eval iterator array helpers dispatch through direct and dynamic calls.
#[test]
fn test_eval_dispatches_iterator_array_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$items = ["x" => 1, "y" => 2];
$copy = iterator_to_array($items);
echo iterator_count($items) . ":" . $copy["x"] . $copy["y"] . ":";
$values = iterator_to_array($items, false);
echo (isset($values["x"]) ? "bad" : "reindexed") . ":" . $values[0] . $values[1] . ":";
echo call_user_func("iterator_count", $items) . ":";
$spread = call_user_func_array("iterator_to_array", ["iterator" => $items, "preserve_keys" => false]);
echo $spread[0] . $spread[1] . ":";
echo function_exists("iterator_count") && function_exists("iterator_to_array");');
"#,
    );
    assert_eq!(out, "2:12:reindexed:12:2:12:1");
}

/// Verifies eval `iterator_apply()` drives AOT Iterator objects through eval callbacks.
#[test]
fn test_eval_dispatches_iterator_apply_object_builtin() {
    let out = compile_and_run(
        r#"<?php
class EvalApplyRange implements Iterator {
    private int $i;
    private int $end;
    public function __construct(int $end) { $this->i = 0; $this->end = $end; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < $this->end; }
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}
eval('function eval_apply_label($prefix) { echo $prefix; return true; }
$r = new EvalApplyRange(2);
echo iterator_apply($r, "eval_apply_label", ["prefix" => "E"]) . ":";
echo call_user_func("iterator_apply", $r, "eval_apply_label", ["C"]);');
"#,
    );
    assert_eq!(out, "EE2:CC2");
}

/// Verifies eval `array_filter()` removes falsey values and preserves source keys.
#[test]
fn test_eval_dispatches_array_filter_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$filtered = array_filter([0, 1, 2, "", false, null, "0", "ok"]);
echo count($filtered) . ":" . $filtered[1] . ":" . $filtered[2] . ":" . $filtered[7] . ":";
$assoc = array_filter(["a" => 0, "b" => 2, "c" => ""]);
echo (array_key_exists("a", $assoc) ? "bad" : "drop") . ":" . $assoc["b"] . ":";
$null = array_filter([0, 3], null, 1);
echo count($null) . ":" . $null[1] . ":";
$call = call_user_func("array_filter", [0, 4]);
echo count($call) . ":" . $call[1] . ":";
$spread = call_user_func_array("array_filter", ["array" => [0, 5], "callback" => null]);
echo count($spread) . ":" . $spread[1] . ":";
function eval_keep_even($value) { return $value % 2 == 0; }
$evens = array_filter([1, 2, 3, 4], "eval_keep_even");
echo count($evens) . ":" . $evens[1] . ":" . $evens[3] . ":";
function eval_keep_key($key) { return $key === "b"; }
$keyed = array_filter(["a" => 10, "b" => 20], "eval_keep_key", ARRAY_FILTER_USE_KEY);
echo count($keyed) . ":" . $keyed["b"] . ":";
function eval_keep_both($value, $key) { return $key === "c" || $value === 1; }
$both = array_filter(["a" => 1, "b" => 2, "c" => 3], "eval_keep_both", ARRAY_FILTER_USE_BOTH);
echo count($both) . ":" . $both["a"] . ":" . $both["c"] . ":";
$ints = array_filter([1, "x", 2], "is_int");
echo count($ints) . ":" . $ints[0] . ":" . $ints[2] . ":";
echo function_exists("array_filter");');
"#,
    );
    assert_eq!(out, "3:1:2:ok:drop:2:1:3:1:4:1:5:2:2:4:1:20:2:1:3:2:1:2:1");
}

/// Verifies eval `array_combine()` supports PHP key conversions and callable dispatch.
#[test]
fn test_eval_dispatches_array_combine_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$pairs = array_combine(["a", "b"], [10, 20]);
echo $pairs["a"] . ":" . $pairs["b"];
$numeric = array_combine(["1", "01"], ["n", "z"]);
echo ":" . $numeric[1] . $numeric["01"];
$scalar = array_combine([null, true, false, 2.8], ["n", "t", "f", "d"]);
echo ":" . $scalar[""] . $scalar[1] . $scalar["2.8"];
$named = array_combine(keys: ["k"], values: ["v"]);
echo ":" . $named["k"];
$call = call_user_func("array_combine", ["x"], [7]);
echo ":" . $call["x"];
$spread = call_user_func_array("array_combine", [["y"], [8]]);
echo ":" . $spread["y"] . ":";
echo function_exists("array_combine");');
"#,
    );
    assert_eq!(out, "10:20:nz:ftd:v:7:8:1");
}

/// Verifies eval `array_column()` extracts present row columns and reindexes them.
#[test]
fn test_eval_dispatches_array_column_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$rows = [["name" => "Ada", "score" => 10], ["score" => 20], ["name" => "Lin", "score" => 30], 42];
$names = array_column($rows, "name");
echo count($names) . ":" . $names[0] . ":" . $names[1];
$scores = array_column($rows, "score");
echo ":" . count($scores) . ":" . $scores[0] . $scores[2];
$numeric = array_column([[0 => "zero", 1 => "one"], [1 => "uno"]], 1);
echo ":" . count($numeric) . ":" . $numeric[0] . ":" . $numeric[1];
$named = array_column(array: $rows, column_key: "score");
echo ":" . $named[1];
$call = call_user_func("array_column", [["x" => 5], ["x" => 6]], "x");
echo ":" . $call[1];
$spread = call_user_func_array("array_column", [[["y" => 7], ["z" => 0], ["y" => 9]], "y"]);
echo ":" . count($spread) . ":" . $spread[1] . ":";
echo function_exists("array_column");');
"#,
    );
    assert_eq!(out, "2:Ada:Lin:3:1030:2:one:uno:20:6:2:9:1");
}

/// Verifies eval `array_pad()` and `array_chunk()` build reindexed array shapes.
#[test]
fn test_eval_dispatches_array_shape_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$right = array_pad([1, 2], 5, 0);
echo count($right) . ":" . $right[0] . $right[1] . $right[2] . $right[4];
$left = array_pad([1, 2], -4, 9);
echo ":" . $left[0] . $left[1] . $left[2] . $left[3];
$copy = array_pad([7, 8], 1, 0);
echo ":" . count($copy) . ":" . $copy[0] . $copy[1];
$chunks = array_chunk([1, 2, 3, 4, 5], 2);
echo ":" . count($chunks) . ":" . $chunks[0][1] . $chunks[2][0];
$named = array_pad(array: ["a"], length: 2, value: "b");
echo ":" . $named[1];
$call = call_user_func("array_chunk", [6, 7, 8], 2);
echo ":" . $call[1][0];
$spread = call_user_func_array("array_pad", [[1], 3, 2]);
echo ":" . $spread[2] . ":";
echo function_exists("array_pad"); echo function_exists("array_chunk");');
"#,
    );
    assert_eq!(out, "5:1200:9912:2:78:3:25:b:8:2:11");
}

/// Verifies eval `array_slice()` observes PHP offset and length bounds.
#[test]
fn test_eval_dispatches_array_slice_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$mid = array_slice([10, 20, 30, 40, 50], 1, 3);
echo count($mid) . ":" . $mid[0] . $mid[1] . $mid[2];
$tail = array_slice([10, 20, 30, 40], -2, 1);
echo ":" . $tail[0];
$open = array_slice([10, 20, 30, 40, 50], 2);
echo ":" . count($open) . ":" . $open[0] . $open[2];
$null_len = array_slice([5, 6, 7], 1, null);
echo ":" . $null_len[0] . $null_len[1];
$negative_len = array_slice([10, 20, 30, 40, 50], 1, -1);
echo ":" . count($negative_len) . ":" . $negative_len[0] . $negative_len[2];
$named = array_slice(array: [1, 2, 3], offset: 1, length: 1);
echo ":" . $named[0];
$call = call_user_func("array_slice", [6, 7, 8], 1, 2);
echo ":" . $call[1];
$spread = call_user_func_array("array_slice", [[9, 10, 11], 1]);
echo ":" . $spread[0] . ":";
echo function_exists("array_slice");');
"#,
    );
    assert_eq!(out, "3:203040:30:3:3050:67:3:2040:2:8:10:1");
}

/// Verifies eval `array_merge()` appends numeric keys and overwrites string keys.
#[test]
fn test_eval_dispatches_array_merge_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$merged = array_merge([1, 2], [3, 4]);
echo count($merged) . ":" . $merged[0] . $merged[1] . $merged[2] . $merged[3];
$left = [1, 2];
$right = [3];
$copy = array_merge($left, $right);
echo ":" . count($left) . ":" . $left[0] . ":" . $copy[2];
$assoc = array_merge(["a" => 1, 2 => "x"], ["a" => 9, 5 => "y", "b" => 3]);
echo ":" . $assoc["a"] . ":" . $assoc[0] . ":" . $assoc[1] . ":" . $assoc["b"];
$call = call_user_func("array_merge", [6], [7, 8]);
echo ":" . $call[2];
$spread = call_user_func_array("array_merge", [[9], [10]]);
echo ":" . $spread[1] . ":";
echo function_exists("array_merge");');
"#,
    );
    assert_eq!(out, "4:1234:2:1:3:9:x:y:3:8:10:1");
}

/// Verifies eval `array_diff()` and `array_intersect()` preserve left keys and compare by string value.
#[test]
fn test_eval_dispatches_array_value_set_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$diff = array_diff(["a" => 1, "b" => 2, "c" => "2", "d" => 3], [2]);
echo count($diff) . ":" . $diff["a"] . ":" . $diff["d"];
echo ":" . (array_key_exists("b", $diff) ? "bad" : "no-b");
echo ":" . (array_key_exists("c", $diff) ? "bad" : "no-c");
$inter = array_intersect(["a" => 1, "b" => 2, "c" => "2", "d" => 3], ["2", 4]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter["c"];
$call = call_user_func("array_diff", [1, 2, 3], [2]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect", [[1, 2, 3], [3]]);
echo ":" . count($spread) . ":" . $spread[2] . ":";
echo function_exists("array_diff"); echo function_exists("array_intersect");');
"#,
    );
    assert_eq!(out, "2:1:3:no-b:no-c:2:2:2:2:13:1:3:11");
}

/// Verifies eval `array_diff_key()` and `array_intersect_key()` preserve first-array keys.
#[test]
fn test_eval_dispatches_array_key_set_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$diff = array_diff_key(["a" => 1, "b" => 2, 4 => 3], ["a" => 0, 5 => 0]);
echo count($diff) . ":" . $diff["b"] . ":" . $diff[4];
echo ":" . (array_key_exists("a", $diff) ? "bad" : "no-a");
$inter = array_intersect_key(["a" => 1, "b" => 2, 4 => 3], ["b" => 0, 4 => 0]);
echo ":" . count($inter) . ":" . $inter["b"] . ":" . $inter[4];
$call = call_user_func("array_diff_key", [10, 20, 30], [1 => 0]);
echo ":" . count($call) . ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_intersect_key", [["x" => 7, "y" => 8], ["y" => 0]]);
echo ":" . count($spread) . ":" . $spread["y"] . ":";
echo function_exists("array_diff_key"); echo function_exists("array_intersect_key");');
"#,
    );
    assert_eq!(out, "2:2:3:no-a:2:2:3:2:1030:1:8:11");
}

/// Verifies eval `range()` builds inclusive ascending and descending integer arrays.
#[test]
fn test_eval_dispatches_range_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$up = range(1, 4);
echo count($up) . ":" . $up[0] . $up[3];
$down = range(4, 1);
echo ":" . count($down) . ":" . $down[0] . $down[3];
$single = range(3, 3);
echo ":" . count($single) . ":" . $single[0];
$named = range(start: 2, end: 4);
echo ":" . $named[0] . $named[2];
$call = call_user_func("range", 5, 7);
echo ":" . $call[2];
$spread = call_user_func_array("range", [8, 6]);
echo ":" . count($spread) . ":" . $spread[0] . $spread[2] . ":";
echo function_exists("range");');
"#,
    );
    assert_eq!(out, "4:14:4:41:1:3:24:7:3:86:1");
}

/// Verifies eval `array_rand()` returns a key that exists in the source array.
#[test]
fn test_eval_dispatches_array_rand_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$nums = [10, 20, 30];
$idx = array_rand($nums);
echo ($idx >= 0 && $idx < 3 && array_key_exists($idx, $nums)) ? "idx" : "bad";
$assoc = ["a" => 1, "b" => 2];
$key = array_rand($assoc);
echo ":" . (array_key_exists($key, $assoc) ? "assoc" : "bad");
$named = array_rand(array: [5, 6]);
echo ":" . (($named >= 0 && $named < 2) ? "named" : "bad");
$call = call_user_func("array_rand", [7, 8]);
echo ":" . (($call >= 0 && $call < 2) ? "call" : "bad");
$spread = call_user_func_array("array_rand", [["x" => 1, "y" => 2]]);
echo ":" . (array_key_exists($spread, ["x" => 1, "y" => 2]) ? "spread" : "bad") . ":";
echo function_exists("array_rand");');
"#,
    );
    assert_eq!(out, "idx:assoc:named:call:spread:1");
}

/// Verifies eval random builtins produce values in their PHP-visible ranges.
#[test]
fn test_eval_dispatches_rand_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$plain = rand();
echo ($plain >= 0 && $plain <= 2147483647) ? "plain" : "bad";
$bounded = rand(2, 4);
echo ":" . (($bounded >= 2 && $bounded <= 4) ? "range" : "bad");
$same = mt_rand(max: 6, min: 6);
echo ":" . ($same === 6 ? "same" : "bad");
$swapped = rand(10, 1);
echo ":" . (($swapped >= 1 && $swapped <= 10) ? "swap" : "bad");
$call = call_user_func("mt_rand", 1, 1);
echo ":" . ($call === 1 ? "call" : "bad");
$spread = call_user_func_array("rand", ["min" => 3, "max" => 3]);
echo ":" . ($spread === 3 ? "spread" : "bad") . ":";
$secure = random_int(max: 4, min: 4);
echo ($secure === 4 ? "random" : "bad") . ":";
$secureCall = call_user_func("random_int", 5, 5);
echo ($secureCall === 5 ? "random-call" : "bad") . ":";
$secureSpread = call_user_func_array("random_int", ["min" => 6, "max" => 6]);
echo ($secureSpread === 6 ? "random-spread" : "bad") . ":";
echo function_exists("rand"); echo function_exists("mt_rand"); echo function_exists("random_int");');
"#,
    );
    assert_eq!(
        out,
        "plain:range:same:swap:call:spread:random:random-call:random-spread:111"
    );
}

/// Verifies eval `spl_classes()` exposes the same static SPL class list as native code.
#[test]
fn test_eval_dispatches_spl_classes_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$names = spl_classes();
echo count($names) . ":" . $names[0] . ":" . $names[55] . ":";
echo (in_array("Exception", $names) ? "exception" : "bad") . ":";
echo (in_array("SplDoublyLinkedList", $names) ? "list" : "bad") . ":";
$call = call_user_func("spl_classes");
echo (in_array("Throwable", $call) ? "call" : "bad") . ":";
$spread = call_user_func_array("spl_classes", []);
echo (count($spread) === count($names) ? "spread" : "bad") . ":";
echo function_exists("spl_classes"); echo is_callable("spl_classes");');
"#,
    );
    assert_eq!(
        out,
        "61:AppendIterator:Throwable:exception:list:call:spread:11"
    );
}

/// Verifies eval fragments can construct and dispatch SPL container objects.
#[test]
fn test_eval_constructs_and_dispatches_spl_container_objects() {
    let out = compile_and_run_capture(
        r#"<?php
eval('$list = new SplDoublyLinkedList();
$list->push("a");
$list->push("b");
echo count($list) . ":" . $list->bottom() . ":" . $list->top() . ":";
foreach ($list as $value) {
    echo $value;
}
$stack = new SplStack();
$stack->push("s");
echo ":" . count($stack) . $stack->pop();
$queue = new SplQueue();
$queue->enqueue("q");
echo ":" . count($queue) . $queue->dequeue();
$emptyFixed = new SplFixedArray();
echo ":" . $emptyFixed->getSize();
$fixed = new SplFixedArray(2);
$fixed->offsetSet(0, "x");
echo ":" . $fixed->getSize() . $fixed->offsetGet(0);
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:a:b:ab:1s:1q:0:2x");
}

/// Verifies eval `array_fill()` and `array_fill_keys()` create arrays with PHP key rules.
#[test]
fn test_eval_dispatches_array_fill_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$filled = array_fill(2, 3, "x");
echo count($filled) . ":" . $filled[2] . $filled[4];
$negative = array_fill(-2, 3, 7);
echo ":" . $negative[-2] . $negative[-1] . $negative[0];
$empty = array_fill(5, 0, "x");
echo ":" . count($empty);
$map = array_fill_keys(["a", "1", "01"], 8);
echo ":" . $map["a"] . ":" . $map[1] . ":" . $map["01"];
$named = array_fill(start_index: 1, count: 2, value: "n");
echo ":" . $named[1] . $named[2];
$call = call_user_func("array_fill", 0, 2, "c");
echo ":" . $call[0] . $call[1];
$spread = call_user_func_array("array_fill_keys", [["x", "y"], "z"]);
echo ":" . $spread["x"] . $spread["y"] . ":";
echo function_exists("array_fill"); echo function_exists("array_fill_keys");');
"#,
    );
    assert_eq!(out, "3:xx:777:0:8:8:8:nn:cc:zz:11");
}

/// Verifies eval `array_flip()` supports PHP key rules and callable dispatch.
#[test]
fn test_eval_dispatches_array_flip_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$flipped = array_flip(["a" => "x", "b" => "y", "c" => "x", "d" => 1, "e" => "01", "skip" => null, "truth" => true]);
echo $flipped["x"] . ":" . $flipped["y"] . ":" . $flipped[1] . ":" . $flipped["01"] . ":" . count($flipped);
$named = array_flip(array: ["k" => "v"]);
echo ":" . $named["v"];
$call = call_user_func("array_flip", ["left" => "right"]);
echo ":" . $call["right"];
$spread = call_user_func_array("array_flip", [["n" => 9]]);
echo ":" . $spread[9] . ":";
echo function_exists("array_flip");');
"#,
    );
    assert_eq!(out, "c:b:d:e:4:k:left:n:1");
}

/// Verifies eval `array_unique()` preserves keys and supports callable dispatch.
#[test]
fn test_eval_dispatches_array_unique_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$unique = array_unique(["a", "b", "a", "2", 2]);
echo count($unique) . ":" . $unique[0] . $unique[1] . $unique[3];
$assoc = array_unique(["x" => "a", "y" => "b", "z" => "a"]);
echo ":" . count($assoc) . ":" . $assoc["x"] . $assoc["y"];
$scalar = array_unique([1, "1", 1.0, true, false, null, ""]);
echo ":" . count($scalar) . ":" . $scalar[0] . ":";
echo $scalar[4] ? "bad" : "F";
$named = array_unique(array: ["k" => "v", "l" => "v"]);
echo ":" . $named["k"] . ":" . count($named);
$call = call_user_func("array_unique", ["q", "q", "r"]);
echo ":" . $call[0] . $call[2];
$spread = call_user_func_array("array_unique", [["s", "s", "t"]]);
echo ":" . $spread[0] . $spread[2] . ":";
echo function_exists("array_unique");');
"#,
    );
    assert_eq!(out, "3:ab2:2:ab:2:1:F:v:1:qr:st:1");
}

/// Verifies eval array projection builtins return indexed key/value arrays.
#[test]
fn test_eval_dispatches_array_projection_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$values = array_values(["a" => 10, "b" => 20]);
echo $values[0] . ":" . $values[1];
$keys = array_keys(["a" => 10, "b" => 20]);
echo ":" . $keys[0] . ":" . $keys[1];
echo ":" . count(array_values([]));
$call_keys = call_user_func("array_keys", ["z" => 7]);
echo ":" . $call_keys[0];
$call_values = call_user_func_array("array_values", [["q" => 8]]);
echo ":" . $call_values[0];
echo ":"; echo function_exists("array_keys"); echo function_exists("array_values");');
"#,
    );
    assert_eq!(out, "10:20:a:b:0:z:8:11");
}

/// Verifies eval `array_reverse()` supports key rules, named args, and callable dispatch.
#[test]
fn test_eval_dispatches_array_reverse_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$indexed = array_reverse([1, 2, 3]);
echo $indexed[0]; echo $indexed[1]; echo $indexed[2]; echo ":";
$mixed = array_reverse([2 => "a", "k" => "b", 5 => "c"]);
echo $mixed[0]; echo $mixed["k"]; echo $mixed[1]; echo ":";
$preserved = array_reverse([2 => "a", "k" => "b", 5 => "c"], true);
echo $preserved[5]; echo $preserved["k"]; echo $preserved[2]; echo ":";
$named = array_reverse(array: ["x", "y"], preserve_keys: true);
echo $named[1]; echo $named[0]; echo ":";
$call = call_user_func("array_reverse", [4, 5]);
echo $call[0]; echo $call[1]; echo ":";
$spread = call_user_func_array("array_reverse", [[6, 7]]);
echo $spread[0]; echo $spread[1]; echo ":";
echo function_exists("array_reverse");');
"#,
    );
    assert_eq!(out, "321:cba:cba:yx:54:76:1");
}

/// Verifies eval `array_key_exists()` distinguishes present null values from missing keys.
#[test]
fn test_eval_dispatches_array_key_exists_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$map = ["name" => null, "age" => 30];
echo array_key_exists("name", $map) ? "Y" : "N"; echo ":";
echo array_key_exists("missing", $map) ? "bad" : "N"; echo ":";
echo array_key_exists(1, [10, null]) ? "Y" : "N"; echo ":";
echo array_key_exists(2, [10, null]) ? "bad" : "N"; echo ":";
echo call_user_func("array_key_exists", "age", $map) ? "Y" : "N"; echo ":";
echo call_user_func_array("array_key_exists", ["age", $map]) ? "Y" : "N";
echo ":"; echo function_exists("array_key_exists");');
"#,
    );
    assert_eq!(out, "Y:N:Y:N:Y:Y:1");
}

/// Verifies eval array search builtins return booleans or matching keys.
#[test]
fn test_eval_dispatches_array_search_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo in_array(2, [1, 2, 3]) ? "Y" : "bad";
echo ":"; echo in_array(4, [1, 2, 3]) ? "bad" : "N";
echo ":" . array_search(20, [10, 20, 30]);
echo ":" . array_search("Grace", ["name" => "Grace"]);
echo ":"; echo array_search("x", ["name" => "Grace"]) === false ? "F" : "bad";
echo ":"; echo call_user_func("in_array", "b", ["a", "b"]) ? "C" : "bad";
$found = call_user_func_array("array_search", ["v", ["k" => "v"]]);
echo ":" . $found;
echo ":"; echo function_exists("in_array"); echo function_exists("array_search");');
"#,
    );
    assert_eq!(out, "Y:N:1:name:F:C:k:11");
}

/// Verifies eval ASCII case-conversion builtins work directly and by callable dispatch.
#[test]
fn test_eval_dispatches_string_case_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strtoupper("Hello World"); echo ":";
echo strtolower("LOUD"); echo ":";
echo ucfirst("eval"); echo ":";
echo lcfirst("LOUD"); echo ":";
echo call_user_func("strtoupper", "xy"); echo ":";
echo call_user_func_array("strtolower", ["ZZ"]); echo ":";
echo call_user_func("ucfirst", "case"); echo ":";
echo call_user_func_array("lcfirst", ["CASE"]);
echo ":"; echo function_exists("strtoupper"); echo function_exists("strtolower"); echo function_exists("ucfirst"); echo function_exists("lcfirst");');
"#,
    );
    assert_eq!(out, "HELLO WORLD:loud:Eval:lOUD:XY:zz:Case:cASE:1111");
}

/// Verifies eval `ucwords()` capitalizes words directly and by callable dispatch.
#[test]
fn test_eval_dispatches_ucwords_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo ucwords("hello world"); echo ":";
echo ucwords(string: "hello-world", separators: "-"); echo ":";
echo ucwords("hello\tworld"); echo ":";
echo call_user_func("ucwords", "a b"); echo ":";
echo call_user_func_array("ucwords", ["string" => "a-b", "separators" => "-"]);
echo ":"; echo function_exists("ucwords");');
"#,
    );
    assert_eq!(out, "Hello World:Hello-World:Hello\tWorld:A B:A-B:1");
}

/// Verifies eval `wordwrap()` wraps at word boundaries and can cut long words.
#[test]
fn test_eval_dispatches_wordwrap_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo wordwrap("The quick brown fox", 10, "|"); echo ":";
echo wordwrap(string: "A verylongword here", width: 8, break: "|"); echo ":";
echo wordwrap("abcdefghij", 4, "|", true); echo ":";
echo wordwrap("preserve\nnewlines here ok", 10, "|"); echo ":";
echo call_user_func("wordwrap", "aaa bbb ccc", 3, "<br>"); echo ":";
echo call_user_func_array("wordwrap", ["string" => "hello world", "width" => 5, "break" => "|"]);
echo ":"; echo function_exists("wordwrap");');
"#,
    );
    assert_eq!(
        out,
        "The quick|brown fox:A|verylongword|here:abcd|efgh|ij:preserve\nnewlines|here ok:aaa<br>bbb<br>ccc:hello|world:1"
    );
}

/// Verifies eval `strrev()` reverses byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_strrev_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strrev("Hello"); echo ":";
echo strrev(123); echo ":";
echo call_user_func("strrev", "ABC"); echo ":";
echo call_user_func_array("strrev", ["def"]);
echo ":"; echo function_exists("strrev");');
"#,
    );
    assert_eq!(out, "olleH:321:CBA:fed:1");
}

/// Verifies eval `chr()` returns single-byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_chr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo chr(65); echo ":";
echo bin2hex(chr(codepoint: 256)); echo ":";
echo bin2hex(call_user_func("chr", -1)); echo ":";
echo call_user_func_array("chr", ["codepoint" => 321]);
echo ":"; echo function_exists("chr");');
"#,
    );
    assert_eq!(out, "A:00:ff:A:1");
}

/// Verifies eval `str_repeat()` repeats byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_str_repeat_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_repeat("ha", 3); echo ":";
echo strlen(str_repeat(string: "x", times: 0)); echo ":";
echo call_user_func("str_repeat", "ab", 2); echo ":";
echo call_user_func_array("str_repeat", ["string" => "z", "times" => 3]);
echo ":"; echo function_exists("str_repeat");');
"#,
    );
    assert_eq!(out, "hahaha:0:abab:zzz:1");
}

/// Verifies eval `substr()` slices byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_substr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo substr("abcdef", 2); echo ":";
echo substr(string: "abcdef", offset: 1, length: -1); echo ":";
echo substr("abcdef", -2); echo ":";
echo call_user_func("substr", "abcdef", 2, -2); echo ":";
echo call_user_func_array("substr", ["string" => "abcdef", "offset" => -4, "length" => 2]);
echo ":"; echo function_exists("substr");');
"#,
    );
    assert_eq!(out, "cdef:bcde:ef:cd:cd:1");
}

/// Verifies eval `substr_replace()` replaces selected byte ranges through callable paths.
#[test]
fn test_eval_dispatches_substr_replace_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo substr_replace("hello world", "PHP", 6, 5); echo ":";
echo substr_replace(string: "abcdef", replace: "X", offset: 1, length: -1); echo ":";
echo substr_replace("abcdef", "X", -2); echo ":";
echo call_user_func("substr_replace", "abcdef", "X", 99, 1); echo ":";
echo call_user_func_array("substr_replace", ["string" => "abcdef", "replace" => "X", "offset" => -99, "length" => 2]);
echo ":"; echo function_exists("substr_replace");');
"#,
    );
    assert_eq!(out, "hello PHP:aXf:abcdX:abcdefX:Xcdef:1");
}

/// Verifies eval `nl2br()` preserves newline bytes while inserting HTML breaks.
#[test]
fn test_eval_dispatches_nl2br_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo bin2hex(nl2br("a\nb")); echo ":";
echo bin2hex(nl2br(string: "a\nb", use_xhtml: false)); echo ":";
echo bin2hex(call_user_func("nl2br", "a\r\nb")); echo ":";
echo bin2hex(call_user_func_array("nl2br", ["string" => "a\n\rb", "use_xhtml" => false]));
echo ":"; echo function_exists("nl2br");');
"#,
    );
    assert_eq!(
        out,
        "613c6272202f3e0a62:613c62723e0a62:613c6272202f3e0d0a62:613c62723e0a0d62:1"
    );
}

/// Verifies eval `explode()` and `implode()` bridge byte strings and arrays.
#[test]
fn test_eval_dispatches_explode_implode_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$parts = explode(",", "a,b,");
echo count($parts); echo ":" . $parts[0] . ":" . $parts[1] . ":" . $parts[2];
echo ":" . implode("|", $parts);
echo ":" . implode(separator: "-", array: ["x", 2, true, null]);
$call_parts = call_user_func("explode", ":", "m:n");
echo ":" . $call_parts[1];
echo ":" . call_user_func_array("implode", ["separator" => "/", "array" => ["p", "q"]]);
echo ":"; echo function_exists("explode");
echo ":"; echo function_exists("implode");');
"#,
    );
    assert_eq!(out, "3:a:b::a|b|:x-2-1-:n:p/q:1:1");
}

/// Verifies eval `str_split()` builds indexed chunk arrays.
#[test]
fn test_eval_dispatches_str_split_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$letters = str_split("abc");
echo count($letters) . ":" . $letters[0] . $letters[1] . $letters[2]; echo ":";
$pairs = str_split(string: "abcd", length: 2);
echo $pairs[0] . "-" . $pairs[1]; echo ":";
$empty = str_split("");
echo count($empty); echo ":";
$call = call_user_func("str_split", "xyz", 2);
echo $call[0] . "-" . $call[1]; echo ":";
$named = call_user_func_array("str_split", ["string" => "pqrs", "length" => 3]);
echo $named[0] . "-" . $named[1];
echo ":"; echo function_exists("str_split");');
"#,
    );
    assert_eq!(out, "3:abc:ab-cd:0:xy-z:pqr-s:1");
}

/// Verifies eval `str_pad()` supports all PHP pad modes and callable dispatch.
#[test]
fn test_eval_dispatches_str_pad_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo "[" . str_pad("hi", 5) . "]"; echo ":";
echo "[" . str_pad(string: "hi", length: 5, pad_string: "_", pad_type: 0) . "]"; echo ":";
echo "[" . str_pad("x", 6, "ab", 2) . "]"; echo ":";
echo call_user_func("str_pad", "42", 5, "0", 0); echo ":";
echo call_user_func_array("str_pad", ["string" => "x", "length" => 3, "pad_string" => "."]);
echo ":"; echo function_exists("str_pad");');
"#,
    );
    assert_eq!(out, "[hi   ]:[___hi]:[abxaba]:00042:x..:1");
}

/// Verifies eval string replacement builtins support direct and callable dispatch.
#[test]
fn test_eval_dispatches_string_replace_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_replace("o", "0", "Hello World"); echo ":";
echo str_replace(search: "aa", replace: "b", subject: "aaaa"); echo ":";
echo str_replace("", "x", "abc"); echo ":";
echo str_ireplace("HE", "ye", "Hello he"); echo ":";
echo call_user_func("str_replace", "l", "L", "hello"); echo ":";
echo call_user_func_array("str_ireplace", ["search" => "x", "replace" => "Y", "subject" => "xX"]);
echo ":"; echo function_exists("str_replace");
echo ":"; echo function_exists("str_ireplace");');
"#,
    );
    assert_eq!(out, "Hell0 W0rld:bb:abc:yello ye:heLLo:YY:1:1");
}

/// Verifies eval HTML entity builtins encode, decode, and dispatch as callables.
#[test]
fn test_eval_dispatches_html_entity_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo htmlspecialchars("<b>\"Hi\" & \'bye\'</b>"); echo ":";
echo htmlentities(string: "<a>"); echo ":";
echo html_entity_decode("&lt;b&gt;hi&lt;/b&gt;"); echo ":";
echo call_user_func("htmlspecialchars", "<x>"); echo ":";
echo call_user_func_array("html_entity_decode", ["string" => "&quot;q&quot;"]);
echo ":"; echo function_exists("htmlspecialchars");
echo ":"; echo function_exists("htmlentities");
echo ":"; echo function_exists("html_entity_decode");');
"#,
    );
    assert_eq!(
        out,
        "&lt;b&gt;&quot;Hi&quot; &amp; &#039;bye&#039;&lt;/b&gt;:&lt;a&gt;:<b>hi</b>:&lt;x&gt;:\"q\":1:1:1"
    );
}

/// Verifies eval URL codec builtins encode, decode, and dispatch as callables.
#[test]
fn test_eval_dispatches_url_codec_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo urlencode("a b&=~"); echo ":";
echo rawurlencode(string: "a b&=~"); echo ":";
echo urldecode("a+b%26%3D%7E"); echo ":";
echo rawurldecode("a+b%26%3D%7E"); echo ":";
echo call_user_func("urlencode", "%zz"); echo ":";
echo call_user_func_array("rawurldecode", ["string" => "x%2By%zz"]);
echo ":"; echo function_exists("urlencode");
echo ":"; echo function_exists("rawurlencode");
echo ":"; echo function_exists("urldecode");
echo ":"; echo function_exists("rawurldecode");');
"#,
    );
    assert_eq!(
        out,
        "a+b%26%3D%7E:a%20b%26%3D~:a b&=~:a+b&=~:%25zz:x+y%zz:1:1:1:1"
    );
}

/// Verifies eval `ctype_*` predicates inspect ASCII byte classes.
#[test]
fn test_eval_dispatches_ctype_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo ctype_alpha("abc") ? "A" : "-"; echo ":";
echo ctype_digit(text: "123") ? "D" : "-"; echo ":";
echo ctype_alnum("a1") ? "N" : "-"; echo ":";
echo ctype_space(" \t\n" . chr(11) . chr(12) . "\r") ? "S" : "-"; echo ":";
echo ctype_alpha("") ? "bad" : "empty"; echo ":";
echo call_user_func("ctype_digit", "12x") ? "bad" : "not-digit"; echo ":";
echo call_user_func_array("ctype_space", ["text" => " x"]) ? "bad" : "not-space";
echo ":"; echo function_exists("ctype_alpha");
echo ":"; echo function_exists("ctype_digit");
echo ":"; echo function_exists("ctype_alnum");
echo ":"; echo function_exists("ctype_space");');
"#,
    );
    assert_eq!(out, "A:D:N:S:empty:not-digit:not-space:1:1:1:1");
}

/// Verifies eval `crc32()` returns PHP-compatible non-negative checksums.
#[test]
fn test_eval_dispatches_crc32_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo crc32(""); echo ":";
echo crc32(string: "123456789"); echo ":";
echo call_user_func("crc32", "hello"); echo ":";
echo call_user_func_array("crc32", ["string" => "The quick brown fox jumps over the lazy dog"]);
echo ":"; echo function_exists("crc32");');
"#,
    );
    assert_eq!(out, "0:3421780262:907060870:1095738169:1");
}

/// Verifies eval `hash_algos()` exposes the native supported hash algorithm list.
#[test]
fn test_eval_dispatches_hash_algos_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$algos = hash_algos();
echo count($algos) . ":" . $algos[0] . ":" . $algos[5] . ":";
echo in_array("crc32c", $algos) ? "crc" : "bad";
$call = call_user_func("hash_algos");
echo ":" . $call[18];
$spread = call_user_func_array("hash_algos", []);
echo ":" . $spread[27] . ":";
echo function_exists("hash_algos") ? "exists" : "missing";');
"#,
    );
    assert_eq!(out, "28:md2:sha256:crc:whirlpool:joaat:exists");
}

/// Verifies eval one-shot hash digest builtins use the crypto bridge.
#[test]
fn test_eval_dispatches_hash_digest_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo md5("abc"); echo ":";
echo sha1(string: "abc"); echo ":";
echo hash("sha256", "abc"); echo ":";
echo hash_hmac(algo: "sha256", data: "data", key: "key"); echo ":";
echo bin2hex(md5("abc", true)); echo ":";
echo bin2hex(call_user_func("sha1", "abc", true)); echo ":";
echo call_user_func_array("hash", ["algo" => "md5", "data" => "abc"]); echo ":";
echo call_user_func_array("hash_hmac", ["algo" => "sha256", "data" => "data", "key" => "key"]); echo ":";
file_put_contents("eval-hash-file.txt", "abc");
echo hash_file("sha256", "eval-hash-file.txt"); echo ":";
echo bin2hex(hash_file(algo: "md5", filename: "eval-hash-file.txt", binary: true)); echo ":";
echo call_user_func_array("hash_file", ["algo" => "md5", "filename" => "eval-hash-file.txt"]); echo ":";
echo hash_file("sha256", "eval-hash-file.txt.missing") === false ? "missing" : "bad"; echo ":";
unlink("eval-hash-file.txt");
echo function_exists("md5"); echo function_exists("sha1"); echo function_exists("hash"); echo function_exists("hash_file"); echo function_exists("hash_hmac");');
"#,
    );
    assert_eq!(
        out,
        concat!(
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "a9993e364706816aba3e25717850c26c9cd0d89d:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0:",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "900150983cd24fb0d6963f7d28e17f72:",
            "missing:",
            "11111"
        )
    );
}

/// Verifies eval zero-argument system builtins match native runtime conventions.
#[test]
fn test_eval_dispatches_zero_arg_system_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo time() > 1000000000 ? "time" : "bad"; echo ":";
echo phpversion(); echo ":";
echo sys_get_temp_dir(); echo ":";
echo strlen(getcwd()) > 0 ? "cwd" : "bad"; echo ":";
echo call_user_func("time") > 1000000000 ? "call-time" : "bad"; echo ":";
echo call_user_func("phpversion"); echo ":";
echo call_user_func_array("getcwd", []) !== "" ? "call-cwd" : "bad"; echo ":";
echo call_user_func_array("sys_get_temp_dir", []); echo ":";
echo function_exists("time"); echo function_exists("phpversion"); echo function_exists("getcwd");
echo function_exists("sys_get_temp_dir");');
"#,
    );
    assert_eq!(
        out,
        format!(
            "time:{}:/tmp:cwd:call-time:{}:call-cwd:/tmp:1111",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION")
        )
    );
}

/// Verifies eval `date()` formats timestamps and `mktime()` creates them.
#[test]
fn test_eval_dispatches_date_mktime_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$ts = mktime(13, 2, 3, 1, 2, 2024);
echo date("Y-m-d H:i:s", $ts);
echo ":" . date("j-n-G-g-A-a-N-D-M-l-F", $ts);
echo ":" . (date("U", $ts) === strval($ts) ? "U" : "bad");
echo ":" . call_user_func("date", "Y", $ts);
$named = call_user_func_array("mktime", ["hour" => 0, "minute" => 0, "second" => 0, "month" => 1, "day" => 1, "year" => 2000]);
echo ":" . date(format: "Y", timestamp: $named);
echo ":"; echo function_exists("date"); echo function_exists("mktime");');
"#,
    );
    assert_eq!(
        out,
        "2024-01-02 13:02:03:2-1-13-1-PM-pm-2-Tue-Jan-Tuesday-January:U:2024:2000:11"
    );
}

/// Verifies eval function probes recognize the DateTime/calendar aliases that static elephc
/// desugars before codegen.
#[test]
fn test_eval_function_probes_date_procedural_aliases() {
    let out = compile_and_run(
        r#"<?php
eval('$aliases = [
    "idate", "mktime", "gmmktime", "date_create", "date_create_immutable",
    "date_create_from_format", "date_create_immutable_from_format",
    "date_parse_from_format", "date_parse", "date_sun_info", "date_sunrise",
    "date_sunset", "strptime", "timezone_name_from_abbr", "cal_to_jd",
    "cal_from_jd", "cal_days_in_month", "cal_info", "gregoriantojd",
    "jdtogregorian", "juliantojd", "jdtojulian", "frenchtojd",
    "jdtofrench", "jewishtojd", "jdtojewish", "jddayofweek", "jdmonthname",
    "jdtounix", "unixtojd", "easter_days", "easter_date", "gettimeofday",
    "date_get_last_errors", "strftime", "gmstrftime", "timezone_open",
    "timezone_identifiers_list", "timezone_location_get",
    "timezone_transitions_get", "timezone_abbreviations_list",
    "timezone_version_get", "date_interval_create_from_date_string",
    "date_diff", "date_format", "date_add", "date_sub", "date_modify",
    "date_timestamp_get", "date_timestamp_set", "date_timezone_get",
    "date_timezone_set", "date_offset_get", "date_date_set",
    "date_isodate_set", "date_time_set", "date_interval_format",
    "timezone_name_get", "timezone_offset_get"
];
foreach ($aliases as $alias) {
    echo function_exists($alias) ? "1" : "0";
}
echo ":";
echo function_exists("Date_Create") ? "C" : "c";
echo function_exists("EvalAlias\\idate") ? "N" : "n";
echo is_callable("timezone_version_get") ? "I" : "i";
echo function_exists("does_not_exist_alias_xyz") ? "x" : "X";');
"#,
    );
    assert_eq!(out, "1".repeat(59) + ":CNIX");
}

/// Verifies eval can execute DateTime-family procedural aliases without static DateTime references.
#[test]
fn test_eval_dispatches_datetime_procedural_aliases_without_static_references() {
    let out = compile_and_run(
        r#"<?php
eval('$d = date_create("2024-01-02 03:04:05");
echo date_format($d, "Y-m-d H:i:s");
echo ":" . call_user_func("date_format", $d, "Y");
$fmt = date_format(...);
echo ":" . $fmt($d, "m");
$tz = timezone_open("UTC");
echo ":" . timezone_name_get($tz);
$iv = date_interval_create_from_date_string("1 day");
echo ":" . date_interval_format($iv, "%d");
echo ":" . (timezone_version_get() === "" ? "bad" : "version");');
"#,
    );
    assert_eq!(out, "2024-01-02 03:04:05:2024:01:UTC:1:version");
}

/// Verifies eval can execute timezone-introspection aliases without static DateTimeZone references.
#[test]
fn test_eval_dispatches_timezone_introspection_aliases_without_static_references() {
    let out = compile_and_run(
        r#"<?php
eval('$paris = timezone_open("Europe/Paris");
$loc = timezone_location_get($paris);
echo $loc["country_code"] . ":" . $loc["latitude"] . ":" . $loc["longitude"];
$transitions = timezone_transitions_get($paris, mktime(0,0,0,1,1,2020), mktime(0,0,0,6,1,2021));
echo ":" . count($transitions) . ":" . $transitions[0]["abbr"] . ":" . $transitions[3]["abbr"];
$abbr = timezone_abbreviations_list();
echo ":" . count($abbr) . ":" . $abbr["acdt"][0]["timezone_id"];
$ids = timezone_identifiers_list(DateTimeZone::PACIFIC);
echo ":" . count($ids) . ":" . $ids[0];');
"#,
    );
    assert_eq!(
        out,
        "FR:48.86666:2.33333:4:CET:CEST:144:Australia/Adelaide:38:Pacific/Apia"
    );
}

/// Verifies eval `strtotime()` parses supported ISO date strings and rejects others.
#[test]
fn test_eval_dispatches_strtotime_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$date = strtotime("2024-06-15");
echo date("Y-m-d H:i:s", $date);
$full = strtotime("2024-06-15 12:30:45");
echo ":" . date("Y-m-d H:i:s", $full);
$short = strtotime("2024-06-15T12:30");
echo ":" . date("Y-m-d H:i:s", $short);
echo ":" . (strtotime("2024/06/15") === -1 ? "bad" : "wrong");
$call = call_user_func("strtotime", "2024-01-02 03:04:05");
echo ":" . date("Y-m-d H:i:s", $call);
$spread = call_user_func_array("strtotime", ["datetime" => "2024-01-02"]);
echo ":" . date("Y-m-d", $spread) . ":";
echo function_exists("strtotime");');
"#,
    );
    assert_eq!(
        out,
        "2024-06-15 00:00:00:2024-06-15 12:30:45:2024-06-15 12:30:00:bad:2024-01-02 03:04:05:2024-01-02:1"
    );
}

/// Verifies eval `microtime()` returns a plausible floating timestamp by all call paths.
#[test]
fn test_eval_dispatches_microtime_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo microtime() > 1000000000 ? "now" : "bad"; echo ":";
echo microtime(as_float: false) > 1000000000 ? "named" : "bad"; echo ":";
echo call_user_func("microtime", true) > 1000000000 ? "call" : "bad"; echo ":";
echo call_user_func_array("microtime", ["as_float" => true]) > 1000000000 ? "array" : "bad";
echo ":"; echo function_exists("microtime");');
"#,
    );
    assert_eq!(out, "now:named:call:array:1");
}

/// Verifies eval realpath-cache builtins expose elephc's empty-cache convention.
#[test]
fn test_eval_dispatches_realpath_cache_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$cache = realpath_cache_get();
echo count($cache) . ":" . realpath_cache_size() . ":";
$call_cache = call_user_func("realpath_cache_get");
echo count($call_cache) . ":";
echo call_user_func_array("realpath_cache_size", []) . ":";
echo function_exists("realpath_cache_get");
echo function_exists("realpath_cache_size");');
"#,
    );
    assert_eq!(out, "0:0:0:0:11");
}

/// Verifies eval environment builtins read, write, unset, and dispatch as callables.
#[test]
fn test_eval_dispatches_environment_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('putenv("ELEPHC_EVAL_ENV_TEST=direct");
echo getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv(assignment: "ELEPHC_EVAL_ENV_TEST=named");
echo getenv(name: "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func("getenv", "ELEPHC_EVAL_ENV_TEST") . ":";
echo call_user_func_array("putenv", ["assignment" => "ELEPHC_EVAL_ENV_TEST=spread"]) ? "set" : "bad";
echo ":" . getenv("ELEPHC_EVAL_ENV_TEST") . ":";
putenv("ELEPHC_EVAL_ENV_TEST");
echo getenv("ELEPHC_EVAL_ENV_TEST") === "" ? "empty" : "bad";
echo ":"; echo function_exists("getenv");
echo function_exists("putenv");');
"#,
    );
    assert_eq!(out, "direct:named:named:set:spread:empty:11");
}

/// Verifies eval sleep builtins dispatch through direct, named, and callable paths.
#[test]
fn test_eval_dispatches_sleep_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo sleep(0) . ":";
echo sleep(seconds: 0) . ":";
usleep(0);
echo "u:";
echo call_user_func("sleep", 0) . ":";
echo call_user_func_array("usleep", ["microseconds" => 0]) === null ? "null" : "bad";
echo ":"; echo function_exists("sleep");
echo function_exists("usleep");');
"#,
    );
    assert_eq!(out, "0:0:u:0:null:11");
}

/// Verifies eval `php_uname()` dispatches default, named, mode, and callable calls.
#[test]
fn test_eval_dispatches_php_uname_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(php_uname()) > 0 ? "all" : "empty"; echo ":";
echo php_uname() === php_uname("a") ? "same" : "different"; echo ":";
echo strlen(php_uname(mode: "s")) > 0 ? "sys" : "empty"; echo ":";
echo strlen(php_uname("n")) > 0 ? "node" : "empty"; echo ":";
echo strlen(php_uname("r")) > 0 ? "release" : "empty"; echo ":";
echo strlen(php_uname("v")) > 0 ? "version" : "empty"; echo ":";
echo strlen(php_uname("m")) > 0 ? "machine" : "empty"; echo ":";
echo strlen(call_user_func("php_uname", "m")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("php_uname", ["mode" => "n"])) > 0 ? "spread" : "empty"; echo ":";
echo function_exists("php_uname");');
"#,
    );
    assert_eq!(
        out,
        "all:same:sys:node:release:version:machine:call:spread:1"
    );
}

/// Verifies eval `gethostbyname()` handles IPv4 literals and failed lookups.
#[test]
fn test_eval_dispatches_gethostbyname_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo gethostbyname("127.0.0.1") . ":";
echo gethostbyname(hostname: "not a host") . ":";
echo call_user_func("gethostbyname", "127.0.0.1") . ":";
echo call_user_func_array("gethostbyname", ["hostname" => "not a host"]) . ":";
echo function_exists("gethostbyname");');
"#,
    );
    assert_eq!(out, "127.0.0.1:not a host:127.0.0.1:not a host:1");
}

/// Verifies eval `gethostname()` dispatches direct and callable zero-arg calls.
#[test]
fn test_eval_dispatches_gethostname_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(gethostname()) > 0 ? "host" : "empty"; echo ":";
echo strlen(call_user_func("gethostname")) > 0 ? "call" : "empty"; echo ":";
echo strlen(call_user_func_array("gethostname", [])) > 0 ? "spread" : "empty"; echo ":";
echo function_exists("gethostname");');
"#,
    );
    assert_eq!(out, "host:call:spread:1");
}

/// Verifies eval `gethostbyaddr()` handles valid, malformed, and callable calls.
#[test]
fn test_eval_dispatches_gethostbyaddr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strlen(gethostbyaddr("127.0.0.1")) > 0 ? "direct" : "empty"; echo ":";
echo strlen(gethostbyaddr(ip: "127.0.0.1")) > 0 ? "named" : "empty"; echo ":";
echo gethostbyaddr("not-an-ip-address") === false ? "false" : "bad"; echo ":";
echo strlen(call_user_func("gethostbyaddr", "127.0.0.1")) > 0 ? "call" : "empty"; echo ":";
echo call_user_func_array("gethostbyaddr", ["ip" => "not-an-ip-address"]) === false ? "spread" : "bad"; echo ":";
echo function_exists("gethostbyaddr");');
"#,
    );
    assert_eq!(out, "direct:named:false:call:spread:1");
}

/// Verifies eval protocol and service database lookups dispatch dynamically.
#[test]
fn test_eval_dispatches_protocol_service_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo getprotobyname("TCP") . ":";
echo getprotobynumber(6) . ":";
echo getprotobyname("no_such_protocol") === false ? "missing-proto" : "bad"; echo ":";
echo getprotobynumber(999) === false ? "missing-number" : "bad"; echo ":";
echo getservbyname("www", "tcp") . ":";
echo getservbyport(80, "tcp") . ":";
echo getservbyname("no_such_service", "tcp") === false ? "missing-service" : "bad"; echo ":";
echo getservbyport(80, "no_such_proto") === false ? "missing-port" : "bad"; echo ":";
echo call_user_func("getprotobyname", "udp") . ":";
echo call_user_func_array("getprotobynumber", ["protocol" => 17]) . ":";
echo call_user_func("getservbyname", "https", "tcp") . ":";
echo call_user_func_array("getservbyport", ["port" => 443, "protocol" => "tcp"]) . ":";
echo function_exists("getprotobyname"); echo function_exists("getprotobynumber"); echo function_exists("getservbyname"); echo function_exists("getservbyport");');
"#,
    );
    assert_eq!(
        out,
        "6:tcp:missing-proto:missing-number:80:http:missing-service:missing-port:17:udp:443:https:1111"
    );
}

/// Verifies eval stream introspection builtins return native-compatible static lists.
#[test]
fn test_eval_dispatches_stream_introspection_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$wrappers = stream_get_wrappers();
$transports = stream_get_transports();
$filters = stream_get_filters();
echo count($wrappers) . ":" . $wrappers[0] . ":" . $wrappers[5] . ":";
echo count($transports) . ":" . $transports[0] . ":" . $transports[8] . ":";
echo count($filters) . ":" . $filters[2] . ":";
$call_wrappers = call_user_func("stream_get_wrappers");
echo $call_wrappers[10] . ":";
$call_transports = call_user_func_array("stream_get_transports", []);
echo $call_transports[11] . ":";
$call_filters = call_user_func_array("stream_get_filters", []);
echo $call_filters[13] . ":";
$tmp = tmpfile();
echo stream_is_local("php://memory") ? "local" : "bad"; echo ":";
echo stream_supports_lock($tmp) ? "lock" : "bad"; echo ":";
echo call_user_func("stream_is_local", "file://tmp") ? "calllocal" : "bad"; echo ":";
echo call_user_func_array("stream_supports_lock", ["stream" => $tmp]) ? "calllock" : "bad"; echo ":";
echo function_exists("stream_get_wrappers"); echo function_exists("stream_get_transports"); echo function_exists("stream_get_filters");
echo function_exists("stream_is_local"); echo function_exists("stream_supports_lock");');
"#,
    );
    assert_eq!(
        out,
        "11:file:https:12:tcp:tlsv1.0:14:string.rot13:glob:tlsv1.3:bzip2.decompress:local:lock:calllocal:calllock:11111"
    );
}

/// Verifies eval IPv4 conversion builtins handle integer, string, and raw-byte forms.
#[test]
fn test_eval_dispatches_ip_conversion_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo long2ip(3232235777) . ":";
echo long2ip(ip: 4294967295) . ":";
echo ip2long("192.168.1.1") . ":";
echo ip2long(ip: "1.2.3") === false ? "bad-ip" : "bad"; echo ":";
$packed = inet_pton("1.2.3.4");
echo bin2hex($packed) . ":";
echo inet_pton(ip: "nonsense") === false ? "bad-pton" : "bad"; echo ":";
echo inet_ntop($packed) . ":";
echo inet_ntop(ip: "xx") === false ? "bad-ntop" : "bad"; echo ":";
echo call_user_func("long2ip", 2130706433) . ":";
echo call_user_func_array("ip2long", ["ip" => "0.0.0.0"]) . ":";
echo function_exists("long2ip"); echo function_exists("ip2long");
echo function_exists("inet_pton"); echo function_exists("inet_ntop");');
"#,
    );
    assert_eq!(
        out,
        "192.168.1.1:255.255.255.255:3232235777:bad-ip:01020304:bad-pton:1.2.3.4:bad-ntop:127.0.0.1:0:1111"
    );
}

/// Verifies eval `basename()` and `dirname()` preserve static path edge-case behavior.
#[test]
fn test_eval_dispatches_path_component_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo basename("/var/log/syslog.log", ".log") . ":";
echo basename(path: "/usr///") . ":";
echo basename("/", "x") === "" ? "root" : "bad"; echo ":";
echo dirname("/usr/local/bin/tool", 2) . ":";
echo dirname(path: "/usr///local///bin") . ":";
echo call_user_func("basename", "foo.tar.gz", ".bz2") . ":";
echo call_user_func_array("dirname", ["path" => "/usr", "levels" => 3]) . ":";
echo function_exists("basename"); echo function_exists("dirname");');
"#,
    );
    assert_eq!(
        out,
        "syslog:usr:root:/usr/local:/usr///local:foo.tar.gz:/:11"
    );
}

/// Verifies eval `realpath()` returns strings for existing paths and false for missing paths.
#[test]
fn test_eval_dispatches_realpath_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo realpath(".") !== false ? "resolved" : "bad"; echo ":";
echo realpath(path: "elephc-magician-missing-path") === false ? "false" : "bad"; echo ":";
echo call_user_func("realpath", ".") !== false ? "call" : "bad"; echo ":";
echo call_user_func_array("realpath", ["path" => "elephc-magician-missing-path"]) === false ? "array-false" : "bad";
echo ":"; echo function_exists("realpath");');
"#,
    );
    assert_eq!(out, "resolved:false:call:array-false:1");
}

/// Verifies eval `stream_resolve_include_path()` dispatches directly and dynamically.
#[test]
fn test_eval_dispatches_stream_resolve_include_path_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo stream_resolve_include_path("/tmp") !== false ? "resolved" : "bad"; echo ":";
echo stream_resolve_include_path(filename: "elephc-magician-missing-path") === false ? "false" : "bad"; echo ":";
echo call_user_func("stream_resolve_include_path", "/tmp") !== false ? "call" : "bad"; echo ":";
echo call_user_func_array("stream_resolve_include_path", ["filename" => "elephc-magician-missing-path"]) === false ? "array-false" : "bad";
echo ":"; echo function_exists("stream_resolve_include_path");');
"#,
    );
    assert_eq!(out, "resolved:false:call:array-false:1");
}

/// Verifies eval regex builtins handle captures, replacement, callbacks, and splitting.
#[test]
fn test_eval_dispatches_preg_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$ok = preg_match("/([a-z]+)([0-9]+)/", "id42", $matches);
echo $ok . ":" . count($matches) . ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . ":";
echo preg_match("/xyz/", "id42") . ":";
echo preg_match_all("/[0-9]+/", "a1b22c333") . ":";
$allCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $all);
echo $allCount . ":" . count($all) . ":" . $all[0][1] . ":" . $all[1][0] . ":" . $all[2][1] . ":";
$setCount = preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $set, PREG_SET_ORDER);
echo $setCount . ":" . count($set) . ":" . $set[0][0] . ":" . $set[0][1] . ":" . $set[1][2] . ":";
preg_match("/(a)?(b)/", "b", $offsetOne, PREG_OFFSET_CAPTURE);
echo $offsetOne[0][0] . ":" . $offsetOne[0][1] . ":" . $offsetOne[1][0] . ":" . $offsetOne[1][1] . ":" . $offsetOne[2][0] . ":" . $offsetOne[2][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetAll, PREG_OFFSET_CAPTURE);
echo $offsetAll[0][1][0] . ":" . $offsetAll[0][1][1] . ":" . $offsetAll[1][0][1] . ":" . $offsetAll[2][1][1] . ":";
preg_match_all("/([a-z]+)([0-9]+)/", "a1 b22", $offsetSet, PREG_SET_ORDER | PREG_OFFSET_CAPTURE);
echo $offsetSet[1][0][0] . ":" . $offsetSet[1][0][1] . ":" . $offsetSet[0][2][1] . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOne, PREG_UNMATCHED_AS_NULL);
echo count($nullOne) . ":" . ($nullOne[1] === null ? "n" : "bad") . ":" . $nullOne[2] . ":" . ($nullOne[3] === null ? "n" : "bad") . ":";
preg_match("/(a)?(b)(c)?/", "b", $nullOffset, PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullOffset[1][0] === null ? "n" : "bad") . ":" . $nullOffset[1][1] . ":" . ($nullOffset[3][0] === null ? "n" : "bad") . ":" . $nullOffset[3][1] . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullAll, PREG_UNMATCHED_AS_NULL);
echo ($nullAll[1][0] === null ? "n" : "bad") . ":" . $nullAll[2][0] . ":" . ($nullAll[3][0] === null ? "n" : "bad") . ":";
preg_match_all("/(a)?(b)(c)?/", "b", $nullSet, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL | PREG_OFFSET_CAPTURE);
echo ($nullSet[0][1][0] === null ? "n" : "bad") . ":" . $nullSet[0][1][1] . ":" . ($nullSet[0][3][0] === null ? "n" : "bad") . ":" . $nullSet[0][3][1] . ":";
preg_match_all("/(x)(y)/", "abc", $none);
echo count($none) . ":" . count($none[0]) . ":" . count($none[1]) . ":" . count($none[2]) . ":";
echo preg_replace("/([a-z])([0-9])/", "$2-$1", "a1 b2") . ":";
function eval_regex_wrap($matches) { return "[" . $matches[0] . "]"; }
echo preg_replace_callback("/[A-Z]/", "eval_regex_wrap", "AB") . ":";
$limited = preg_split("/,/", "a,b,c", 2);
echo count($limited) . ":" . $limited[0] . ":" . $limited[1] . ":";
$kept = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY);
echo count($kept) . ":" . $kept[1] . ":";
echo call_user_func("preg_match", "/x/", "x") . ":";
$replaced = call_user_func_array("preg_replace", ["pattern" => "/[0-9]+/", "replacement" => "N", "subject" => "a12"]);
echo $replaced . ":";
$captured = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE);
echo count($captured) . ":" . $captured[1] . ":";
$splitOffsets = preg_split("/,/", "a,b,c", 2, PREG_SPLIT_OFFSET_CAPTURE);
echo $splitOffsets[0][0] . ":" . $splitOffsets[0][1] . ":" . $splitOffsets[1][0] . ":" . $splitOffsets[1][1] . ":";
$splitBoth = preg_split("/(,)/", "a,b", 0, PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE);
echo count($splitBoth) . ":" . $splitBoth[1][0] . ":" . $splitBoth[1][1] . ":";
$splitNoEmpty = preg_split("/,/", "a,,b", 0, PREG_SPLIT_NO_EMPTY | PREG_SPLIT_OFFSET_CAPTURE);
echo $splitNoEmpty[1][0] . ":" . $splitNoEmpty[1][1] . ":";
class EvalPregMatchesBox {
    public array $matches = [];
    public static array $staticMatches = [];
}
$box = new EvalPregMatchesBox();
preg_match("/([a-z]+)([0-9]+)/", "ab12", $box->matches);
echo $box->matches[0] . ":" . $box->matches[1] . ":" . $box->matches[2] . ":";
$name = "matches";
preg_match_all("/([a-z])([0-9])/", "a1 b2", $box->{$name}, PREG_SET_ORDER);
echo count($box->matches) . ":" . $box->matches[1][0] . ":" . $box->matches[1][2] . ":";
preg_match("/([A-Z]+)/", "ID", EvalPregMatchesBox::$staticMatches);
echo EvalPregMatchesBox::$staticMatches[0] . ":";
$class = "EvalPregMatchesBox";
$staticName = "staticMatches";
preg_match_all("/([a-z])/", "xy", $class::${$staticName});
echo count(EvalPregMatchesBox::$staticMatches[0]) . ":" . EvalPregMatchesBox::$staticMatches[0][1] . ":";
echo function_exists("preg_match") && function_exists("preg_match_all") && function_exists("preg_replace") && function_exists("preg_replace_callback") && function_exists("preg_split") && defined("PREG_SPLIT_NO_EMPTY") && defined("PREG_SET_ORDER") && defined("PREG_OFFSET_CAPTURE") && defined("PREG_SPLIT_OFFSET_CAPTURE") && defined("PREG_UNMATCHED_AS_NULL");');
"#,
    );
    assert_eq!(
        out,
        "1:3:id42:id:42:0:3:2:3:b22:a:22:2:2:a1:a:22:b:0::-1:b:0:b22:3:0:4:b22:3:1:4:n:b:n:n:-1:n:-1:n:b:n:n:-1:n:-1:3:0:0:0:1-a 2-b:[A][B]:2:a:b,c:2:b:1:aN:3:,:a:0:b,c:2:3:,:1:b:3:ab12:ab:12:2:b2:2:ID:2:y:1"
    );
}

/// Verifies eval `preg_replace_callback()` accepts general callable forms.
#[test]
fn test_eval_preg_replace_callback_accepts_general_callables() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalPregCallbackBox {
    public $prefix = "";
    public function __construct($prefix) { $this->prefix = $prefix; }
    public function wrap($matches) { return $this->prefix . $matches[0]; }
    public static function wrapStatic($matches) { return "S" . $matches[0]; }
}
$box = new EvalPregCallbackBox("O");
echo preg_replace_callback("/[A-Z]/", [$box, "wrap"], "AB") . ":";
echo preg_replace_callback("/[0-9]/", "EvalPregCallbackBox::wrapStatic", "12") . ":";
echo preg_replace_callback("/[C]/", ["EvalPregCallbackBox", "wrapStatic"], "CC") . ":";
$first = $box->wrap(...);
echo preg_replace_callback("/[a-z]/", $first, "xy") . ":";
$static = EvalPregCallbackBox::wrapStatic(...);
return preg_replace_callback("/[m]/", $static, "mm");');
"#,
    );
    assert_eq!(out, "OAOB:S1S2:SCSC:OxOy:SmSm");
}

/// Verifies dynamic eval preg callables write by-reference `$matches` arrays.
#[test]
fn test_eval_dynamic_preg_callables_write_matches_by_ref() {
    let out = compile_and_run(
        r#"<?php
eval('$match = "preg_match";
$ok = $match("/([a-z]+)([0-9]+)/", "id42", $matches);
echo $ok . ":" . $matches[0] . ":" . $matches[1] . ":" . $matches[2] . ":";
$matchAll = "preg_match_all";
$count = $matchAll("/([a-z])([0-9])/", "a1 b2", $all, PREG_SET_ORDER);
echo $count . ":" . $all[1][0] . ":" . $all[1][2] . ":";
$firstClass = preg_match(...);
$okAgain = $firstClass("/([A-Z]+)/", "ID", $firstClassMatches);
echo $okAgain . ":" . $firstClassMatches[0];');
"#,
    );
    assert_eq!(out, "1:id42:id:42:2:b2:2:1:ID");
}

/// Verifies named eval preg calls write by-reference `$matches` arrays.
#[test]
fn test_eval_named_preg_calls_write_matches_by_ref() {
    let out = compile_and_run(
        r#"<?php
eval('$named = [];
$ok = preg_match(pattern: "/([a-z]+)([0-9]+)/", subject: "id42", matches: $named);
echo $ok . ":" . $named[0] . ":" . $named[1] . ":" . $named[2] . ":";
$all = [];
$count = preg_match_all(pattern: "/([a-z])([0-9])/", subject: "a1 b2", matches: $all, flags: PREG_SET_ORDER);
echo $count . ":" . $all[1][0] . ":" . $all[1][2] . ":";
echo preg_match(pattern: "/x/", subject: "x", flags: PREG_OFFSET_CAPTURE);');
"#,
    );
    assert_eq!(out, "1:id42:id:42:2:b2:2:1");
}

/// Verifies eval `call_user_func*()` warns for by-value regex `$matches` outputs.
#[test]
fn test_eval_call_user_func_regex_ref_like_builtin_args_warn_and_use_value_copy() {
    let out = compile_and_run_capture(
        r#"<?php
eval('$matches = ["old"];
echo call_user_func("preg_match", "/x/", "x", $matches) . ":" . $matches[0] . "|";
$all = ["old"];
echo call_user_func("preg_match_all", "/x/", "xx", $all) . ":" . $all[0] . "|";
$named = ["old"];
echo call_user_func_array("preg_match", ["pattern" => "/y/", "subject" => "y", "matches" => $named]) . ":" . $named[0] . "|";
$flagged = ["old"];
echo call_user_func("preg_match_all", "/([a-z])/", "ab", $flagged, PREG_SET_ORDER) . ":" . $flagged[0];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1:old|2:old|1:old|2:old");
    for warning in [
        "preg_match(): Argument #3 ($matches) must be passed by reference, value given",
        "preg_match_all(): Argument #3 ($matches) must be passed by reference, value given",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing by-ref warning {warning:?}: {}",
            out.stderr
        );
    }
}

/// Verifies eval `fnmatch()` supports wildcards, classes, flags, constants, and callables.
#[test]
fn test_eval_dispatches_fnmatch_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo fnmatch("*.log", "system.log") ? "match" : "bad"; echo ":";
echo fnmatch("*.log", "logs/system.log", FNM_PATHNAME) ? "bad" : "path"; echo ":";
echo fnmatch("*.LOG", "system.log", FNM_CASEFOLD) ? "case" : "bad"; echo ":";
echo fnmatch("*", ".env", FNM_PERIOD) ? "bad" : "period"; echo ":";
echo fnmatch("[!abc]oo", "doo") && !fnmatch("[!abc]oo", "boo") ? "class" : "bad"; echo ":";
echo fnmatch("a\\\\*b", "a*b") ? "escape" : "bad"; echo ":";
echo fnmatch("a\\\\*b", "a\\\\xxb", FNM_NOESCAPE) ? "noescape" : "bad"; echo ":";
$flags = FNM_PATHNAME | FNM_CASEFOLD;
echo fnmatch("dir/*.TXT", "dir/file.txt", $flags) ? "flags" : "bad"; echo ":";
echo call_user_func("fnmatch", "*.txt", "report.txt") ? "call" : "bad"; echo ":";
echo call_user_func_array("fnmatch", ["pattern" => "*.TXT", "filename" => "report.txt", "flags" => FNM_CASEFOLD]) ? "callarray" : "bad"; echo ":";
echo function_exists("fnmatch"); echo defined("FNM_CASEFOLD");');
"#,
    );
    assert_eq!(
        out,
        "match:path:case:period:class:escape:noescape:flags:call:callarray:11"
    );
}

/// Verifies eval `basename()` and `dirname()` support defaults, named call arrays, and function probes.
#[test]
fn test_eval_dispatches_basename_dirname_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo basename("/var/log/syslog.log", ".log") . ":";
echo dirname("/usr/local/bin/tool", 2) . ":";
echo call_user_func("basename", "/tmp/file.txt") . ":";
echo call_user_func_array("dirname", ["path" => "/a/b/c", "levels" => 2]) . ":";
echo function_exists("basename") && function_exists("dirname");');
"#,
    );
    assert_eq!(out, "syslog:/usr/local:file.txt:/a:1");
}

/// Verifies eval `pathinfo()` supports arrays, component flags, constants, and callables.
#[test]
fn test_eval_dispatches_pathinfo_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$info = pathinfo("/var/log/syslog.log");
echo $info["dirname"] . "|" . $info["basename"] . "|" . $info["extension"] . "|" . $info["filename"] . ":";
echo pathinfo("archive.tar.gz", PATHINFO_EXTENSION) . ":";
echo pathinfo(".bashrc", PATHINFO_FILENAME) === "" ? "dotfile" : "bad"; echo ":";
echo pathinfo("file.", PATHINFO_EXTENSION) === "" ? "trail" : "bad"; echo ":";
echo pathinfo("", PATHINFO_DIRNAME) === "" ? "empty-dir" : "bad"; echo ":";
$plain = pathinfo("/etc/hosts");
echo array_key_exists("extension", $plain) ? "bad" : "no-ext"; echo ":";
echo pathinfo("/a/b.php", PATHINFO_BASENAME | PATHINFO_FILENAME) . ":";
$call = call_user_func("pathinfo", "foo.txt", PATHINFO_ALL);
echo $call["basename"] . ":";
echo call_user_func_array("pathinfo", ["path" => "foo.txt", "flags" => 0]) === "" ? "zero" : "bad";
echo ":"; echo function_exists("pathinfo"); echo defined("PATHINFO_ALL");');
"#,
    );
    assert_eq!(
        out,
        "/var/log|syslog.log|log|syslog:gz:dotfile:trail:empty-dir:no-ext:b.php:foo.txt:zero:11"
    );
}

/// Verifies eval local filesystem builtins read, write, stat, delete, and dispatch.
#[test]
fn test_eval_dispatches_filesystem_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo file_put_contents("eval-fs.txt", "hello") . ":";
echo file_get_contents("eval-fs.txt") . ":";
echo file_exists("eval-fs.txt") ? "exists" : "missing"; echo ":";
echo is_file(filename: "eval-fs.txt") ? "file" : "bad"; echo ":";
echo is_dir(".") ? "dir" : "bad"; echo ":";
echo is_readable("eval-fs.txt") ? "readable" : "bad"; echo ":";
echo is_writable("eval-fs.txt") ? "writable" : "bad"; echo ":";
echo is_writeable("eval-fs.txt") ? "writeable" : "bad"; echo ":";
echo filesize("eval-fs.txt") . ":";
echo call_user_func("file_exists", "eval-fs.txt") ? "call-exists" : "bad"; echo ":";
echo call_user_func_array("filesize", ["filename" => "eval-fs.txt"]) . ":";
echo unlink("eval-fs.txt") ? "unlinked" : "bad"; echo ":";
echo file_exists("eval-fs.txt") ? "bad" : "gone"; echo ":";
echo function_exists("file_get_contents"); echo function_exists("file_put_contents");
echo function_exists("file_exists"); echo function_exists("is_file"); echo function_exists("is_dir");
echo function_exists("is_readable"); echo function_exists("is_writable"); echo function_exists("is_writeable");
echo function_exists("filesize"); echo function_exists("unlink");');
"#,
    );
    assert_eq!(
        out,
        "5:hello:exists:file:dir:readable:writable:writeable:5:call-exists:5:unlinked:gone:1111111111"
    );
}

/// Verifies dynamic eval `flock()` callables write the by-reference `$would_block` output.
#[test]
fn test_eval_dynamic_flock_callables_write_would_block_by_ref() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-lock.txt", "x");
$h = fopen("eval-lock.txt", "r+");
$lock = "flock";
$would = true;
echo $lock($h, LOCK_EX, $would) ? "dynlock" : "bad"; echo ":";
echo $would === false ? "dyn0" : "bad"; echo ":";
flock($h, LOCK_UN);
$firstClass = flock(...);
$firstClassWould = true;
echo $firstClass($h, LOCK_SH, $firstClassWould) ? "fcclock" : "bad"; echo ":";
echo $firstClassWould === false ? "fcc0" : "bad"; echo ":";
flock($h, LOCK_UN);
fclose($h);
echo unlink("eval-lock.txt") ? "cleanup" : "bad";');
"#,
    );
    assert_eq!(out, "dynlock:dyn0:fcclock:fcc0:cleanup");
}

/// Verifies eval `call_user_func()` warns for by-value filesystem ref-like outputs.
#[test]
fn test_eval_call_user_func_filesystem_ref_like_builtin_args_warn_and_use_value_copy() {
    let out = compile_and_run_capture(
        r#"<?php
eval('file_put_contents("eval-cuf-lock.txt", "x");
$h = fopen("eval-cuf-lock.txt", "r+");
$would = "old";
echo call_user_func("flock", $h, LOCK_SH, $would) ? "lock" : "bad";
echo ":" . $would . ":";
flock($h, LOCK_UN);
fclose($h);
unlink("eval-cuf-lock.txt");
$pair = stream_socket_pair(1, 1, 0);
$read = [$pair[0]];
$write = [];
$except = [];
echo call_user_func("stream_select", $read, $write, $except, 0) . ":";
echo count($read) . ":" . count($write) . ":" . count($except);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "lock:old:0:1:0:0");
    for warning in [
        "flock(): Argument #3 ($would_block) must be passed by reference, value given",
        "stream_select(): Argument #1 ($read) must be passed by reference, value given",
        "stream_select(): Argument #2 ($write) must be passed by reference, value given",
        "stream_select(): Argument #3 ($except) must be passed by reference, value given",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing by-ref warning {warning:?}: {}",
            out.stderr
        );
    }
}

/// Verifies dynamic eval stream-socket callables write by-reference output parameters.
#[test]
fn test_eval_dynamic_stream_socket_callables_write_ref_outputs() {
    let out = compile_and_run(
        r#"<?php
eval('$server = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($server, false);
$accept = "stream_socket_accept";
$client = stream_socket_client("tcp://" . $addr);
$peerName = "";
$peer = $accept($server, null, $peerName);
echo is_resource($peer) && $peerName !== "" ? "dynaccept" : "bad"; echo ":";
stream_socket_sendto($client, "ping");
$recv = stream_socket_recvfrom(...);
$remoteAddr = "";
echo $recv($peer, 4, 0, $remoteAddr) === "ping" && $remoteAddr !== "" ? "dynrecv" : "bad"; echo ":";
fclose($client); fclose($peer); fclose($server);
$server = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($server, false);
$client = stream_socket_client("tcp://" . $addr);
$directPeerName = "";
$peer = stream_socket_accept(socket: $server, peer_name: $directPeerName);
echo is_resource($peer) && $directPeerName !== "" ? "namedaccept" : "bad"; echo ":";
stream_socket_sendto($client, "pong");
$directAddr = "";
echo stream_socket_recvfrom(socket: $peer, length: 4, address: $directAddr) === "pong" && $directAddr !== "" ? "namedrecv" : "bad";
fclose($client); fclose($peer); fclose($server);');
"#,
    );
    assert_eq!(out, "dynaccept:dynrecv:namedaccept:namedrecv");
}

/// Verifies eval fsockopen and stream_select callables write by-reference outputs.
#[test]
fn test_eval_dynamic_fsockopen_and_stream_select_write_ref_outputs() {
    let out = compile_and_run(
        r#"<?php
eval('$server = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($server, false);
$parts = explode(":", $addr);
$open = "fsockopen";
$errno = 9; $errstr = "x";
$client = $open("127.0.0.1", intval($parts[1]), $errno, $errstr);
$peer = stream_socket_accept($server);
echo is_resource($client) && is_resource($peer) && $errno === 0 && $errstr === "" ? "dynfsock" : "bad"; echo ":";
fclose($client); fclose($peer); fclose($server);
$server = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($server, false);
$parts = explode(":", $addr);
$namedErrno = 9; $namedErrstr = "x";
$client = fsockopen(hostname: "127.0.0.1", port: intval($parts[1]), error_code: $namedErrno, error_message: $namedErrstr);
$peer = stream_socket_accept($server);
echo is_resource($client) && is_resource($peer) && $namedErrno === 0 && $namedErrstr === "" ? "namedfsock" : "bad"; echo ":";
fclose($client); fclose($peer); fclose($server);
$pair = stream_socket_pair(1, 1, 0);
$read = [$pair[1]]; $write = [$pair[0]]; $except = [$pair[0]];
$select = stream_select(...);
echo $select($read, $write, $except, 0) === 0 && count($read) === 0 && count($write) === 0 && count($except) === 0 ? "fccselect" : "bad"; echo ":";
$read = [$pair[1]]; $write = []; $except = [];
echo stream_select(read: $read, write: $write, except: $except, seconds: 0) === 0 && count($read) === 0 ? "namedselect" : "bad";
fclose($pair[0]); fclose($pair[1]);');
"#,
    );
    assert_eq!(out, "dynfsock:namedfsock:fccselect:namedselect");
}

/// Verifies eval disk-space builtins return positive local capacity and zero on failure.
#[test]
fn test_eval_dispatches_disk_space_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo disk_free_space(".") > 0 ? "free" : "bad"; echo ":";
echo disk_total_space(directory: ".") > 0 ? "total" : "bad"; echo ":";
echo disk_total_space(".") >= disk_free_space(".") ? "ordered" : "bad"; echo ":";
echo disk_free_space("no/such/path/elephc-magician") === 0.0 ? "missing" : "bad"; echo ":";
echo call_user_func("disk_free_space", ".") > 0 ? "call" : "bad"; echo ":";
echo call_user_func_array("disk_total_space", ["directory" => "."]) > 0 ? "spread" : "bad";
echo ":"; echo function_exists("disk_free_space"); echo function_exists("disk_total_space");');
"#,
    );
    assert_eq!(out, "free:total:ordered:missing:call:spread:11");
}

/// Verifies eval stat metadata builtins return scalar metadata and dispatch dynamically.
#[test]
fn test_eval_dispatches_stat_metadata_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-stat.txt", "hello");
echo filemtime("eval-stat.txt") > 0 ? "mtime" : "bad"; echo ":";
echo fileatime(filename: "eval-stat.txt") > 0 ? "atime" : "bad"; echo ":";
echo filectime("eval-stat.txt") > 0 ? "ctime" : "bad"; echo ":";
echo fileperms("eval-stat.txt") > 0 ? "perms" : "bad"; echo ":";
echo fileowner("eval-stat.txt") >= 0 ? "owner" : "bad"; echo ":";
echo filegroup("eval-stat.txt") >= 0 ? "group" : "bad"; echo ":";
echo fileinode("eval-stat.txt") > 0 ? "inode" : "bad"; echo ":";
echo filetype("eval-stat.txt") . ":";
echo filetype(".") . ":";
echo is_executable("/bin/sh") ? "exec" : "bad"; echo ":";
echo is_link("eval-stat.txt") ? "bad" : "notlink"; echo ":";
echo fileatime("missing-stat.txt") === false ? "missing-atime" : "bad"; echo ":";
echo filetype("missing-stat.txt") === false ? "missing-type" : "bad"; echo ":";
echo filemtime("missing-stat.txt") === 0 ? "missing-mtime" : "bad"; echo ":";
echo call_user_func("filetype", "eval-stat.txt") . ":";
echo call_user_func_array("fileinode", ["filename" => "eval-stat.txt"]) > 0 ? "callinode" : "bad"; echo ":";
echo function_exists("filemtime"); echo function_exists("fileatime");
echo function_exists("filectime"); echo function_exists("fileperms");
echo function_exists("fileowner"); echo function_exists("filegroup");
echo function_exists("fileinode"); echo function_exists("filetype");
echo function_exists("is_executable"); echo function_exists("is_link");
unlink("eval-stat.txt");');
"#,
    );
    assert_eq!(
        out,
        "mtime:atime:ctime:perms:owner:group:inode:file:dir:exec:notlink:missing-atime:missing-type:missing-mtime:file:callinode:1111111111"
    );
}

/// Verifies eval `stat()` and `lstat()` build PHP-compatible metadata arrays.
#[test]
fn test_eval_dispatches_stat_array_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-stat-array.txt", "hello");
symlink("eval-stat-array.txt", "eval-lstat-array.txt");
$stat = stat("eval-stat-array.txt");
$lstat = lstat("eval-lstat-array.txt");
echo $stat["size"] === 5 && $stat[7] === $stat["size"] ? "stat" : "bad"; echo ":";
echo ($stat["mode"] & 61440) === 32768 ? "mode" : "bad"; echo ":";
echo ($lstat["mode"] & 61440) === 40960 ? "lstat" : "bad"; echo ":";
echo stat("eval-stat-array-missing.txt") === false && lstat("eval-stat-array-missing.txt") === false ? "missing" : "bad"; echo ":";
$call = call_user_func("stat", "eval-stat-array.txt");
echo $call["mtime"] === filemtime("eval-stat-array.txt") ? "callstat" : "bad"; echo ":";
$call_lstat = call_user_func_array("lstat", ["filename" => "eval-lstat-array.txt"]);
echo $call_lstat["ino"] > 0 ? "calllstat" : "bad"; echo ":";
echo unlink("eval-lstat-array.txt") && unlink("eval-stat-array.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("stat"); echo function_exists("lstat");
');
"#,
    );
    assert_eq!(out, "stat:mode:lstat:missing:callstat:calllstat:cleanup:11");
}

/// Verifies eval path operation builtins mutate local filesystem state.
#[test]
fn test_eval_dispatches_path_operation_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-op-src.txt", "hello");
echo mkdir("eval-op-dir") ? "mkdir" : "bad"; echo ":";
echo copy("eval-op-src.txt", "eval-op-copy.txt") ? "copy" : "bad"; echo ":";
echo rename("eval-op-copy.txt", "eval-op-moved.txt") && file_exists("eval-op-moved.txt") ? "rename" : "bad"; echo ":";
echo symlink("eval-op-src.txt", "eval-op-link.txt") ? "symlink" : "bad"; echo ":";
echo readlink("eval-op-link.txt") === "eval-op-src.txt" ? "readlink" : "bad"; echo ":";
echo linkinfo("eval-op-link.txt") >= 0 ? "linkinfo" : "bad"; echo ":";
echo link("eval-op-src.txt", "eval-op-hard.txt") ? "hardlink" : "bad"; echo ":";
echo readlink("eval-op-src.txt") === false ? "readlink-false" : "bad"; echo ":";
echo linkinfo("eval-op-missing.txt") === -1 ? "linkinfo-missing" : "bad"; echo ":";
echo chdir("eval-op-dir") ? "chdir" : "bad"; echo ":";
echo getcwd() !== "" ? "cwd" : "bad"; echo ":";
chdir("..");
echo clearstatcache(true, "eval-op-src.txt") === null ? "cache" : "bad"; echo ":";
echo unlink("eval-op-link.txt") && unlink("eval-op-hard.txt") && unlink("eval-op-moved.txt") && unlink("eval-op-src.txt") && rmdir("eval-op-dir") ? "cleanup" : "bad"; echo ":";
echo call_user_func("mkdir", "eval-op-call-dir") ? "callmkdir" : "bad"; echo ":";
echo call_user_func_array("rmdir", ["directory" => "eval-op-call-dir"]) ? "callrmdir" : "bad"; echo ":";
echo function_exists("mkdir"); echo function_exists("rmdir"); echo function_exists("copy");
echo function_exists("rename"); echo function_exists("symlink"); echo function_exists("link");
echo function_exists("readlink"); echo function_exists("linkinfo"); echo function_exists("clearstatcache");
');
"#,
    );
    assert_eq!(
        out,
        "mkdir:copy:rename:symlink:readlink:linkinfo:hardlink:readlink-false:linkinfo-missing:chdir:cwd:cache:cleanup:callmkdir:callrmdir:111111111"
    );
}

/// Verifies eval file-listing builtins build arrays, stream files, and dispatch dynamically.
#[test]
fn test_eval_dispatches_file_listing_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-lines.txt", "one\ntwo");
file_put_contents("eval-empty.txt", "");
$lines = file("eval-lines.txt");
echo count($lines) . ":";
echo $lines[0] === "one\n" ? "line0" : "bad"; echo ":";
echo $lines[1] === "two" ? "line1" : "bad"; echo ":";
echo "[";
$bytes = readfile(filename: "eval-empty.txt");
echo "]" . $bytes . ":";
echo readfile("eval-missing.txt") === false ? "missing-readfile" : "bad"; echo ":";
mkdir("eval-list-dir");
file_put_contents("eval-list-dir/a.txt", "a");
file_put_contents("eval-list-dir/b.txt", "b");
$scan = scandir(directory: "eval-list-dir");
echo count($scan) . ":";
echo in_array(".", $scan) && in_array("..", $scan) && in_array("a.txt", $scan) && in_array("b.txt", $scan) ? "scan" : "bad"; echo ":";
$call_lines = call_user_func("file", "eval-lines.txt");
echo $call_lines[0] === "one\n" ? "callfile" : "bad"; echo ":";
$call_scan = call_user_func_array("scandir", ["directory" => "eval-list-dir"]);
echo count($call_scan) . ":";
echo unlink("eval-list-dir/a.txt") && unlink("eval-list-dir/b.txt") && rmdir("eval-list-dir") && unlink("eval-lines.txt") && unlink("eval-empty.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("file"); echo function_exists("readfile"); echo function_exists("scandir");
');
"#,
    );
    assert_eq!(
        out,
        "2:line0:line1:[]0:missing-readfile:4:scan:callfile:4:cleanup:111"
    );
}

/// Verifies eval directory resource builtins dispatch directly and dynamically.
#[test]
fn test_eval_dispatches_directory_resource_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('mkdir("eval-dir-handle");
file_put_contents("eval-dir-handle/a.txt", "a");
$dh = opendir(directory: "eval-dir-handle");
$first = readdir(dir_handle: $dh);
$second = readdir($dh);
$third = readdir($dh);
$end = readdir($dh);
$found = ($first === "a.txt" || $second === "a.txt" || $third === "a.txt") ? 1 : 0;
rewinddir($dh);
$again = readdir($dh);
closedir($dh);
echo is_string($first) && is_string($second) && is_string($third) && $end === false && $found === 1 && $again === $first ? "iter" : "bad"; echo ":";
$call = call_user_func("opendir", "eval-dir-handle");
$call_first = call_user_func("readdir", $call);
echo is_string($call_first) ? "callread" : "bad"; echo ":";
echo call_user_func("rewinddir", $call) === null ? "callrewind" : "bad"; echo ":";
echo call_user_func_array("closedir", ["dir_handle" => $call]) === null ? "callclose" : "bad"; echo ":";
echo unlink("eval-dir-handle/a.txt") && rmdir("eval-dir-handle") ? "cleanup" : "bad"; echo ":";
echo function_exists("opendir"); echo function_exists("readdir"); echo function_exists("rewinddir"); echo function_exists("closedir");
');
"#,
    );
    assert_eq!(out, "iter:callread:callrewind:callclose:cleanup:1111");
}

/// Verifies eval `glob()` expands local patterns and dispatches dynamically.
#[test]
fn test_eval_dispatches_glob_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('mkdir("eval-glob-dir");
file_put_contents("eval-glob-dir/a.txt", "a");
file_put_contents("eval-glob-dir/b.log", "b");
file_put_contents("eval-glob-dir/c.txt", "c");
file_put_contents("eval-glob-dir/.hidden.txt", "h");
$matches = glob("eval-glob-dir/*.txt");
echo count($matches) === 2 && basename($matches[0]) === "a.txt" && basename($matches[1]) === "c.txt" ? "glob" : "bad"; echo ":";
echo count(glob("eval-glob-dir/*.none")) === 0 ? "empty" : "bad"; echo ":";
$literal = glob("eval-glob-dir/a.txt");
echo count($literal) === 1 && $literal[0] === "eval-glob-dir/a.txt" ? "literal" : "bad"; echo ":";
$all = glob("eval-glob-dir/*");
echo in_array("eval-glob-dir/.hidden.txt", $all) ? "bad" : "hidden"; echo ":";
$call = call_user_func("glob", "eval-glob-dir/*.log");
echo count($call) === 1 && basename($call[0]) === "b.log" ? "callglob" : "bad"; echo ":";
$call_array = call_user_func_array("glob", ["pattern" => "eval-glob-dir/*.txt"]);
echo count($call_array) === 2 ? "callarray" : "bad"; echo ":";
unlink("eval-glob-dir/.hidden.txt");
unlink("eval-glob-dir/c.txt");
unlink("eval-glob-dir/b.log");
unlink("eval-glob-dir/a.txt");
echo rmdir("eval-glob-dir") ? "cleanup" : "bad"; echo ":";
echo function_exists("glob");
');
"#,
    );
    assert_eq!(
        out,
        "glob:empty:literal:hidden:callglob:callarray:cleanup:1"
    );
}

/// Verifies eval file-modification builtins update modes, masks, temp files, and dispatch.
#[test]
fn test_eval_dispatches_file_modify_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('file_put_contents("eval-mod.txt", "x");
echo chmod(filename: "eval-mod.txt", permissions: 384) ? "chmod" : "bad"; echo ":";
echo (fileperms("eval-mod.txt") & 511) === 384 ? "mode" : "bad"; echo ":";
echo chmod("eval-missing-mod.txt", 384) ? "bad" : "chmod-false"; echo ":";
echo chown("eval-missing-owner.txt", 1000) ? "bad" : "chown-false"; echo ":";
echo chgrp("eval-missing-group.txt", 1000) ? "bad" : "chgrp-false"; echo ":";
echo lchown("eval-missing-link.txt", 1000) ? "bad" : "lchown-false"; echo ":";
echo lchgrp("eval-missing-link.txt", 1000) ? "bad" : "lchgrp-false"; echo ":";
$tmp = tempnam(directory: ".", prefix: "evm");
echo file_exists($tmp) && str_starts_with(basename($tmp), "evm") ? "tempnam" : "bad"; echo ":";
unlink($tmp);
$previous = umask(mask: 18);
$set = umask($previous);
echo $set === 18 ? "umask" : "bad"; echo ":";
$before = umask(18);
$probe = umask();
$restore = umask($before);
echo $probe === 18 && $restore === 18 ? "probe" : "bad"; echo ":";
echo call_user_func("chmod", "eval-mod.txt", 420) ? "callchmod" : "bad"; echo ":";
echo call_user_func("chown", "eval-missing-call-owner.txt", 1000) ? "bad" : "callchown-false"; echo ":";
echo call_user_func_array("chgrp", ["filename" => "eval-missing-call-group.txt", "group" => 1000]) ? "bad" : "callchgrp-false"; echo ":";
$call_tmp = call_user_func_array("tempnam", ["directory" => ".", "prefix" => "evc"]);
echo file_exists($call_tmp) && str_starts_with(basename($call_tmp), "evc") ? "calltempnam" : "bad"; echo ":";
unlink($call_tmp);
echo unlink("eval-mod.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("chmod"); echo function_exists("chown"); echo function_exists("chgrp");
echo function_exists("lchown"); echo function_exists("lchgrp"); echo function_exists("tempnam");
echo function_exists("umask");
');
"#,
    );
    assert_eq!(
        out,
        "chmod:mode:chmod-false:chown-false:chgrp-false:lchown-false:lchgrp-false:tempnam:umask:probe:callchmod:callchown-false:callchgrp-false:calltempnam:cleanup:1111111"
    );
}

/// Verifies eval `touch()` creates files, stamps mtimes, and dispatches dynamically.
#[test]
fn test_eval_dispatches_touch_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo touch(filename: "eval-touch-created.txt") && file_exists("eval-touch-created.txt") ? "create" : "bad"; echo ":";
file_put_contents("eval-touch-stamped.txt", "x");
echo touch("eval-touch-stamped.txt", 1000000000) ? "mtime" : "bad"; echo ":";
echo filemtime("eval-touch-stamped.txt") === 1000000000 ? "readmtime" : "bad"; echo ":";
echo touch("eval-touch-stamped.txt", 1000000001, null) && filemtime("eval-touch-stamped.txt") === 1000000001 ? "nullatime" : "bad"; echo ":";
echo touch("eval-touch-stamped.txt", 1000000002, 1000000003) && filemtime("eval-touch-stamped.txt") === 1000000002 ? "both" : "bad"; echo ":";
echo touch("eval-touch-missing/x.txt") ? "bad" : "touch-false"; echo ":";
echo call_user_func("touch", "eval-touch-created.txt", 1000000004) ? "calltouch" : "bad"; echo ":";
echo call_user_func_array("touch", ["filename" => "eval-touch-stamped.txt", "mtime" => 1000000005]) ? "callarray" : "bad"; echo ":";
echo unlink("eval-touch-created.txt") && unlink("eval-touch-stamped.txt") ? "cleanup" : "bad"; echo ":";
echo function_exists("touch");
');
"#,
    );
    assert_eq!(
        out,
        "create:mtime:readmtime:nullatime:both:touch-false:calltouch:callarray:cleanup:1"
    );
}

/// Verifies eval process-pipe and temporary stream builtins dispatch dynamically.
#[test]
fn test_eval_dispatches_process_pipe_and_tmpfile_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$tmp = tmpfile();
echo gettype($tmp) === "resource" ? "tmpfile" : "bad"; echo ":";
echo fwrite($tmp, "abc") . ":";
rewind($tmp);
echo fread($tmp, 3) . ":";
$pipe = popen("printf xyz", "r");
echo fread($pipe, 3) . ":";
echo pclose($pipe) . ":";
echo call_user_func("tmpfile") !== false ? "calltmp" : "bad"; echo ":";
$callPipe = call_user_func_array("popen", ["command" => "printf q", "mode" => "r"]);
echo fread($callPipe, 1) . ":";
echo call_user_func("pclose", $callPipe) . ":";
echo function_exists("tmpfile"); echo function_exists("popen"); echo function_exists("pclose");');
"#,
    );
    assert_eq!(out, "tmpfile:3:abc:xyz:0:calltmp:q:0:111");
}

/// Verifies eval `bin2hex()` converts byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_bin2hex_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo bin2hex("Az"); echo ":";
echo bin2hex(string: "A\n"); echo ":";
echo bin2hex(\'\n\'); echo ":";
echo bin2hex("A\q"); echo ":";
echo bin2hex("A\v\e\f"); echo ":";
echo call_user_func("bin2hex", "!?"); echo ":";
echo call_user_func_array("bin2hex", ["string" => "ok"]);
echo ":"; echo function_exists("bin2hex");');
"#,
    );
    assert_eq!(out, "417a:410a:5c6e:415c71:410b1b0c:213f:6f6b:1");
}

/// Verifies eval `hex2bin()` decodes hex strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_hex2bin_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo hex2bin("417a"); echo ":";
echo bin2hex(hex2bin(string: "410a")); echo ":";
echo call_user_func("hex2bin", "213f"); echo ":";
echo call_user_func_array("hex2bin", ["string" => "6f6b"]);
echo ":"; echo function_exists("hex2bin");');
"#,
    );
    assert_eq!(out, "Az:410a:!?:ok:1");
}

/// Verifies eval `addslashes()` and `stripslashes()` use PHP byte escaping semantics.
#[test]
fn test_eval_dispatches_slash_escape_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$escaped = addslashes("a\"b");
echo bin2hex($escaped); echo ":";
echo bin2hex(stripslashes($escaped)); echo ":";
echo call_user_func("addslashes", "x\"y"); echo ":";
echo call_user_func_array("stripslashes", [addslashes("o\"k")]);
echo ":"; echo function_exists("addslashes") && function_exists("stripslashes");');
"#,
    );
    assert_eq!(out, "615c2262:612262:x\\\"y:o\"k:1");
}

/// Verifies eval `base64_encode()` encodes byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_base64_encode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo base64_encode("Hello"); echo ":";
echo base64_encode(string: "Hi"); echo ":";
echo call_user_func("base64_encode", "Test 123!"); echo ":";
echo call_user_func_array("base64_encode", ["string" => ""]);
echo ":"; echo function_exists("base64_encode");');
"#,
    );
    assert_eq!(out, "SGVsbG8=:SGk=:VGVzdCAxMjMh::1");
}

/// Verifies eval `base64_decode()` decodes byte strings directly and by callable dispatch.
#[test]
fn test_eval_dispatches_base64_decode_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo base64_decode("SGVsbG8="); echo ":";
echo base64_decode(string: "SGk="); echo ":";
echo call_user_func("base64_decode", "VGVzdCAxMjMh"); echo ":";
echo call_user_func_array("base64_decode", ["string" => ""]);
echo ":"; echo function_exists("base64_decode");');
"#,
    );
    assert_eq!(out, "Hello:Hi:Test 123!::1");
}

/// Verifies eval `str_contains()` supports direct and callable byte-string search.
#[test]
fn test_eval_dispatches_str_contains_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_contains("Hello World", "World") ? "Y" : "N";
echo str_contains("Hello", "z") ? "bad" : ":N";
echo str_contains("Hello", "") ? ":E" : "bad";
echo call_user_func("str_contains", "abc", "b") ? ":C" : "bad";
echo call_user_func_array("str_contains", ["abc", "x"]) ? "bad" : ":A";
echo ":"; echo function_exists("str_contains");');
"#,
    );
    assert_eq!(out, "Y:N:E:C:A:1");
}

/// Verifies eval `strpos()` and `strrpos()` return byte offsets or false.
#[test]
fn test_eval_dispatches_string_position_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strpos("banana", "na");
echo ":"; echo strrpos("banana", "na");
echo ":"; echo strpos("abc", "z") === false ? "F" : "bad";
echo ":"; echo strpos("abc", "");
echo ":"; echo strrpos("abc", "");
echo ":"; echo call_user_func("strpos", "abc", "b");
echo ":"; echo call_user_func_array("strrpos", ["ababa", "ba"]);
echo ":"; echo function_exists("strpos"); echo function_exists("strrpos");');
"#,
    );
    assert_eq!(out, "2:4:F:0:3:1:3:11");
}

/// Verifies eval `strstr()` returns matching suffixes, prefixes, and false for misses.
#[test]
fn test_eval_dispatches_strstr_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo strstr("user@example.com", "@"); echo ":";
echo strstr(haystack: "hello world", needle: "lo", before_needle: true); echo ":";
echo strstr("hello", "x") === false ? "F" : "bad"; echo ":";
echo strstr("hello", ""); echo ":";
echo call_user_func("strstr", "abcabc", "bc"); echo ":";
echo call_user_func_array("strstr", ["haystack" => "abcabc", "needle" => "bc", "before_needle" => true]);
echo ":"; echo function_exists("strstr");');
"#,
    );
    assert_eq!(out, "@example.com:hel:F:hello:bcabc:a:1");
}

/// Verifies eval string boundary builtins support direct and callable byte-string checks.
#[test]
fn test_eval_dispatches_string_boundary_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo str_starts_with("Hello World", "Hello") ? "S" : "bad";
echo str_starts_with("Hello", "World") ? "bad" : ":s";
echo str_starts_with("Hello", "") ? ":se" : "bad";
echo str_ends_with("Hello World", "World") ? ":E" : "bad";
echo str_ends_with("Hello", "World") ? "bad" : ":e";
echo str_ends_with("Hello", "") ? ":ee" : "bad";
echo call_user_func("str_starts_with", "abc", "a") ? ":CS" : "bad";
echo call_user_func_array("str_ends_with", ["abc", "c"]) ? ":CE" : "bad";
echo ":"; echo function_exists("str_starts_with"); echo function_exists("str_ends_with");');
"#,
    );
    assert_eq!(out, "S:s:se:E:e:ee:CS:CE:11");
}

/// Verifies eval string comparison builtins return compatible scalar results.
#[test]
fn test_eval_dispatches_string_compare_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo strcmp("abc", "abc");
echo ":"; echo strcmp("abc", "abd") < 0 ? "lt" : "bad";
echo ":"; echo strcasecmp("Hello", "hello");
echo ":"; echo call_user_func("strcmp", "b", "a") > 0 ? "gt" : "bad";
echo ":"; echo call_user_func_array("strcasecmp", ["A", "a"]) === 0 ? "ci" : "bad";
echo ":"; echo hash_equals("abc", "abc") ? "heq" : "bad";
echo ":"; echo hash_equals("abc", "abcd") ? "bad" : "hlen";
echo ":"; echo call_user_func("hash_equals", "abc", "abd") ? "bad" : "hneq";
echo ":"; echo function_exists("strcmp"); echo function_exists("strcasecmp"); echo function_exists("hash_equals");');
"#,
    );
    assert_eq!(out, "0:lt:0:gt:ci:heq:hlen:hneq:111");
}

/// Verifies eval trim-like builtins strip default and explicit masks.
#[test]
fn test_eval_dispatches_trim_like_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo "[" . trim("  hello  ") . "]";
echo ":[" . ltrim("  left") . "]";
echo ":[" . rtrim("right  ") . "]";
echo ":[" . chop("tail... ", " .") . "]";
echo ":[" . trim("**boxed**", "*") . "]";
echo ":[" . call_user_func("trim", "  cuf  ") . "]";
echo ":[" . call_user_func_array("ltrim", ["0007", "0"]) . "]";
echo ":"; echo function_exists("trim"); echo function_exists("ltrim"); echo function_exists("rtrim"); echo function_exists("chop");');
"#,
    );
    assert_eq!(out, "[hello]:[left]:[right]:[tail]:[boxed]:[cuf]:[7]:1111");
}

/// Verifies eval scalar type-predicate builtins inspect boxed Mixed runtime tags.
#[test]
fn test_eval_dispatches_type_predicate_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
class EvalAotPredicateIterator implements Iterator {
    private int $i = 0;
    public function current(): int { return $this->i; }
    public function key(): int { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
    public function valid(): bool { return $this->i < 0; }
    public function rewind(): void { $this->i = 0; }
}
$iterator = new EvalAotPredicateIterator();
eval('echo is_int(1); echo is_integer(1); echo is_long(1);
echo is_float(1.5); echo is_double(1.5); echo is_real(1.5);
echo is_string("x"); echo is_bool(false); echo is_null(null);
echo is_array([1]); echo is_array(["a" => 1]);
echo is_iterable([1]); echo is_iterable(["a" => 1]);
echo is_iterable($iterator) ? "I" : "bad";
echo is_iterable(1) ? "bad" : "T";
echo is_array(1) ? "bad" : "ok";
echo is_numeric(42); echo is_numeric(3.14); echo is_numeric("42");
echo is_numeric("-5"); echo is_numeric("3.14");
echo is_numeric("abc") ? "bad" : "N";
echo is_numeric(true) ? "bad" : "B";
echo is_resource(1) ? "bad" : "R";
$object = json_decode("{}");
echo is_object($object) ? "O" : "bad";
echo is_object([1]) ? "bad" : "o";
echo is_nan(fdiv(0, 0)) ? "N" : "bad";
echo is_infinite(fdiv(1, 0)) ? "I" : "bad";
echo is_infinite(fdiv(-1, 0)) ? "i" : "bad";
echo is_finite(42) ? "F" : "bad";
echo is_finite(fdiv(1, 0)) ? "bad" : "f";
echo is_resource($h) ? "H" : "bad";
echo ":";
echo call_user_func("is_string", "x");
echo call_user_func_array("is_array", [[1]]);
echo call_user_func("is_numeric", "12");
echo call_user_func("is_iterable", [1]);
echo call_user_func("is_iterable", $iterator) ? "C" : "bad";
echo call_user_func_array("is_iterable", ["value" => $iterator]) ? "D" : "bad";
echo call_user_func_array("is_iterable", ["value" => 1]) ? "bad" : "t";
echo call_user_func("is_resource", $h);
echo call_user_func_array("is_resource", [$h]);
echo call_user_func("is_object", $object) ? "O" : "bad";
echo call_user_func_array("is_object", ["value" => 1]) ? "bad" : "o";
echo call_user_func("is_nan", fdiv(0, 0)) ? "N" : "bad";
echo call_user_func_array("is_finite", [42]) ? "F" : "bad";
echo function_exists("is_double"); echo function_exists("is_numeric"); echo function_exists("is_object"); echo function_exists("is_resource");
echo function_exists("is_nan"); echo function_exists("is_finite"); echo function_exists("is_iterable"); echo function_exists("is_infinite");');
"#,
    );
    assert_eq!(
        out,
        "1111111111111ITok11111NBROoNIiFfH:1111CDt11OoNF11111111"
    );
}

/// Verifies eval resource introspection builtins inspect boxed runtime resources.
#[test]
fn test_eval_dispatches_resource_introspection_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
eval('echo get_resource_type($h);
echo ":"; echo get_resource_id($h) > 0 ? "id" : "bad";
echo ":"; echo call_user_func("get_resource_type", $h);
echo ":"; echo call_user_func_array("get_resource_id", ["resource" => $h]) > 0 ? "id" : "bad";
echo ":"; echo function_exists("get_resource_type"); echo function_exists("get_resource_id");');
"#,
    );
    assert_eq!(out, "stream:id:stream:id:11");
}

/// Verifies eval scalar cast builtins return boxed Mixed cells through direct and callable calls.
#[test]
fn test_eval_dispatches_cast_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo intval("42"); echo ":";
echo floatval("3.5"); echo ":";
echo strval(12); echo ":";
echo boolval("0") ? "bad" : "false";
echo ":"; echo call_user_func("strval", 7);
echo ":"; echo call_user_func_array("intval", ["9"]);
echo ":"; echo function_exists("boolval");');
"#,
    );
    assert_eq!(out, "42:3.5:12:false:7:9:1");
}

/// Verifies eval-declared `__toString()` runs in string contexts through the bridge.
#[test]
fn test_eval_declared_tostring_string_contexts() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalStringableBox {
    public string $name = "Ada";
    public function __toString() {
        return "box:" . $this->name;
    }
    public function accepts(string $value) {
        return "typed:" . $value;
    }
}
$box = new EvalStringableBox();
echo $box; echo ":";
print $box; echo ":";
echo "pre" . $box; echo ":";
echo strval($box); echo ":";
echo call_user_func("strval", $box); echo ":";
echo call_user_func_array("strval", [$box]); echo ":";
echo $box instanceof Stringable ? "S" : "s"; echo ":";
echo $box->accepts($box);');
"#,
    );
    assert_eq!(
        out,
        "box:Ada:box:Ada:prebox:Ada:box:Ada:box:Ada:box:Ada:S:typed:box:Ada"
    );
}

/// Verifies eval-declared objects support nullsafe property reads and method calls.
#[test]
fn test_eval_declared_nullsafe_property_and_method_access() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalNullsafeProfile {
    public string $name = "Ada";

    public function label($value) {
        echo "method:";
        return $this->name . ":" . $value;
    }
}

class EvalNullsafeUser {
    public $profile = null;
}

function eval_nullsafe_side() {
    echo "bad";
    return "side";
}

$with = new EvalNullsafeUser();
$with->profile = new EvalNullsafeProfile();
$without = new EvalNullsafeUser();

echo $with->profile?->name ?? "none"; echo "|";
echo $without->profile?->name ?? "none"; echo "|";
echo $with?->profile?->label("ok") ?? "none"; echo "|";
echo $without?->profile?->label(eval_nullsafe_side()) ?? "none";');
"#,
    );
    assert_eq!(out, "Ada|none|method:Ada:ok|none");
}

/// Verifies eval-declared objects support runtime-name property reads and method calls.
#[test]
fn test_eval_declared_dynamic_property_and_method_access() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalDynamicMemberProfile {
    public string $name = "Ada";

    public function label($value) {
        echo "call:";
        return $this->name . ":" . $value;
    }
}

class EvalDynamicMemberUser {
    public $profile = null;
}

function eval_dynamic_member_name() {
    echo "name:";
    return "profile";
}

function eval_dynamic_member_method() {
    echo "methodName:";
    return "label";
}

function eval_dynamic_member_bad() {
    echo "bad";
    return "profile";
}

$with = new EvalDynamicMemberUser();
$with->profile = new EvalDynamicMemberProfile();
$missing = null;
$name = "profile";
$method = "label";

echo $with->{$name}->name; echo "|";
echo $with?->{eval_dynamic_member_name()}?->name ?? "none"; echo "|";
echo $with->{$name}->$method("ok"); echo "|";
echo $with->{$name}->{eval_dynamic_member_method()}("yes"); echo "|";
echo $missing?->{eval_dynamic_member_bad()} ?? "none"; echo "|";
echo $missing?->{eval_dynamic_member_bad()}(eval_dynamic_member_bad()) ?? "none";');
"#,
    );
    assert_eq!(
        out,
        "Ada|name:Ada|call:Ada:ok|methodName:call:Ada:yes|none|none"
    );
}

/// Verifies eval-declared objects support runtime-name property writes and probes.
#[test]
fn test_eval_declared_dynamic_property_write_unset_isset_empty() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalDynamicWriteBox {
    public $name = "old";
    public $blank = "";
}

function eval_dynamic_write_name() {
    echo "name:";
    return "name";
}

function eval_dynamic_write_value() {
    echo "value:";
    return "Ada";
}

function eval_dynamic_write_bad() {
    echo "bad";
    return "name";
}

$box = new EvalDynamicWriteBox();
$name = "name";
$blank = "blank";
$box->{eval_dynamic_write_name()} = eval_dynamic_write_value();
echo $box->name; echo "|";
echo isset($box->{$name}) ? "set" : "bad"; echo "|";
echo empty($box->{$blank}) ? "empty" : "bad"; echo "|";
unset($box->{$name});
echo isset($box->{$name}) ? "bad" : "unset"; echo "|";
$missing = null;
echo isset($missing?->{eval_dynamic_write_bad()}) ? "bad" : "nullset"; echo "|";
echo empty($missing?->{eval_dynamic_write_bad()}) ? "nullempty" : "bad";');
"#,
    );
    assert_eq!(out, "name:value:Ada|set|empty|unset|nullset|nullempty");
}

/// Verifies eval-declared object properties can bind to local variables by reference.
#[test]
fn test_eval_declared_property_reference_binding() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalPropertyReferenceBindBox {
    public $value = "old";
    public $other = "old";
}

$box = new EvalPropertyReferenceBindBox();
$source = "A";
$box->value =& $source;
$source = "B";
echo $box->value . "|";
$box->value = "C";
echo $source . "|";

$name = "other";
$dynamic = "D";
$box->{$name} =& $dynamic;
$dynamic = "E";
echo $box->other . "|";
$box->{$name} = "F";
echo $dynamic;');
"#,
    );
    assert_eq!(out, "B|C|E|F");
}

/// Verifies eval object property increment/decrement works with named and dynamic properties.
#[test]
fn test_eval_object_property_inc_dec_statements() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotPropertyIncDec {
    public int $count = 10;
}
eval('class EvalDynamicPropertyIncDec {
    public $count = 1;
}
function eval_property_inc_name() {
    echo "n|";
    return "count";
}
$box = new EvalDynamicPropertyIncDec();
$box->count++;
++$box->count;
$name = "count";
$box->{$name}++;
--$box->{$name};
++$box->{$name};
$box->{eval_property_inc_name()}++;
--$box->{eval_property_inc_name()};
$i = 0;
for (; $i < 3; $box->count++) {
    $i++;
}
echo $box->count; echo "|";
$aot = new EvalAotPropertyIncDec();
$aot->count++;
++$aot->count;
$aot->count--;
--$aot->count;
echo $aot->count;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "n|n|7|10");
}

/// Verifies eval object property compound assignments work for named and dynamic properties.
#[test]
fn test_eval_object_property_compound_assignment_statements() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotPropertyCompoundAssign {
    public int $count = 10;
    public string $label = "a";
}
eval('class EvalDynamicPropertyCompoundAssign {
    public $count = 1;
    public $label = "x";
}
function eval_property_compound_name() {
    echo "n|";
    return "count";
}
function eval_property_compound_rhs() {
    echo "r|";
    return 4;
}
$box = new EvalDynamicPropertyCompoundAssign();
$box->count += 2;
$box->count *= 3;
$box->label .= "y";
$name = "count";
$box->{$name} -= 1;
$box->{eval_property_compound_name()} += eval_property_compound_rhs();
echo $box->count; echo ":"; echo $box->label; echo "|";
$aot = new EvalAotPropertyCompoundAssign();
$aot->count += 5;
$aot->count %= 6;
$aot->label .= "b";
echo $aot->count; echo ":"; echo $aot->label;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "n|r|12:xy|3:ab");
}

/// Verifies eval object property array writes and appends update property storage.
#[test]
fn test_eval_object_property_array_write_statements() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotPropertyArrayWrite {
    public array $items = [];
}
eval('class EvalDynamicPropertyArrayWrite {
    public $items = [];
    public $dyn = [];
}
function eval_property_array_name() {
    echo "n|";
    return "dyn";
}
$box = new EvalDynamicPropertyArrayWrite();
$box->items[0] = "a";
$box->items[] = "b";
$box->items[0] .= "A";
$name = "dyn";
$box->{$name}[1] = "x";
$box->{$name}[] = "y";
$box->{eval_property_array_name()}[] = "z";
echo $box->items[0] . ":" . $box->items[1] . ":";
echo $box->dyn[1] . ":" . $box->dyn[2] . ":" . $box->dyn[3] . "|";
$aot = new EvalAotPropertyArrayWrite();
$aot->items[0] = "m";
$aot->items[] = "n";
$aot->items[0] .= "M";
echo $aot->items[0] . ":" . $aot->items[1];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "n|aA:b:x:y:z|mM:n");
}

/// Verifies eval string contexts dispatch AOT `__toString()` through the runtime method bridge.
#[test]
fn test_eval_aot_tostring_string_contexts() {
    let out = compile_and_run(
        r#"<?php
class EvalAotStringableBox {
    public string $name = "Ada";
    public function __toString() {
        return "box:" . $this->name;
    }
    public function accepts(string $value) {
        return "typed:" . $value;
    }
}
$box = new EvalAotStringableBox();
eval('echo $box; echo ":";
print $box; echo ":";
echo "pre" . $box; echo ":";
echo strval($box); echo ":";
echo call_user_func("strval", $box); echo ":";
echo call_user_func_array("strval", [$box]); echo ":";
echo $box instanceof Stringable ? "S" : "s"; echo ":";
echo $box->accepts($box);');
"#,
    );
    assert_eq!(
        out,
        "box:Ada:box:Ada:prebox:Ada:box:Ada:box:Ada:box:Ada:S:typed:box:Ada"
    );
}

/// Verifies eval-declared objects without `__toString()` throw catchable PHP errors in string contexts.
#[test]
fn test_eval_declared_object_string_context_without_tostring_throws_error() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalPlainStringContext {}
$box = new EvalPlainStringContext();
try {
    echo $box;
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Object of class EvalPlainStringContext could not be converted to string"
    );
}

/// Verifies AOT objects stringified from eval throw PHP's catchable conversion error.
#[test]
fn test_eval_aot_object_string_context_without_tostring_throws_error() {
    let out = compile_and_run(
        r#"<?php
class EvalAotPlainStringContext {}
eval('$box = new EvalAotPlainStringContext();
try {
    echo $box;
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Object of class EvalAotPlainStringContext could not be converted to string"
    );
}

/// Verifies eval `settype()` mutates direct variables and supports named arguments.
#[test]
fn test_eval_dispatches_settype_builtin_call() {
    let out = compile_and_run(
        r#"<?php
class EvalAotSettypeBox {
    public mixed $value = 8;
    public static mixed $staticValue = 9;
}
eval('$x = 42;
echo settype($x, "string") ? gettype($x) . ":" . $x : "bad";
echo ":";
$y = "0";
echo settype(type: "bool", var: $y) ? gettype($y) . ":" . ($y ? "true" : "false") : "bad";
echo ":";
$items = ["k" => "6"];
echo settype($items["k"], "integer") ? gettype($items["k"]) . ":" . $items["k"] : "bad";
echo ":";
$box = new EvalAotSettypeBox();
echo settype($box->value, "string") ? gettype($box->value) . ":" . $box->value : "bad";
echo ":";
$name = "value";
echo settype($box->{$name}, "integer") ? gettype($box->value) . ":" . $box->value : "bad";
echo ":";
echo settype(EvalAotSettypeBox::$staticValue, "string") ? gettype(EvalAotSettypeBox::$staticValue) . ":" . EvalAotSettypeBox::$staticValue : "bad";
echo ":";
$class = "EvalAotSettypeBox";
$staticName = "staticValue";
echo settype($class::${$staticName}, "bool") ? gettype(EvalAotSettypeBox::$staticValue) . ":" . (EvalAotSettypeBox::$staticValue ? "true" : "false") : "bad";
echo ":";
echo function_exists("settype");');
"#,
    );
    assert_eq!(
        out,
        "string:42:boolean:false:integer:6:string:8:integer:8:string:9:boolean:true:1"
    );
}

/// Verifies eval SPL object identity builtins inspect AOT object cells.
#[test]
fn test_eval_dispatches_spl_object_identity_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
class EvalObjectIdentityProbe {}

eval('$a = new EvalObjectIdentityProbe();
$b = new EvalObjectIdentityProbe();
echo (spl_object_id($a) === spl_object_id($a)) ? "stable" : "drift";
echo ":";
echo (spl_object_id($a) !== spl_object_id($b)) ? "unique" : "same";
echo ":";
echo (spl_object_hash(object: $a) === spl_object_hash($a)) ? "hash" : "bad";
echo ":";
echo (call_user_func("spl_object_id", $a) === spl_object_id($a)) ? "call" : "bad";
echo ":";
echo (call_user_func_array("spl_object_hash", ["object" => $b]) === spl_object_hash($b)) ? "array" : "bad";
echo ":";
echo function_exists("spl_object_id"); echo function_exists("spl_object_hash");');
"#,
    );
    assert_eq!(out, "stable:unique:hash:call:array:11");
}

/// Verifies eval `gettype()` maps boxed Mixed runtime tags to PHP type names.
#[test]
fn test_eval_dispatches_gettype_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo gettype(1); echo ":";
echo gettype(1.5); echo ":";
echo gettype("x"); echo ":";
echo gettype(false); echo ":";
echo gettype(null); echo ":";
echo gettype([1]); echo ":";
echo gettype(["a" => 1]); echo ":";
echo call_user_func("gettype", true); echo ":";
echo call_user_func_array("gettype", [null]);
echo ":"; echo function_exists("gettype");');
"#,
    );
    assert_eq!(
        out,
        "integer:double:string:boolean:NULL:array:array:boolean:NULL:1"
    );
}

/// Verifies eval `get_class()` resolves stdClass and AOT object runtime names.
#[test]
fn test_eval_dispatches_get_class_builtin_call() {
    let out = compile_and_run(
        r#"<?php
class EvalClassNameProbe {}

eval('$object = json_decode("{}");
echo get_class($object) . ":";
$probe = new EvalClassNameProbe();
echo get_class($probe) . ":";
echo call_user_func("get_class", $object) . ":";
echo call_user_func_array("get_class", ["object" => $probe]) . ":";
echo function_exists("get_class");');
"#,
    );
    assert_eq!(
        out,
        "stdClass:EvalClassNameProbe:stdClass:EvalClassNameProbe:1"
    );
}

/// Verifies eval no-arg `get_class()` and `get_parent_class()` use method class scope.
#[test]
fn test_eval_dispatches_no_arg_class_name_builtins() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalNoArgRuntimeParent {}
class EvalNoArgRuntimeBase extends EvalNoArgRuntimeParent {
    public function inherited() {
        return get_class() . ":" . get_parent_class();
    }
    public function inheritedCallable() {
        return call_user_func("get_class") . ":" . call_user_func_array("get_parent_class", []);
    }
}
class EvalNoArgRuntimeChild extends EvalNoArgRuntimeBase {
    public function own() {
        return get_class() . ":" . get_parent_class();
    }
}
$child = new EvalNoArgRuntimeChild();
echo $child->inherited() . ":";
echo $child->inheritedCallable() . ":";
echo $child->own() . ":";
try {
    get_class();
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
echo get_parent_class();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalNoArgRuntimeBase:EvalNoArgRuntimeParent:EvalNoArgRuntimeBase:EvalNoArgRuntimeParent:EvalNoArgRuntimeChild:EvalNoArgRuntimeBase:Error:get_class() without arguments must be called from within a class:"
    );
}

/// Verifies eval `get_parent_class()` resolves AOT object and class-string parents.
#[test]
fn test_eval_dispatches_get_parent_class_builtin_call() {
    let out = compile_and_run(
        r#"<?php
class EvalParentBase {}
class EvalParentChild extends EvalParentBase {}

eval('$child = new EvalParentChild();
echo get_parent_class($child) . ":";
echo get_parent_class("EvalParentChild") . ":";
echo get_parent_class("evalparentchild") . ":";
echo call_user_func("get_parent_class", $child) . ":";
echo call_user_func_array("get_parent_class", ["object_or_class" => "EvalParentChild"]) . ":";
echo function_exists("get_parent_class");');
"#,
    );

    assert_eq!(
        out,
        "EvalParentBase:EvalParentBase:EvalParentBase:EvalParentBase:EvalParentBase:1"
    );
}

/// Verifies eval `define()` and `defined()` share dynamic constant names across fragments.
#[test]
fn test_eval_define_and_defined_dynamic_constants() {
    let out = compile_and_run_capture(
        r#"<?php
echo eval('return define("DynEvalConst", 7) ? "Y" : "N";');
echo eval('return defined("DynEvalConst") ? "Y" : "N";');
echo eval('return DynEvalConst;');
echo eval('return \DynEvalConst;');
echo eval('return defined("dynevalconst") ? "bad" : "N";');
echo eval('return define("DynEvalConst", 8) ? "bad" : "N";');
echo eval('return define(value: 9, constant_name: "DynEvalNamedConst") ? "Y" : "N";');
echo eval('return defined(constant_name: "DynEvalNamedConst") ? "Y" : "N";');
echo eval('return call_user_func("defined", "DynEvalConst") ? "Y" : "N";');
echo eval('return call_user_func_array("defined", ["constant_name" => "DynEvalConst"]) ? "Y" : "N";');
echo eval('return function_exists("define") && function_exists("defined") ? "Y" : "N";');
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "YY77NNYYYYY");
    assert!(
        out.stderr
            .contains("Warning: define(): Constant already defined"),
        "expected duplicate eval define warning, got stderr={}",
        out.stderr
    );
}

/// Verifies eval can read predefined runtime constants and protect them from redefinition.
#[test]
fn test_eval_reads_predefined_runtime_constants() {
    let out = compile_and_run_capture(
        r#"<?php
echo eval('return (PHP_EOL === "\n" ? "eol" : "bad") . ":" .
    ((PHP_OS === "Darwin" || PHP_OS === "Linux") ? "os" : "bad") . ":" .
    DIRECTORY_SEPARATOR . ":" .
    (PHP_INT_MAX > 9000000000000000000 ? "int" : "bad") . ":" .
    (defined("PHP_OS") ? "defined" : "bad") . ":" .
    (defined("\\\\PHP_OS") ? "root" : "bad") . ":" .
    (defined("php_os") ? "bad" : "case") . ":" .
    (define("PHP_OS", "x") ? "bad" : "locked");');
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "eol:os:/:int:defined:root:case:locked");
    assert!(
        out.stderr
            .contains("Warning: define(): Constant already defined"),
        "expected predefined eval define warning, got stderr={}",
        out.stderr
    );
}

/// Verifies `@eval(...)` suppresses duplicate eval `define()` warnings.
#[test]
fn test_error_control_suppresses_duplicate_eval_define_warning() {
    let out = compile_and_run_capture(
        r#"<?php
eval('define("DynEvalSuppressedConst", 1);');
echo @eval('return define("DynEvalSuppressedConst", 2) ? "bad" : "ok";');
echo eval('return DynEvalSuppressedConst;');
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok1");
    assert_eq!(out.stderr, "");
}

/// Verifies native `defined()` probes can see constants defined by eval after the barrier.
#[test]
fn test_eval_defined_constant_is_visible_to_native_defined_after_barrier() {
    let out = compile_and_run(
        r#"<?php
echo defined("DynEvalNativeDefinedConst") ? "bad" : "N";
eval('define("DynEvalNativeDefinedConst", 5);');
echo defined("DynEvalNativeDefinedConst") ? "Y" : "N";
echo defined("\\DynEvalNativeDefinedConst") ? "Y" : "N";
echo defined("dynevalnativedefinedconst") ? "bad" : "N";
"#,
    );
    assert_eq!(out, "NYYN");
}

/// Verifies native constant fetch can read eval-defined constants after the barrier.
#[test]
fn test_eval_defined_constant_is_visible_to_native_constant_fetch_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('define("DynEvalNativeFetchConst", "dynamic");');
echo DynEvalNativeFetchConst;
"#,
    );
    assert_eq!(out, "dynamic");
}

/// Verifies native constant fetch misses after eval fail through the eval runtime path.
#[test]
fn test_eval_missing_native_dynamic_constant_fetch_fails() {
    let err = compile_and_run_expect_failure("<?php eval(''); echo MissingNativeEvalConst;");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies missing eval dynamic constants fail through the eval runtime path.
#[test]
fn test_eval_missing_dynamic_constant_fetch_fails() {
    let err = compile_and_run_expect_failure("<?php eval('return MissingEvalConst;');");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies invalid eval fragments report the dedicated parse-error diagnostic.
#[test]
fn test_eval_parse_error_reports_eval_parse_diagnostic() {
    let err = compile_and_run_expect_failure("<?php eval('if (');");
    assert!(
        err.contains("Parse error: eval() fragment is invalid"),
        "stderr did not contain eval parse-error diagnostic: {err}"
    );
}

/// Verifies eval failure classes map to distinct stable user-facing diagnostics.
#[test]
fn test_eval_error_contract_distinguishes_parse_unsupported_runtime_and_warning() {
    let parse_err = compile_and_run_expect_failure("<?php eval('if (');");
    assert!(
        parse_err.contains("Parse error: eval() fragment is invalid"),
        "stderr did not contain eval parse-error diagnostic: {parse_err}"
    );
    assert_no_rust_panic_leaked(&parse_err);

    let unsupported_err = compile_and_run_expect_failure(
        "<?php eval('function eval_bad_static_return(): static {}');",
    );
    assert!(
        unsupported_err.contains("Fatal error: eval() fragment uses an unsupported construct"),
        "stderr did not contain eval unsupported-construct diagnostic: {unsupported_err}"
    );
    assert_no_rust_panic_leaked(&unsupported_err);

    let runtime_err =
        compile_and_run_expect_failure("<?php eval('return MissingEvalContractConst;');");
    assert!(
        runtime_err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime-fatal diagnostic: {runtime_err}"
    );
    assert_no_rust_panic_leaked(&runtime_err);

    let warning = compile_and_run_capture(
        r#"<?php
eval('define("EvalErrorContractConst", 1);');
echo eval('return define("EvalErrorContractConst", 2) ? "bad" : "ok";');
"#,
    );
    assert!(
        warning.success,
        "warning fixture should not fail: {}",
        warning.stderr
    );
    assert_eq!(warning.stdout, "ok");
    assert!(
        warning
            .stderr
            .contains("Warning: define(): Constant already defined"),
        "stderr did not contain eval warning diagnostic: {}",
        warning.stderr
    );
}

/// Verifies malformed input, builtin failure, and non-callables do not leak Rust panics.
#[test]
fn test_eval_bridge_failure_paths_do_not_leak_rust_panics() {
    for (source, expected) in [
        (
            "<?php eval('if (');",
            "Parse error: eval() fragment is invalid",
        ),
        (
            "<?php eval('clamp(5, 10, 0);');",
            "Fatal error: eval() runtime failed",
        ),
        (
            concat!(
                "<?php eval('class EvalPanicBoundaryPlainCallback {} ",
                "$callback = new EvalPanicBoundaryPlainCallback(); ",
                "call_user_func($callback);');"
            ),
            "Fatal error: uncaught exception",
        ),
    ] {
        let err = compile_and_run_expect_failure(source);
        assert!(
            err.contains(expected),
            "stderr did not contain expected eval diagnostic `{expected}`: {err}"
        );
        assert_no_rust_panic_leaked(&err);
    }
}

/// Verifies eval `abs()` preserves integer/float result typing through direct and callable calls.
#[test]
fn test_eval_dispatches_abs_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo abs(-5); echo ":";
echo abs(-2.5); echo ":";
echo gettype(abs(-2.5)); echo ":";
echo call_user_func("abs", -7); echo ":";
echo call_user_func_array("abs", [-9]);
echo ":"; echo function_exists("abs");');
"#,
    );
    assert_eq!(out, "5:2.5:double:7:9:1");
}

/// Verifies eval `floor()` and `ceil()` return boxed double cells through direct and callable calls.
#[test]
fn test_eval_dispatches_floor_and_ceil_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo floor(3.7); echo ":";
echo gettype(floor(3)); echo ":";
echo ceil(3.2); echo ":";
echo gettype(ceil(3)); echo ":";
echo call_user_func("floor", 4.9); echo ":";
echo call_user_func_array("ceil", [4.1]);
echo ":"; echo function_exists("floor"); echo function_exists("ceil");');
"#,
    );
    assert_eq!(out, "3:double:4:double:4:5:11");
}

/// Verifies eval `fdiv()` and `fmod()` return boxed double cells through direct and callable calls.
#[test]
fn test_eval_dispatches_float_binary_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo fdiv(10, 4); echo ":";
echo gettype(fdiv(10, 4)); echo ":";
echo fdiv(1, 0); echo ":";
echo fdiv(0, 0); echo ":";
echo round(fmod(10.5, 3.2), 1); echo ":";
echo round(call_user_func("fdiv", 9, 2), 1); echo ":";
echo round(call_user_func_array("fmod", [10.5, 3.2]), 1);
echo ":"; echo function_exists("fdiv"); echo function_exists("fmod");');
"#,
    );
    assert_eq!(out, "2.5:double:INF:NAN:0.9:4.5:0.9:11");
}

/// Verifies eval extended scalar math builtins through direct, named, callable, and probe paths.
#[test]
fn test_eval_dispatches_extended_math_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo sin(0); echo ":";
echo cos(0); echo ":";
echo tan(0); echo ":";
echo round(asin(1), 2); echo ":";
echo acos(1); echo ":";
echo round(atan(1), 2); echo ":";
echo sinh(0); echo ":";
echo cosh(0); echo ":";
echo tanh(0); echo ":";
echo log2(8); echo ":";
echo log10(100); echo ":";
echo exp(0); echo ":";
echo round(deg2rad(180), 2); echo ":";
echo round(rad2deg(pi()), 0); echo ":";
echo log(num: 8, base: 2); echo ":";
echo atan2(y: 0, x: 1); echo ":";
echo hypot(3, 4); echo ":";
echo intdiv(7, 2); echo ":";
echo round(call_user_func("sin", pi() / 2), 0); echo ":";
echo call_user_func_array("intdiv", ["num1" => 9, "num2" => 2]); echo ":";
echo function_exists("sin"); echo function_exists("log"); echo function_exists("intdiv"); echo function_exists("hypot");');
"#,
    );
    assert_eq!(
        out,
        "0:1:0:1.57:0:0.79:0:1:0:3:2:1:3.14:180:3:0:5:3:1:4:1111"
    );
}

/// Verifies eval `pow()` reuses exponentiation runtime hooks through direct and callable calls.
#[test]
fn test_eval_dispatches_pow_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo pow(2, 3); echo ":";
echo gettype(pow(2, 3)); echo ":";
echo call_user_func("pow", 2, 5); echo ":";
echo call_user_func_array("pow", [3, 3]);
echo ":"; echo function_exists("pow");');
"#,
    );
    assert_eq!(out, "8:double:32:27:1");
}

/// Verifies eval `round()` supports default and explicit precision through callable paths.
#[test]
fn test_eval_dispatches_round_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo round(3.5); echo ":";
echo round(3.14159, 2); echo ":";
echo gettype(round(3)); echo ":";
echo call_user_func("round", 2.5); echo ":";
echo call_user_func_array("round", [1.55, 1]);
echo ":"; echo function_exists("round");');
"#,
    );
    assert_eq!(out, "4:3.14:double:3:1.6:1");
}

/// Verifies eval `number_format()` groups and rounds numbers through callable paths.
#[test]
fn test_eval_dispatches_number_format_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo number_format(1234567); echo ":";
echo number_format(1234.5678, 2); echo ":";
echo number_format(num: 1234567.89, decimals: 2, decimal_separator: ",", thousands_separator: "."); echo ":";
echo number_format(1234567.89, 2, ".", ""); echo ":";
echo call_user_func("number_format", -1234.5, 1); echo ":";
echo call_user_func_array("number_format", ["num" => 1234, "decimals" => 0, "decimal_separator" => ".", "thousands_separator" => " "]);
echo ":"; echo function_exists("number_format");');
"#,
    );
    assert_eq!(
        out,
        "1,234,567:1,234.57:1.234.567,89:1234567.89:-1,234.5:1 234:1"
    );
}

/// Verifies eval printf-family builtins format, print, and dispatch through callables.
#[test]
fn test_eval_dispatches_printf_family_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo sprintf("Hello %s", "World"); echo ":";
echo sprintf("%05d", 42); echo ":";
echo sprintf("%.2f", 3.14159); echo ":";
echo sprintf("%-6s|", "hi"); echo ":";
$printed = printf("%s=%d", "n", 42);
echo ":" . $printed . ":";
echo vsprintf("%s/%d/%.1f", ["age", 42, 3]); echo ":";
$vprinted = vprintf("%s-%d", ["v", 7]);
echo ":" . $vprinted . ":";
echo call_user_func("sprintf", "%+d", 42); echo ":";
echo call_user_func_array("vsprintf", ["format" => "%s", "values" => ["spread"]]); echo ":";
echo function_exists("sprintf"); echo is_callable("printf"); echo function_exists("vsprintf"); echo is_callable("vprintf");');
"#,
    );
    assert_eq!(
        out,
        "Hello World:00042:3.14:hi    |:n=42:4:age/42/3.0:v-7:3:+42:spread:1111"
    );
}

/// Verifies eval `sscanf()` returns indexed string matches through direct and callable paths.
#[test]
fn test_eval_dispatches_sscanf_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('$result = sscanf("John 1.5 30", "%s %f %d");
echo $result[0] . ":" . $result[1] . ":" . $result[2] . ":";
$named = sscanf(string: "Age: -25", format: "Age: %d");
echo $named[0] . ":";
$call = call_user_func("sscanf", "-2.5e3", "%f");
echo $call[0] . ":";
$spread = call_user_func_array("sscanf", ["string" => "ok %", "format" => "%s %%"]);
echo $spread[0] . ":";
echo function_exists("sscanf");');
"#,
    );
    assert_eq!(out, "John:1.5:30:-25:-2.5e3:ok:1");
}

/// Verifies eval `min()` and `max()` select numeric values directly and through callables.
#[test]
fn test_eval_dispatches_min_max_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo min(3, 1, 2); echo ":";
echo max(1, 3, 2); echo ":";
echo min(2.5, 1.5); echo ":";
echo max(1.5, 2.5); echo ":";
echo call_user_func("min", 9, 4, 7); echo ":";
echo call_user_func_array("max", [4, 8, 6]);
echo ":"; echo function_exists("min"); echo function_exists("max");');
"#,
    );
    assert_eq!(out, "1:3:1.5:2.5:4:8:11");
}

/// Verifies eval `clamp()` selects numeric values directly and through callables.
#[test]
fn test_eval_dispatches_clamp_builtin_calls() {
    let out = compile_and_run(
        r#"<?php
eval('echo clamp(5, 0, 10); echo ":";
echo clamp(15, 0, 10); echo ":";
echo clamp(-5, 0, 10); echo ":";
echo clamp(2.75, 1.5, 2.5); echo ":";
echo clamp(value: 8, min: 0, max: 5); echo ":";
echo call_user_func("clamp", -1, 0, 10); echo ":";
echo call_user_func_array("clamp", ["value" => 9, "min" => 0, "max" => 7]);
echo ":"; echo function_exists("clamp"); echo is_callable("clamp");');
"#,
    );
    assert_eq!(out, "5:10:0:2.5:5:0:7:11");
}

/// Verifies eval `pi()` returns the PHP math constant through direct and callable calls.
#[test]
fn test_eval_dispatches_pi_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo round(pi(), 2); echo ":";
echo gettype(pi()); echo ":";
echo round(call_user_func("pi"), 3); echo ":";
echo round(call_user_func_array("pi", []), 4);
echo ":"; echo function_exists("pi");');
"#,
    );
    assert_eq!(out, "3.14:double:3.142:3.1416:1");
}

/// Verifies eval `sqrt()` returns boxed double cells through direct and callable calls.
#[test]
fn test_eval_dispatches_sqrt_builtin_call() {
    let out = compile_and_run(
        r#"<?php
eval('echo sqrt(16); echo ":";
echo gettype(sqrt(9)); echo ":";
echo call_user_func("sqrt", 25); echo ":";
echo call_user_func_array("sqrt", [36]);
echo ":"; echo function_exists("sqrt");');
"#,
    );
    assert_eq!(out, "4:double:5:6:1");
}

/// Verifies eval `isset()` distinguishes missing, null, and falsey non-null values.
#[test]
fn test_eval_isset_distinguishes_missing_null_and_falsey_values() {
    let out = compile_and_run(
        r#"<?php
$nullish = null;
$zero = 0;
$empty = "";
eval('if (isset($missing)) { echo "1"; } else { echo "0"; }
if (isset($nullish)) { echo "1"; } else { echo "0"; }
if (isset($zero)) { echo "1"; } else { echo "0"; }
if (isset($empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $nullish)) { echo "1"; } else { echo "0"; }
echo function_exists("isset") . "x";');
"#,
    );
    assert_eq!(out, "0011101x");
}

/// Verifies eval `empty()` uses PHP truthiness without warning on missing variables.
#[test]
fn test_eval_empty_uses_php_truthiness_without_missing_warnings() {
    let out = compile_and_run(
        r#"<?php
$nullish = null;
$zero = 0;
$empty = "";
$zero_string = "0";
$value = "x";
eval('if (empty($missing)) { echo "1"; } else { echo "0"; }
if (empty($nullish)) { echo "1"; } else { echo "0"; }
if (empty($zero)) { echo "1"; } else { echo "0"; }
if (empty($empty)) { echo "1"; } else { echo "0"; }
if (empty($zero_string)) { echo "1"; } else { echo "0"; }
if (empty($value)) { echo "1"; } else { echo "0"; }
echo function_exists("empty") . "x";');
"#,
    );
    assert_eq!(out, "1111101x");
}

/// Verifies eval `isset()` and `empty()` use PHP offset semantics for array reads.
#[test]
fn test_eval_isset_and_empty_support_array_offsets() {
    let out = compile_and_run(
        r#"<?php
$map = eval('return [
    "present" => "x",
    "nullish" => null,
    "zero" => 0,
    "empty" => "",
    "child" => ["leaf" => "ok", "null" => null],
];');
eval('echo isset($map["present"]) ? "1" : "0";
echo isset($map["nullish"]) ? "1" : "0";
echo isset($map["missing"]) ? "1" : "0";
echo isset($map["zero"]) ? "1" : "0";
echo isset($map["child"]["leaf"]) ? "1" : "0";
echo isset($map["child"]["null"]) ? "1" : "0";
echo isset($map["missing"]["leaf"]) ? "1" : "0";
echo ":";
echo empty($map["present"]) ? "1" : "0";
echo empty($map["nullish"]) ? "1" : "0";
echo empty($map["missing"]) ? "1" : "0";
echo empty($map["zero"]) ? "1" : "0";
echo empty($map["empty"]) ? "1" : "0";
echo empty($map["child"]["leaf"]) ? "1" : "0";
echo empty($map["child"]["null"]) ? "1" : "0";
echo empty($map["missing"]["leaf"]) ? "1" : "0";');
"#,
    );
    assert_eq!(out, "1001100:01111011");
}

/// Verifies eval builtin dispatch can inspect arrays from the caller scope.
#[test]
fn test_eval_count_reads_scope_array() {
    let out = compile_and_run(
        r#"<?php
$items = eval('return ["a", "b"];');
eval('echo count($items);');
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies eval-declared functions can be called inside the same fragment.
#[test]
fn test_eval_declared_function_can_be_called_in_fragment() {
    let out = compile_and_run(
        r#"<?php
echo eval('function dyn_eval_add($x) { return $x + 1; } return dyn_eval_add(4);');
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval-declared functions bind named arguments inside eval fragments.
#[test]
fn test_eval_declared_function_accepts_named_args_in_fragment() {
    let out = compile_and_run(
        r#"<?php
echo eval('function dyn_eval_named($x, $y) { return ($x * 10) + $y; } return dyn_eval_named(y: 2, x: 1);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies eval-declared functions unpack spread arguments inside eval fragments.
#[test]
fn test_eval_declared_function_accepts_spread_args_in_fragment() {
    let out = compile_and_run(
        r#"<?php
echo eval('function dyn_eval_spread($x, $y) { return ($x * 10) + $y; } return dyn_eval_spread(...[1, 2]);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies eval magic constants use fragment line and eval-declared function metadata.
#[test]
fn test_eval_magic_line_function_and_method_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval("
echo __LINE__ . ':';
");
eval('function DynEvalMagic() { return __FUNCTION__ . ":" . __METHOD__; } echo dynevalmagic();');
"#,
    );
    assert_eq!(out, "2:DynEvalMagic:DynEvalMagic");
}

/// Verifies eval file-dependent magic constants use call-site metadata in EIR AOT.
#[test]
fn test_literal_eval_magic_file_and_dir_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_magic_file_dir_aot");
    let source = r#"<?php
eval('if (strlen(__DIR__) > 0) { echo "D"; } else { echo "d"; }
echo ":";
if (strlen(__FILE__) > strlen(__DIR__)) { echo "F"; } else { echo "f"; }');
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval __FILE__/__DIR__ should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval __FILE__/__DIR__ should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval __FILE__/__DIR__ should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval __FILE__/__DIR__ should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval __FILE__/__DIR__ should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "D:F");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies eval scope magic constants are empty through EIR AOT even from namespaced methods.
#[test]
fn test_literal_eval_scope_magic_constants_use_eir_aot_without_magician() {
    let dir = make_cli_test_dir("elephc_literal_eval_scope_magic_aot");
    let source = r#"<?php
namespace EvalMagicScope;
class Box {
    public function run() {
        eval('echo "[" . __CLASS__ . "|" . __NAMESPACE__ . "|" . __TRAIT__ . "|" . __FUNCTION__ . "|" . __METHOD__ . "]";');
    }
}
(new Box())->run();
"#;
    let (user_asm, runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    assert!(
        user_asm.contains("eval literal AOT compiled EIR function"),
        "literal eval scope magic constants should use the internal EIR AOT function path:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_execute"),
        "literal eval scope magic constants should not call the interpreter bridge:\n{user_asm}"
    );
    assert!(
        !user_asm.contains("__elephc_eval_"),
        "native-only literal eval scope magic constants should not reference eval bridge helpers:\n{user_asm}"
    );
    assert!(
        !runtime_asm.contains("__elephc_eval_"),
        "native-only literal eval scope magic constants should not emit eval bridge runtime helpers:\n{runtime_asm}"
    );
    assert!(
        !required_libraries
            .iter()
            .any(|lib| lib == "elephc_magician"),
        "native-only literal eval scope magic constants should not link elephc_magician: {required_libraries:?}"
    );
    let runtime_obj = runtime_obj_for_asm(&runtime_asm);
    let out = assemble_and_run(
        &user_asm,
        &runtime_obj,
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "[||||]");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies eval trait methods expose PHP magic constants from the declaring trait.
#[test]
fn test_eval_trait_method_magic_constants_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('namespace EvalBridgeTraitMagic;
trait Inner {
    public function report() {
        return __NAMESPACE__ . "|" . __CLASS__ . "|" . __TRAIT__ . "|" . __METHOD__ . "|" . __FUNCTION__;
    }
    public static function stat() {
        return __NAMESPACE__ . "|" . __CLASS__ . "|" . __TRAIT__ . "|" . __METHOD__ . "|" . __FUNCTION__;
    }
}
trait Outer {
    use Inner {
        report as aliasReport;
        stat as aliasStat;
    }
}
class Box {
    use Outer;
}
echo (new Box())->aliasReport(); echo ":";
echo Box::aliasStat();');
"#,
    );
    let expected = concat!(
        "EvalBridgeTraitMagic|EvalBridgeTraitMagic\\Box|",
        "EvalBridgeTraitMagic\\Inner|",
        "EvalBridgeTraitMagic\\Inner::report|report:",
        "EvalBridgeTraitMagic|EvalBridgeTraitMagic\\Box|",
        "EvalBridgeTraitMagic\\Inner|",
        "EvalBridgeTraitMagic\\Inner::stat|stat"
    );

    assert_eq!(out, expected);
}

/// Verifies eval trait member defaults expose PHP magic constants through the bridge.
#[test]
fn test_eval_trait_member_default_magic_constants_execute_through_bridge() {
    let out = compile_and_run(
        r#"<?php
eval('namespace EvalBridgeDefaultMagic;
trait Inner {
    public const C = __CLASS__;
    public const T = __TRAIT__;
    public string $p = __CLASS__;
    public string $pt = __TRAIT__;
    public static string $sp = __CLASS__;
    public static string $st = __TRAIT__;
}
trait Outer {
    use Inner;
}
class Base {
    use Outer;
}
class Child extends Base {}
class Direct {
    public const C = __CLASS__;
    public const T = __TRAIT__;
    public string $p = __CLASS__;
    public string $pt = __TRAIT__;
    public static string $sp = __CLASS__;
    public static string $st = __TRAIT__;
}
$object = new Child();
$traitProps = (new \ReflectionClass(Inner::class))->getDefaultProperties();
echo Base::C . "|" . Base::T . "|";
echo Child::C . "|" . Child::T . "|";
echo $object->p . "|" . $object->pt . "|";
echo Child::$sp . "|" . Child::$st . "|";
echo $traitProps["p"] . "|" . $traitProps["pt"] . ":";
$direct = new Direct();
echo Direct::C . "|" . Direct::T . "|" . $direct->p . "|" . $direct->pt . "|" . Direct::$sp . "|" . Direct::$st;');
"#,
    );
    let expected = concat!(
        "EvalBridgeDefaultMagic\\Base|EvalBridgeDefaultMagic\\Inner|",
        "EvalBridgeDefaultMagic\\Base|EvalBridgeDefaultMagic\\Inner|",
        "EvalBridgeDefaultMagic\\Base|EvalBridgeDefaultMagic\\Inner|",
        "EvalBridgeDefaultMagic\\Base|EvalBridgeDefaultMagic\\Inner|",
        "EvalBridgeDefaultMagic\\Inner|EvalBridgeDefaultMagic\\Inner:",
        "EvalBridgeDefaultMagic\\Direct||",
        "EvalBridgeDefaultMagic\\Direct||",
        "EvalBridgeDefaultMagic\\Direct|"
    );

    assert_eq!(out, expected);
}

/// Verifies eval-declared functions persist across eval calls in the same generated context.
#[test]
fn test_eval_declared_function_persists_across_eval_calls() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inc($x) { return $x + 1; }');
eval('echo dyn_eval_inc(4);');
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies native code can call a zero-argument function declared by eval.
#[test]
fn test_eval_declared_function_can_be_called_from_native_code() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_native() { return 42; }');
echo dyn_eval_native();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies static locals in eval-declared functions persist between native calls.
#[test]
fn test_eval_declared_function_static_local_persists() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_static_counter() { static $n = 0; $n++; return $n; }');
echo dyn_eval_static_counter();
echo ":";
echo dyn_eval_static_counter();
"#,
    );
    assert_eq!(out, "1:2");
}

/// Verifies top-level static locals inside separate eval calls are reinitialized like PHP.
#[test]
fn test_eval_top_level_static_var_reinitializes_per_eval_call() {
    let out = compile_and_run(
        r#"<?php
eval('static $n = 0; $n++; echo $n;');
echo ":";
eval('static $n = 0; $n++; echo $n;');
"#,
    );
    assert_eq!(out, "1:1");
}

/// Verifies a top-level eval static declared without an initializer defaults to null.
#[test]
fn test_eval_top_level_static_var_without_initializer_defaults_to_null() {
    let out = compile_and_run(
        r#"<?php
eval('static $x; var_dump($x);');
"#,
    );
    assert_eq!(out, "NULL\n");
}

/// Verifies a dynamic (interpreter-path) eval static declared without an initializer defaults to null.
#[test]
fn test_eval_dynamic_static_var_without_initializer_defaults_to_null() {
    let out = compile_and_run(
        r#"<?php
$prefix = 'static $x; ';
eval($prefix . 'var_dump($x);');
"#,
    );
    assert_eq!(out, "NULL\n");
}

/// Verifies eval inside a closure can mutate the closure's by-value capture without touching the outer variable.
#[test]
fn test_eval_inside_closure_updates_by_value_capture_copy() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$fn = function() use ($x) {
    eval('$x = $x + 4;');
    return $x;
};
echo $fn();
echo ":" . $x;
"#,
    );
    assert_eq!(out, "5:1");
}

/// Verifies eval inside a closure writes through a by-reference capture to the source variable.
#[test]
fn test_eval_inside_closure_updates_by_ref_capture_source() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$fn = function() use (&$x) {
    eval('$x = $x + 4;');
};
$fn();
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies `global` inside eval can write compiler-known global storage.
#[test]
fn test_eval_global_alias_updates_global_storage() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function bump_eval_global() {
    global $g;
    eval('global $g; $g = $g + 1;');
}
bump_eval_global();
echo $g;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies a function can read a global alias after eval mutates that global.
#[test]
fn test_eval_global_alias_read_after_eval_in_same_function() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function bump_eval_global_and_read() {
    global $g;
    eval('global $g; $g = $g + 1;');
    echo $g;
}
bump_eval_global_and_read();
echo ":" . $g;
"#,
    );
    assert_eq!(out, "2:2");
}

/// Verifies unsetting an eval global alias does not unset the actual global value.
#[test]
fn test_eval_global_alias_unset_keeps_global_storage() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function unset_eval_global_alias() {
    global $g;
    eval('global $g; unset($g);');
}
unset_eval_global_alias();
echo $g;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies eval references to global aliases update the source global storage.
#[test]
fn test_eval_reference_alias_to_global_updates_global_storage() {
    let out = compile_and_run(
        r#"<?php
$g = 1;
function ref_eval_global_alias() {
    global $g;
    eval('$alias =& $g; $alias = 4;');
}
ref_eval_global_alias();
echo $g;
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies top-level eval fragments can read CLI `$argc` and `$argv`.
#[test]
fn test_eval_top_level_reads_argc_argv() {
    let out = compile_and_run(
        r#"<?php
eval('echo $argc . ":" . count($argv) . ":" . (strlen($argv[0]) > 0 ? "Y" : "N");');
"#,
    );
    assert_eq!(out, "1:1:Y");
}

/// Verifies top-level eval can replace `$argc` after the eval barrier widens it.
#[test]
fn test_eval_top_level_can_replace_argc_type() {
    let out = compile_and_run(
        r#"<?php
eval('$argc = "changed";');
echo $argc;
"#,
    );
    assert_eq!(out, "changed");
}

/// Verifies eval `global` aliases can read CLI argument globals inside functions.
#[test]
fn test_eval_global_alias_reads_argc_argv_in_function() {
    let out = compile_and_run(
        r#"<?php
function show_eval_process_args() {
    eval('global $argc, $argv; echo $argc . ":" . count($argv) . ":" . (strlen($argv[0]) > 0 ? "Y" : "N");');
}
show_eval_process_args();
"#,
    );
    assert_eq!(out, "1:1:Y");
}

/// Verifies functions declared by eval from a namespace are registered globally.
#[test]
fn test_eval_declared_function_in_namespace_is_global() {
    let out = compile_and_run(
        r#"<?php
namespace EvalNs;
eval('function dyn_eval_ns_global() { return 42; }');
echo function_exists('EvalNs\\dyn_eval_ns_global') ? '1' : '0';
echo ":";
echo function_exists('dyn_eval_ns_global') ? '1' : '0';
echo ":";
echo \dyn_eval_ns_global();
"#,
    );
    assert_eq!(out, "0:1:42");
}

/// Verifies namespace declarations inside eval qualify dynamic declarations and fall back to builtins.
#[test]
fn test_eval_fragment_namespace_declares_qualified_function() {
    let out = compile_and_run(
        r#"<?php
eval('namespace EvalInnerNs;
function dyn_eval_inner_ns() { return __NAMESPACE__ . ":" . __FUNCTION__; }
echo dyn_eval_inner_ns();
echo ":" . strlen("abcd");');
echo ":";
echo function_exists("EvalInnerNs\\dyn_eval_inner_ns") ? "Y" : "N";
echo ":";
echo call_user_func("EvalInnerNs\\dyn_eval_inner_ns");
"#,
    );
    assert_eq!(
        out,
        "EvalInnerNs:EvalInnerNs\\dyn_eval_inner_ns:4:Y:EvalInnerNs:EvalInnerNs\\dyn_eval_inner_ns"
    );
}

/// Verifies native calls can pass positional arguments to eval-declared functions.
#[test]
fn test_eval_declared_function_native_call_accepts_positional_args() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_native_add($x, $y) { return $x + $y; }');
echo dyn_eval_native_add(4, 5);
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies `call_user_func()` can dispatch to an eval-declared function after the barrier.
#[test]
fn test_eval_declared_function_can_be_called_with_call_user_func() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_cuf($x) { return $x + 1; }');
echo call_user_func('dyn_eval_cuf', 4);
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies post-barrier `call_user_func_array()` can dispatch to eval-declared functions.
#[test]
fn test_eval_declared_function_can_be_called_with_call_user_func_array() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_cufa($x, $y) { return ($x * 10) + $y; }');
echo call_user_func_array('dyn_eval_cufa', ['y' => 2, 'x' => 1]);
$args = ['y' => 3, 'x' => 2];
echo ":" . call_user_func_array('dyn_eval_cufa', $args);
"#,
    );
    assert_eq!(out, "12:23");
}

/// Verifies `call_user_func()` inside eval can dispatch to an eval-declared function.
#[test]
fn test_eval_fragment_call_user_func_dispatches_eval_declared_function() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_cuf($x) { return $x + 1; }
echo call_user_func("dyn_eval_inner_cuf", 4);');
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies `call_user_func()` inside eval can dispatch to supported builtins.
#[test]
fn test_eval_fragment_call_user_func_dispatches_builtin() {
    let out = compile_and_run(
        r#"<?php
eval('echo call_user_func("strlen", "abcd");
echo ":";
echo function_exists("call_user_func");');
"#,
    );
    assert_eq!(out, "4:1");
}

/// Verifies `call_user_func()` inside eval can dispatch to registered AOT functions.
#[test]
fn test_eval_fragment_call_user_func_dispatches_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_cuf_add($x, $y) { return $x + $y; }
eval('echo call_user_func("native_eval_cuf_add", 4, 6);');
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies variable call syntax inside eval dispatches supported builtin callables.
#[test]
fn test_eval_fragment_variable_callable_dispatches_builtin() {
    let out = compile_and_run(
        r#"<?php
eval('$fn = "strlen";
echo $fn("abcd") . ":";
$callbacks = ["strtoupper"];
echo $callbacks[0]("xy");');
"#,
    );
    assert_eq!(out, "4:XY");
}

/// Verifies variable call syntax inside eval dispatches eval-declared functions with named args.
#[test]
fn test_eval_fragment_variable_callable_dispatches_eval_declared_function() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_var_callable($x, $y) { return ($x * 10) + $y; }
$fn = "dyn_eval_var_callable";
echo $fn(y: 2, x: 1);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies variable call syntax inside eval dispatches registered AOT user functions.
#[test]
fn test_eval_fragment_variable_callable_dispatches_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_var_callable($left, $right) { return $left . ":" . $right; }
eval('$fn = "native_eval_var_callable";
echo $fn(right: "R", left: "L");');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies `call_user_func_array()` inside eval dispatches to eval-declared functions.
#[test]
fn test_eval_fragment_call_user_func_array_dispatches_eval_declared_function() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_cufa($x, $y) { return $x + $y; }
echo call_user_func_array("dyn_eval_inner_cufa", [4, 5]);');
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies `call_user_func_array()` inside eval binds eval-declared named arguments.
#[test]
fn test_eval_fragment_call_user_func_array_binds_eval_declared_named_args() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_cufa_named($x, $y) { return ($x * 10) + $y; }
echo call_user_func_array("dyn_eval_inner_cufa_named", ["y" => 2, "x" => 1]);');
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies `call_user_func_array()` inside eval dispatches to supported builtins.
#[test]
fn test_eval_fragment_call_user_func_array_dispatches_builtin() {
    let out = compile_and_run(
        r#"<?php
eval('echo call_user_func_array("strlen", ["abcd"]);
echo ":";
echo function_exists("call_user_func_array");');
"#,
    );
    assert_eq!(out, "4:1");
}

/// Verifies `call_user_func_array()` inside eval dispatches to registered AOT functions.
#[test]
fn test_eval_fragment_call_user_func_array_dispatches_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_cufa_add($x, $y) { return $x + $y; }
eval('echo call_user_func_array("native_eval_cufa_add", [4, 6]);');
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies `call_user_func_array()` inside eval binds registered AOT named arguments.
#[test]
fn test_eval_fragment_call_user_func_array_binds_native_user_function_named_args() {
    let out = compile_and_run(
        r#"<?php
function native_eval_cufa_named($left, $right) { return $left . ":" . $right; }
eval('echo call_user_func_array("native_eval_cufa_named", ["right" => "R", "left" => "L"]);');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies eval fragments can call AOT user functions registered in the eval context.
#[test]
fn test_eval_fragment_can_call_native_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_add($x, $y) { return $x + $y; }
eval('echo native_eval_add(4, 6); echo ":"; echo function_exists("native_eval_add");');
"#,
    );
    assert_eq!(out, "10:1");
}

/// Verifies eval fragments bind AOT user function parameters by name.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_named_args() {
    let out = compile_and_run(
        r#"<?php
function native_eval_named($left, $right) { return $left . ":" . $right; }
eval('echo native_eval_named(right: "R", left: "L");');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies eval fragments can unpack arrays into AOT user function calls.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_spread_args() {
    let out = compile_and_run(
        r#"<?php
function native_eval_spread($left, $right) { return $left . ":" . $right; }
eval('echo native_eval_spread(...["L", "R"]);');
"#,
    );
    assert_eq!(out, "L:R");
}

/// Verifies eval can dispatch AOT user functions with untyped by-reference params.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_mixed_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
function native_eval_ref_update(mixed &$value, string $suffix): string {
    $value = $value . $suffix;
    return "ret:" . $value;
}

echo eval('$value = "A";
$first = native_eval_ref_update($value, "B");
$fn = "native_eval_ref_update";
$fn($value, "C");
native_eval_ref_update(value: $value, suffix: "D");
return $first . ":" . $value;');
"#,
    );
    assert_eq!(out, "ret:AB:ABCD");
}

/// Verifies eval can dispatch AOT user functions with raw scalar by-reference params.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_scalar_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
function native_eval_ref_scalars(int &$i, bool &$b, float &$f): string {
    $i = $i + 2;
    $b = !$b;
    $f = $f + 0.5;
    return "done";
}

echo eval('$i = 3;
$b = true;
$f = 1.5;
$first = native_eval_ref_scalars($i, $b, $f);
$fn = "native_eval_ref_scalars";
$fn($i, $b, $f);
native_eval_ref_scalars(i: $i, b: $b, f: $f);
return $first . ":" . $i . ":" . ($b ? "T" : "F") . ":" . ($f == 3.0 ? "F3" : "bad");');
"#,
    );
    assert_eq!(out, "done:9:F:F3");
}

/// Verifies eval can dispatch AOT user functions with string by-reference params.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_string_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
function native_eval_ref_string(string &$value): string {
    $value = $value . "!";
    return "ret:" . $value;
}

echo eval('$value = "A";
$first = native_eval_ref_string($value);
$fn = "native_eval_ref_string";
$fn($value);
native_eval_ref_string(value: $value);
return $first . ":" . $value;');
"#,
    );
    assert_eq!(out, "ret:A!:A!!!");
}

/// Verifies eval can dispatch AOT user functions with one-word heap by-reference params.
#[test]
fn test_eval_fragment_can_call_native_user_function_with_heap_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
class NativeEvalRefBox {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }
}

function native_eval_ref_array_heap(array &$items): string {
    $items = ["A", "B"];
    return "array";
}

function native_eval_ref_iterable_heap(iterable &$items): string {
    $items = ["left" => "L", "right" => "R"];
    return "iter";
}

function native_eval_ref_object_heap(NativeEvalRefBox &$box): string {
    $box = new NativeEvalRefBox($box->name . "!");
    return $box->name;
}

echo eval('$items = [1];
$first = native_eval_ref_array_heap($items);
$fn = "native_eval_ref_array_heap";
$fn($items);
native_eval_ref_array_heap(items: $items);
$iter = [0];
$iterFirst = native_eval_ref_iterable_heap($iter);
$box = new NativeEvalRefBox("start");
$objectFirst = native_eval_ref_object_heap($box);
native_eval_ref_object_heap(box: $box);
return $first . ":" . $items[0] . ":" . $items[1] . ":" . $iterFirst . ":" . $iter["right"] . ":" . $objectFirst . ":" . $box->name;');
"#,
    );
    assert_eq!(out, "array:A:B:iter:R:start!:start!!");
}

/// Verifies eval can dispatch generated/AOT variadic functions through the native bridge.
#[test]
fn test_eval_fragment_can_call_native_variadic_user_function() {
    let out = compile_and_run(
        r#"<?php
function native_eval_variadic_collect(string $head, string ...$items): string {
    return $head . ":" . count($items) . ":" . $items[0] . ":" . $items[1];
}

function native_eval_variadic_default(string $head = "D", string ...$items): string {
    return $head . ":" . count($items) . ":" . (count($items) > 0 ? $items[0] : "-");
}

echo eval('$fn = "native_eval_variadic_collect";
return native_eval_variadic_collect("H", "A", "B") . "|"
    . native_eval_variadic_default(head: "N") . "|"
    . native_eval_variadic_default("P", "Q") . "|"
    . $fn("V", "X", "Y") . "|"
    . call_user_func("native_eval_variadic_collect", "C", "M", "N");');
"#,
    );
    assert_eq!(out, "H:2:A:B|N:0:-|P:1:Q|V:2:X:Y|C:2:M:N");
}

/// Verifies eval fragments called from methods can mutate public properties through `$this`.
#[test]
fn test_eval_fragment_can_mutate_this_public_property() {
    let out = compile_and_run(
        r#"<?php
class EvalPropBox {
    public int $x = 1;

    public function bump(): void {
        eval('$this->x = $this->x + 1;');
    }
}

$box = new EvalPropBox();
$box->bump();
echo $box->x;
"#,
    );
    assert_eq!(out, "2");
}

/// Verifies eval fragments inherit native method scope for private AOT property access.
#[test]
fn test_eval_fragment_can_mutate_this_private_property_from_declaring_method() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivatePropBox {
    private int $x = 1;

    public function run(): void {
        echo eval('$this->x = $this->x + 4; return $this->x;');
    }
}

$box = new EvalPrivatePropBox();
$box->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments outside the declaring scope cannot read private AOT properties.
#[test]
fn test_eval_fragment_rejects_private_property_outside_declaring_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivatePropOutsideBox {
    private int $x = 1;
}

$box = new EvalPrivatePropOutsideBox();
echo eval('try {
    return $box->x;
} catch (Error $e) {
    return get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot access private property EvalPrivatePropOutsideBox::$x"
    );
}

/// Verifies eval fragments can access inherited protected AOT properties from child scopes.
#[test]
fn test_eval_fragment_can_mutate_protected_aot_property_from_child_method() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedPropBase {
    protected int $x = 1;
}

class EvalProtectedPropChild extends EvalProtectedPropBase {
    public function run(): void {
        echo eval('$this->x = $this->x + 4; return $this->x;');
    }
}

$box = new EvalProtectedPropChild();
$box->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments allow parent scopes to access child-declared protected AOT properties.
#[test]
fn test_eval_fragment_can_mutate_child_protected_aot_property_from_parent_method() {
    let out = compile_and_run(
        r#"<?php
class EvalParentScopeProtectedPropBase {
    public function run(): void {
        echo eval('$this->x = $this->x + 4; return $this->x;');
    }
}

class EvalParentScopeProtectedPropChild extends EvalParentScopeProtectedPropBase {
    protected int $x = 1;
}

$box = new EvalParentScopeProtectedPropChild();
$box->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments reject protected AOT properties between sibling class scopes.
#[test]
fn test_eval_fragment_rejects_protected_aot_property_from_sibling_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedPropSiblingBase {}

class EvalProtectedPropLeft extends EvalProtectedPropSiblingBase {
    protected int $x = 1;
}

class EvalProtectedPropRight extends EvalProtectedPropSiblingBase {
    public function run(): void {
        echo eval('try {
            return (new EvalProtectedPropLeft())->x;
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }');
    }
}

$right = new EvalProtectedPropRight();
$right->run();
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot access protected property EvalProtectedPropLeft::$x"
    );
}

/// Verifies eval fragments can read and write public nullable-int AOT properties through `$this`.
#[test]
fn test_eval_fragment_can_mutate_this_nullable_int_property() {
    let out = compile_and_run(
        r#"<?php
class EvalNullableIntPropBox {
    public ?int $count = null;

    public function run(): void {
        echo eval('$out = ($this->count === null) ? "N" : "n";
            $this->count = 7;
            $out = $out . ":" . (($this->count === 7) ? "I7" : "bad");
            $this->count = "42";
            $out = $out . ":" . (($this->count === 42) ? "I42" : "bad");
            $this->count = null;
            return $out . ":" . (($this->count === null) ? "N" : "bad");');
    }
}

$box = new EvalNullableIntPropBox();
$box->run();
"#,
    );
    assert_eq!(out, "N:I7:I42:N");
}

/// Verifies eval fragments can read and write public nullable scalar AOT properties through `$this`.
#[test]
fn test_eval_fragment_can_mutate_this_nullable_scalar_properties() {
    let out = compile_and_run(
        r#"<?php
class EvalNullableScalarPropBox {
    public ?string $name = null;
    public ?bool $flag = null;
    public ?float $ratio = null;

    public function run(): void {
        echo eval('$out = ($this->name === null && $this->flag === null && $this->ratio === null) ? "N" : "bad";
            $this->name = "Ada";
            $this->flag = true;
            $this->ratio = 2.5;
            $out = $out . ":" . (($this->name === "Ada" && $this->flag === true && $this->ratio === 2.5) ? "set" : "bad");
            $this->name = null;
            $this->flag = null;
            $this->ratio = null;
            return $out . ":" . (($this->name === null && $this->flag === null && $this->ratio === null) ? "N" : "bad");');
    }
}

$box = new EvalNullableScalarPropBox();
$box->run();
"#,
    );
    assert_eq!(out, "N:set:N");
}

/// Verifies eval fragments can replace public array AOT properties through `$this`.
#[test]
fn test_eval_fragment_can_mutate_this_array_property() {
    let out = compile_and_run(
        r#"<?php
class EvalArrayPropBox {
    public array $items = [1];

    public function run(): void {
        echo eval('$this->items = [3, 4];
            return count($this->items) . ":" . $this->items[0] . ":" . $this->items[1];');
    }
}

$box = new EvalArrayPropBox();
$box->run();
"#,
    );
    assert_eq!(out, "2:3:4");
}

/// Verifies eval fragments can replace public object AOT properties through `$this`.
#[test]
fn test_eval_fragment_can_mutate_this_object_property() {
    let out = compile_and_run(
        r#"<?php
class EvalObjectPropValue {
    public int $n;

    public function __construct(int $n) {
        $this->n = $n;
    }
}

class EvalObjectPropBox {
    public EvalObjectPropValue $value;

    public function __construct() {
        $this->value = new EvalObjectPropValue(1);
    }

    public function run(): void {
        echo eval('$this->value = new EvalObjectPropValue(7);
            $value = $this->value;
            return $value->n;');
    }
}

$box = new EvalObjectPropBox();
$box->run();
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval fragments can read and write public nullable-int AOT static properties.
#[test]
fn test_eval_fragment_can_mutate_aot_nullable_int_static_property() {
    let out = compile_and_run(
        r#"<?php
class EvalNullableIntStaticPropBox {
    public static ?int $count = null;
}

echo eval('$out = (EvalNullableIntStaticPropBox::$count === null) ? "N" : "n";
    EvalNullableIntStaticPropBox::$count = 9;
    $out = $out . ":" . ((EvalNullableIntStaticPropBox::$count === 9) ? "I9" : "bad");
    EvalNullableIntStaticPropBox::$count = "33";
    $out = $out . ":" . ((EvalNullableIntStaticPropBox::$count === 33) ? "I33" : "bad");
    EvalNullableIntStaticPropBox::$count = null;
    return $out . ":" . ((EvalNullableIntStaticPropBox::$count === null) ? "N" : "bad");');
"#,
    );
    assert_eq!(out, "N:I9:I33:N");
}

/// Verifies eval fragments can read and write public nullable scalar AOT static properties.
#[test]
fn test_eval_fragment_can_mutate_aot_nullable_scalar_static_properties() {
    let out = compile_and_run(
        r#"<?php
class EvalNullableScalarStaticPropBox {
    public static ?string $name = null;
    public static ?bool $flag = null;
    public static ?float $ratio = null;
}

echo eval('$out = (EvalNullableScalarStaticPropBox::$name === null && EvalNullableScalarStaticPropBox::$flag === null && EvalNullableScalarStaticPropBox::$ratio === null) ? "N" : "bad";
    EvalNullableScalarStaticPropBox::$name = "Bea";
    EvalNullableScalarStaticPropBox::$flag = false;
    EvalNullableScalarStaticPropBox::$ratio = 3.5;
    $out = $out . ":" . ((EvalNullableScalarStaticPropBox::$name === "Bea" && EvalNullableScalarStaticPropBox::$flag === false && EvalNullableScalarStaticPropBox::$ratio === 3.5) ? "set" : "bad");
    EvalNullableScalarStaticPropBox::$name = null;
    EvalNullableScalarStaticPropBox::$flag = null;
    EvalNullableScalarStaticPropBox::$ratio = null;
    return $out . ":" . ((EvalNullableScalarStaticPropBox::$name === null && EvalNullableScalarStaticPropBox::$flag === null && EvalNullableScalarStaticPropBox::$ratio === null) ? "N" : "bad");');
"#,
    );
    assert_eq!(out, "N:set:N");
}

/// Verifies eval fragments inherit native class scope for private AOT static properties.
#[test]
fn test_eval_fragment_can_mutate_private_aot_static_property_from_declaring_method() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivateStaticPropBox {
    private static int $x = 1;

    public function run(): void {
        echo eval('self::$x = self::$x + 4; return self::$x;');
    }
}

$box = new EvalPrivateStaticPropBox();
$box->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments reject private AOT static properties outside the declaring scope.
#[test]
fn test_eval_fragment_rejects_private_aot_static_property_outside_declaring_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivateStaticPropBase {
    private static int $x = 1;
}

class EvalPrivateStaticPropChild extends EvalPrivateStaticPropBase {
    public function run(): void {
        echo eval('try {
            return EvalPrivateStaticPropBase::$x;
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }');
    }
}

$box = new EvalPrivateStaticPropChild();
$box->run();
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot access private property EvalPrivateStaticPropBase::$x"
    );
}

/// Verifies eval fragments can access inherited protected AOT static properties from child scopes.
#[test]
fn test_eval_fragment_can_mutate_protected_aot_static_property_from_child_method() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedStaticPropBase {
    protected static int $x = 1;
}

class EvalProtectedStaticPropChild extends EvalProtectedStaticPropBase {
    public function run(): void {
        echo eval('EvalProtectedStaticPropBase::$x = EvalProtectedStaticPropBase::$x + 4;
            return EvalProtectedStaticPropBase::$x;');
    }
}

$box = new EvalProtectedStaticPropChild();
$box->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments use AOT private parent static slots when child classes shadow them.
#[test]
fn test_eval_fragment_uses_aot_private_parent_static_property_shadowing_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalPrivateStaticShadowParent {
    private static $value = "p";
    public static function parentView() {
        return eval('return self::$value;');
    }
    public static function parentWrite() {
        return eval('self::$value = "P"; return self::$value;');
    }
}
class EvalPrivateStaticShadowChild extends EvalPrivateStaticShadowParent {
    public static $value = "c";
    public static function childView() {
        return eval('return self::$value;');
    }
}
eval('echo EvalPrivateStaticShadowChild::$value; echo "|";
echo EvalPrivateStaticShadowChild::childView(); echo "|";
echo EvalPrivateStaticShadowChild::parentView(); echo "|";
echo EvalPrivateStaticShadowChild::parentWrite(); echo "|";
echo EvalPrivateStaticShadowChild::$value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "c|c|p|P|c");
}

/// Verifies eval fragments allow parent scopes to access child-declared protected AOT static properties.
#[test]
fn test_eval_fragment_can_mutate_child_protected_aot_static_property_from_parent_method() {
    let out = compile_and_run(
        r#"<?php
class EvalParentScopeProtectedStaticPropBase {
    public function run(): void {
        echo eval('EvalParentScopeProtectedStaticPropChild::$x =
            EvalParentScopeProtectedStaticPropChild::$x + 4;
            return EvalParentScopeProtectedStaticPropChild::$x;');
    }
}

class EvalParentScopeProtectedStaticPropChild extends EvalParentScopeProtectedStaticPropBase {
    protected static int $x = 1;
}

$box = new EvalParentScopeProtectedStaticPropBase();
$box->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments reject protected AOT static properties between sibling class scopes.
#[test]
fn test_eval_fragment_rejects_protected_aot_static_property_from_sibling_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedStaticPropSiblingBase {}

class EvalProtectedStaticPropLeft extends EvalProtectedStaticPropSiblingBase {
    protected static int $x = 1;
}

class EvalProtectedStaticPropRight extends EvalProtectedStaticPropSiblingBase {
    public function run(): void {
        echo eval('try {
            return EvalProtectedStaticPropLeft::$x;
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }');
    }
}

$right = new EvalProtectedStaticPropRight();
$right->run();
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot access protected property EvalProtectedStaticPropLeft::$x"
    );
}

/// Verifies eval fragments throw Error for invalid AOT static property access.
#[test]
fn test_eval_fragment_invalid_aot_static_property_access_throws_error() {
    let out = compile_and_run(
        r#"<?php
class EvalInvalidAotStaticPropBox {
    public int $instance = 1;
    public static int $typed;
}

eval('try {
    EvalInvalidAotStaticPropBox::$missing;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalInvalidAotStaticPropBox::$instance;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalInvalidAotStaticPropBox::$typed;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalMissingAotStaticPropBox::$missing;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Access to undeclared static property EvalInvalidAotStaticPropBox::$missing|\
Error:Access to undeclared static property EvalInvalidAotStaticPropBox::$instance|\
Error:Typed static property EvalInvalidAotStaticPropBox::$typed must not be accessed before initialization|\
Error:Class \"EvalMissingAotStaticPropBox\" not found"
    );
}

/// Verifies eval fragments can replace public array AOT static properties.
#[test]
fn test_eval_fragment_can_mutate_aot_array_static_property() {
    let out = compile_and_run(
        r#"<?php
class EvalArrayStaticPropBox {
    public static array $items = [1];
}

echo eval('EvalArrayStaticPropBox::$items = [5, 6];
    return count(EvalArrayStaticPropBox::$items) . ":" .
        EvalArrayStaticPropBox::$items[0] . ":" . EvalArrayStaticPropBox::$items[1];');
"#,
    );
    assert_eq!(out, "2:5:6");
}

/// Verifies eval fragments can replace public object AOT static properties.
#[test]
fn test_eval_fragment_can_mutate_aot_object_static_property() {
    let out = compile_and_run(
        r#"<?php
class EvalObjectStaticPropValue {
    public int $n;

    public function __construct(int $n) {
        $this->n = $n;
    }
}

class EvalObjectStaticPropBox {
    public static EvalObjectStaticPropValue $value;
}

EvalObjectStaticPropBox::$value = new EvalObjectStaticPropValue(1);

echo eval('EvalObjectStaticPropBox::$value = new EvalObjectStaticPropValue(8);
    $value = EvalObjectStaticPropBox::$value;
    return $value->n;');
"#,
    );
    assert_eq!(out, "8");
}

/// Verifies eval keeps PHP property names case-sensitive while parsing keywords case-insensitively.
#[test]
fn test_eval_fragment_preserves_this_property_case() {
    let out = compile_and_run(
        r#"<?php
class EvalCasePropBox {
    public int $camelName = 42;

    public function read(): void {
        echo eval('RETURN $this->camelName;');
    }
}

$box = new EvalCasePropBox();
$box->read();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies eval fragments can call public zero-argument AOT methods through `$this`.
#[test]
fn test_eval_fragment_can_call_this_public_zero_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodBox {
    public int $x = 41;

    public function answer(): int {
        return $this->x + 1;
    }

    public function run(): void {
        echo eval('return $this->answer();');
    }
}

$box = new EvalMethodBox();
$box->run();
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies eval fragments can call private AOT instance methods from the declaring scope.
#[test]
fn test_eval_fragment_can_call_private_aot_method_from_declaring_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivateAotMethodBox {
    private function secret(int $n): int {
        return $n + 2;
    }

    public function run(): void {
        echo eval('return $this->secret(3);');
    }
}

(new EvalPrivateAotMethodBox())->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval callable arrays can call private AOT instance methods from the declaring scope.
#[test]
fn test_eval_fragment_callable_array_can_call_private_aot_method_from_declaring_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivateAotCallableMethodBox {
    private function secret(int $n): int {
        return $n + 2;
    }

    public function run(): void {
        echo eval('$cb = [$this, "secret"]; return call_user_func($cb, 3);');
    }
}

(new EvalPrivateAotCallableMethodBox())->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments reject private AOT instance methods from child scopes.
#[test]
fn test_eval_fragment_rejects_private_aot_method_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivateAotMethodBase {
    private function secret(int $n): int {
        return $n + 2;
    }
}

class EvalPrivateAotMethodChild extends EvalPrivateAotMethodBase {
    public function run(): void {
        echo eval('try {
            return $this->secret(3);
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }');
    }
}

(new EvalPrivateAotMethodChild())->run();
"#,
    );
    assert_eq!(
        out,
        "Error:Call to private method EvalPrivateAotMethodBase::secret() from scope EvalPrivateAotMethodChild"
    );
}

/// Verifies eval fragments dispatch private AOT parent methods on child receivers.
#[test]
fn test_eval_fragment_dispatches_aot_private_parent_method_on_child_receiver() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalPrivateAotParentReceiverBase {
    private function secret() { return "p"; }
    public function parentView() {
        return eval('return $this->secret();');
    }
}
class EvalPrivateAotParentReceiverChild extends EvalPrivateAotParentReceiverBase {}
$object = new EvalPrivateAotParentReceiverChild();
eval('echo $object->parentView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "p");
}

/// Verifies eval fragments dispatch AOT private parent methods shadowed by child methods.
#[test]
fn test_eval_fragment_dispatches_aot_private_parent_method_shadowing_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalPrivateAotMethodShadowParent {
    private function secret() { return "p"; }
    private static function staticSecret() { return "ps"; }
    public function parentView() {
        return eval('return $this->secret();');
    }
    public function parentCallback() {
        return eval('$cb = [$this, "secret"]; return call_user_func($cb);');
    }
    public static function parentStaticView() {
        return eval('return self::staticSecret();');
    }
}
class EvalPrivateAotMethodShadowChild extends EvalPrivateAotMethodShadowParent {
    public function secret() { return "c"; }
    public static function staticSecret() { return "cs"; }
    public function childView() {
        return eval('return $this->secret();');
    }
    public function childCallback() {
        return eval('$cb = [$this, "secret"]; return call_user_func($cb);');
    }
    public static function childStaticView() {
        return eval('return self::staticSecret();');
    }
}
$object = new EvalPrivateAotMethodShadowChild();
eval('echo $object->secret(); echo "|";
echo $object->childView(); echo "|";
echo $object->parentView(); echo "|";
echo $object->childCallback(); echo "|";
echo $object->parentCallback(); echo "|";
echo EvalPrivateAotMethodShadowChild::staticSecret(); echo "|";
echo EvalPrivateAotMethodShadowChild::childStaticView(); echo "|";
echo EvalPrivateAotMethodShadowChild::parentStaticView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "c|c|p|c|p|cs|cs|ps");
}

/// Verifies eval fragments can call inherited protected AOT methods from child scopes.
#[test]
fn test_eval_fragment_can_call_protected_aot_method_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedAotMethodBase {
    protected function add(int $n): int {
        return $n + 2;
    }
}

class EvalProtectedAotMethodChild extends EvalProtectedAotMethodBase {
    public function run(): void {
        echo eval('return $this->add(3);');
    }
}

(new EvalProtectedAotMethodChild())->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments reject protected AOT instance methods between sibling scopes.
#[test]
fn test_eval_fragment_rejects_protected_aot_method_from_sibling_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedAotMethodSiblingBase {}

class EvalProtectedAotMethodLeft extends EvalProtectedAotMethodSiblingBase {
    protected function add(int $n): int {
        return $n + 2;
    }
}

class EvalProtectedAotMethodRight extends EvalProtectedAotMethodSiblingBase {
    public function run(): void {
        echo eval('try {
            return (new EvalProtectedAotMethodLeft())->add(3);
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }');
    }
}

(new EvalProtectedAotMethodRight())->run();
"#,
    );
    assert_eq!(
        out,
        "Error:Call to protected method EvalProtectedAotMethodLeft::add() from scope EvalProtectedAotMethodRight"
    );
}

/// Verifies eval fragments pass one scalar argument to public AOT methods through `$this`.
#[test]
fn test_eval_fragment_can_call_this_public_one_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodArgBox {
    public int $x = 41;

    public function add(int $amount): int {
        return $this->x + $amount;
    }

    public function run(): void {
        echo eval('return $this->add(9);');
    }
}

$box = new EvalMethodArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "50");
}

/// Verifies eval fragments pass two scalar arguments to public AOT methods through `$this`.
#[test]
fn test_eval_fragment_can_call_this_public_two_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodTwoArgBox {
    public int $x = 41;

    public function label(int $amount, string $suffix): string {
        return ($this->x + $amount) . $suffix;
    }

    public function run(): void {
        echo eval('return $this->label(9, "!");');
    }
}

$box = new EvalMethodTwoArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "50!");
}

/// Verifies eval fragments pass more than two fixed scalar arguments to public AOT methods.
#[test]
fn test_eval_fragment_can_call_this_public_many_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodManyArgBox {
    public int $x = 10;

    public function label(int $a, int $b, int $c, string $suffix): string {
        return ($this->x + $a + $b + $c) . $suffix;
    }

    public function run(): void {
        echo eval('return $this->label(1, 2, 3, "!");');
    }
}

$box = new EvalMethodManyArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "16!");
}

/// Verifies eval fragments pass AOT method arguments that overflow onto the caller stack.
#[test]
fn test_eval_fragment_can_call_this_public_method_with_stack_string_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodStackStringArgBox {
    public function join4(string $a, string $b, string $c, string $d): string {
        return $a . $b . $c . $d;
    }

    public function run(): void {
        echo eval('return $this->join4("A", "B", "C", "D");');
    }
}

$box = new EvalMethodStackStringArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "ABCD");
}

/// Verifies eval fragments pass boxed Mixed values to public AOT methods.
#[test]
fn test_eval_fragment_can_call_this_public_mixed_arg_method() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodMixedArgBox {
    public function identity(mixed $value): mixed {
        return $value;
    }

    public function run(): void {
        echo eval('return $this->identity("mixed-ok");');
    }
}

$box = new EvalMethodMixedArgBox();
$box->run();
"#,
    );
    assert_eq!(out, "mixed-ok");
}

/// Verifies eval fragments can pass object-typed arguments to public AOT methods.
#[test]
fn test_eval_fragment_can_call_aot_method_with_object_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodObjectArgItem {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }
}

class EvalMethodObjectArgBox {
    public function describe(EvalMethodObjectArgItem $item): string {
        return $item->name;
    }

    public static function describeStatic(EvalMethodObjectArgItem $item): string {
        return $item->name . "!";
    }

    public function run() {
        $item = new EvalMethodObjectArgItem("Obj");
        return eval('return $this->describe($item) . ":" . EvalMethodObjectArgBox::describeStatic($item);');
    }
}

echo (new EvalMethodObjectArgBox())->run();
"#,
    );
    assert_eq!(out, "Obj:Obj!");
}

/// Verifies eval fragments can pass array-typed arguments to public AOT methods.
#[test]
fn test_eval_fragment_can_call_aot_method_with_array_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodArrayArgBox {
    public function countItems(array $items): int {
        return count($items);
    }

    public static function countStatic(array $items): int {
        return count($items);
    }

    public function run() {
        return eval('return $this->countItems([1, 2, 3]) . ":" . EvalMethodArrayArgBox::countStatic([4, 5]);');
    }
}

echo (new EvalMethodArrayArgBox())->run();
"#,
    );
    assert_eq!(out, "3:2");
}

/// Verifies eval fragments can pass iterable arguments to AOT methods and constructors.
#[test]
fn test_eval_fragment_dispatches_aot_iterable_parameters() {
    let out = compile_and_run(
        r#"<?php
class EvalAotIterableParamIterator implements Iterator {
    private int $i = 0;

    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
    public function current(): mixed { return "I" . $this->i; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i = $this->i + 1; }
}

class EvalAotIterableParamBox {
    public string $label;

    public function __construct(iterable $items) {
        $this->label = self::join($items);
    }

    public function describe(iterable $items): string {
        return self::join($items);
    }

    public static function describeStatic(iterable $items): string {
        return self::join($items);
    }

    private static function join(iterable $items): string {
        $out = "";
        foreach ($items as $item) {
            $out .= $item;
        }
        return $out;
    }
}

echo eval('$box = new EvalAotIterableParamBox(["C", "D"]);
$fromIterator = new EvalAotIterableParamBox(new EvalAotIterableParamIterator());
return $box->describe(["A", "B"]) . ":" . EvalAotIterableParamBox::describeStatic(new EvalAotIterableParamIterator()) . ":" . $box->label . ":" . $fromIterator->label;');
"#,
    );
    assert_eq!(out, "AB:I0I1:CD:I0I1");
}

/// Verifies eval fragments can pass nullable-int arguments to AOT methods and constructors.
#[test]
fn test_eval_fragment_dispatches_aot_nullable_int_parameters() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNullableIntParamBox {
    public string $label;

    public function __construct(?int $count = null) {
        $this->label = self::format($count);
    }

    public function describe(?int $count): string {
        return self::format($count);
    }

    public static function describeStatic(?int $count = null): string {
        return self::format($count);
    }

    private static function format(?int $count): string {
        return $count === null ? "N" : "I" . $count;
    }
}

echo eval('$defaulted = new EvalAotNullableIntParamBox();
$fromInt = new EvalAotNullableIntParamBox(7);
return $defaulted->label . ":" . $fromInt->label . ":" . $fromInt->describe(null) . ":" . $fromInt->describe("42") . ":" . EvalAotNullableIntParamBox::describeStatic() . ":" . EvalAotNullableIntParamBox::describeStatic(5);');
"#,
    );
    assert_eq!(out, "N:I7:N:I42:N:I5");
}

/// Verifies eval fragments can read nullable-int return values from AOT methods.
#[test]
fn test_eval_fragment_dispatches_aot_nullable_int_returns() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNullableIntReturnBox {
    public function maybe(bool $keep): ?int {
        return $keep ? 7 : null;
    }

    public static function maybeStatic(bool $keep): ?int {
        return $keep ? 11 : null;
    }

    public function run() {
        return eval('return ($this->maybe(true) === 7 ? "I7" : "bad") . ":" . (is_null($this->maybe(false)) ? "N" : "bad") . ":" . (EvalAotNullableIntReturnBox::maybeStatic(true) === 11 ? "S11" : "bad") . ":" . (is_null(EvalAotNullableIntReturnBox::maybeStatic(false)) ? "SN" : "bad");');
    }
}

echo (new EvalAotNullableIntReturnBox())->run();
"#,
    );
    assert_eq!(out, "I7:N:S11:SN");
}

/// Verifies eval fragments can pass nullable scalar arguments to AOT methods and constructors.
#[test]
fn test_eval_fragment_dispatches_aot_nullable_scalar_parameters() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNullableScalarParamBox {
    public string $label;

    public function __construct(?string $name = null, ?bool $flag = null, ?float $ratio = null) {
        $this->label = self::format($name, $flag, $ratio);
    }

    public function describe(?string $name, ?bool $flag, ?float $ratio): string {
        return self::format($name, $flag, $ratio);
    }

    public static function describeStatic(?string $name = null, ?bool $flag = null, ?float $ratio = null): string {
        return self::format($name, $flag, $ratio);
    }

    private static function format(?string $name, ?bool $flag, ?float $ratio): string {
        $namePart = $name === null ? "N" : (is_string($name) ? "S" . $name : "badName");
        $flagPart = $flag === null ? "N" : (is_bool($flag) ? ($flag ? "BT" : "BF") : "badFlag");
        $ratioPart = $ratio === null ? "N" : (is_float($ratio) ? "F" . $ratio : "badRatio");
        return $namePart . "/" . $flagPart . "/" . $ratioPart;
    }
}

echo eval('$defaulted = new EvalAotNullableScalarParamBox();
$filled = new EvalAotNullableScalarParamBox("Ada", true, 2.5);
return $defaulted->label . ":" . $filled->label . ":" . $filled->describe(null, false, 3.5) . ":" . EvalAotNullableScalarParamBox::describeStatic("Bea", true, 4.5);');
"#,
    );
    assert_eq!(out, "N/N/N:SAda/BT/F2.5:N/BF/F3.5:SBea/BT/F4.5");
}

/// Verifies eval fragments can read nullable scalar return values from AOT methods.
#[test]
fn test_eval_fragment_dispatches_aot_nullable_scalar_returns() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNullableScalarReturnBox {
    public function maybeName(bool $keep): ?string {
        return $keep ? "Ada" : null;
    }

    public function maybeFlag(bool $keep): ?bool {
        return $keep ? false : null;
    }

    public function maybeRatio(bool $keep): ?float {
        return $keep ? 1.5 : null;
    }

    public static function maybeStaticName(bool $keep): ?string {
        return $keep ? "Bea" : null;
    }

    public static function maybeStaticFlag(bool $keep): ?bool {
        return $keep ? true : null;
    }

    public static function maybeStaticRatio(bool $keep): ?float {
        return $keep ? 2.5 : null;
    }

    public function run() {
        return eval('return ($this->maybeName(true) === "Ada" ? "S" : "bad") . ":" . (is_null($this->maybeName(false)) ? "SN" : "bad") . ":" . ($this->maybeFlag(true) === false ? "BF" : "bad") . ":" . (is_null($this->maybeFlag(false)) ? "BN" : "bad") . ":" . ($this->maybeRatio(true) === 1.5 ? "F15" : "bad") . ":" . (is_null($this->maybeRatio(false)) ? "FN" : "bad") . ":" . (EvalAotNullableScalarReturnBox::maybeStaticName(true) === "Bea" ? "SS" : "bad") . ":" . (is_null(EvalAotNullableScalarReturnBox::maybeStaticName(false)) ? "SSN" : "bad") . ":" . (EvalAotNullableScalarReturnBox::maybeStaticFlag(true) === true ? "SBT" : "bad") . ":" . (is_null(EvalAotNullableScalarReturnBox::maybeStaticFlag(false)) ? "SBN" : "bad") . ":" . (EvalAotNullableScalarReturnBox::maybeStaticRatio(true) === 2.5 ? "SF25" : "bad") . ":" . (is_null(EvalAotNullableScalarReturnBox::maybeStaticRatio(false)) ? "SFN" : "bad");');
    }
}

echo (new EvalAotNullableScalarReturnBox())->run();
"#,
    );
    assert_eq!(out, "S:SN:BF:BN:F15:FN:SS:SSN:SBT:SBN:SF25:SFN");
}

/// Verifies eval dispatch uses inherited AOT metadata for complex signatures.
#[test]
fn test_eval_fragment_dispatches_inherited_aot_complex_signatures() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotComplexLeft {}
interface EvalAotComplexRight {}
class EvalAotComplexBoth implements EvalAotComplexLeft, EvalAotComplexRight {}

class EvalAotComplexParent {
    public function choose(int|string $value = "D", string $suffix = "S"): int|string {
        return is_int($value) ? $value + 10 : $value . $suffix;
    }

    public static function chooseStatic(int|string $value = "D", string $suffix = "S"): int|string {
        return is_int($value) ? $value + 10 : $value . $suffix;
    }

    public function both(EvalAotComplexLeft&EvalAotComplexRight $value): string {
        return "both";
    }
}

class EvalAotComplexChild extends EvalAotComplexParent {}

echo eval('$child = new EvalAotComplexChild();
return $child->choose(suffix: "X") . ":" . $child->choose(value: 2) . ":" . EvalAotComplexChild::chooseStatic(suffix: "Y") . ":" . EvalAotComplexChild::chooseStatic(value: 3) . ":" . $child->both(new EvalAotComplexBoth());');
"#,
    );
    assert_eq!(out, "DX:12:DY:13:both");
}

/// Verifies eval fragments can read iterable return values from AOT methods.
#[test]
fn test_eval_fragment_dispatches_aot_method_with_iterable_return() {
    let out = compile_and_run(
        r#"<?php
class EvalAotIterableReturnBox {
    public function items(): iterable {
        return [1, 2, 3];
    }

    public static function labels(): iterable {
        return ["left" => "L", "right" => "R"];
    }

    public function run() {
        return eval('$items = $this->items();
$labels = EvalAotIterableReturnBox::labels();
return is_iterable($items) . ":" . count($items) . ":" . $items[1] . ":" . is_iterable($labels) . ":" . $labels["right"];');
    }
}

echo (new EvalAotIterableReturnBox())->run();
"#,
    );
    assert_eq!(out, "1:3:2:1:R");
}

/// Verifies eval fragments inherit lexical `self::` from an AOT instance method.
#[test]
fn test_eval_fragment_in_aot_method_resolves_self_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalAotScopeSelfBox {
    public static function tag(): string {
        return "self";
    }

    public function run() {
        return eval('return self::class . ":" . self::tag();');
    }
}

echo (new EvalAotScopeSelfBox())->run();
"#,
    );
    assert_eq!(out, "EvalAotScopeSelfBox:self");
}

/// Verifies eval fragments inherit late-static `static::` from an AOT instance method.
#[test]
fn test_eval_fragment_in_aot_method_resolves_late_static_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalAotScopeStaticBase {
    public static function tag(): string {
        return "base";
    }

    public function run(): void {
        echo eval('return self::class . ":" . static::class . ":" . static::tag();');
    }
}

class EvalAotScopeStaticChild extends EvalAotScopeStaticBase {
    public static function tag(): string {
        return "child";
    }
}

(new EvalAotScopeStaticChild())->run();
"#,
    );
    assert_eq!(out, "EvalAotScopeStaticBase:EvalAotScopeStaticChild:child");
}

/// Verifies eval classes keep late-static scope inside inherited AOT eval fragments.
#[test]
fn test_eval_declared_child_inherited_aot_eval_fragment_preserves_late_static_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotEvalScopeParent {
    public function instanceProbe() {
        return eval('return self::class . ":" . static::class . ":" . get_called_class();');
    }

    public static function staticProbe() {
        return eval('return self::class . ":" . static::class . ":" . get_called_class();');
    }
}

eval('class EvalAotEvalScopeChild extends EvalAotEvalScopeParent {}
echo (new EvalAotEvalScopeChild())->instanceProbe() . "|";
echo EvalAotEvalScopeChild::staticProbe();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotEvalScopeParent:EvalAotEvalScopeChild:EvalAotEvalScopeChild|\
EvalAotEvalScopeParent:EvalAotEvalScopeChild:EvalAotEvalScopeChild"
    );
}

/// Verifies eval classes keep late-static `static::class` in inherited AOT methods.
#[test]
fn test_eval_declared_child_inherited_aot_method_static_class_preserves_late_static_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotDirectScopeParent {
    public function instanceClass() {
        return static::class;
    }

    public static function staticClass() {
        return static::class;
    }
}

eval('class EvalAotDirectScopeChild extends EvalAotDirectScopeParent {}
echo (new EvalAotDirectScopeChild())->instanceClass() . "|";
echo EvalAotDirectScopeChild::staticClass();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotDirectScopeChild|EvalAotDirectScopeChild"
    );
}

/// Verifies eval classes keep late-static `static::method()` in inherited AOT methods.
#[test]
fn test_eval_declared_child_inherited_aot_method_static_call_preserves_late_static_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotDirectStaticCallParent {
    public function instanceCall(): string {
        return static::tag();
    }

    public static function staticCall(): string {
        return static::tag();
    }

    public static function tag(): string {
        return "parent";
    }
}

eval('class EvalAotDirectStaticCallChild extends EvalAotDirectStaticCallParent {
    public static function tag(): string {
        return "child";
    }
}
echo (new EvalAotDirectStaticCallChild())->instanceCall() . "|";
echo EvalAotDirectStaticCallChild::staticCall();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "child|child");
}

/// Verifies eval classes keep late-static `static::$property` access in inherited AOT methods.
#[test]
fn test_eval_declared_child_inherited_aot_method_static_property_preserves_late_static_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotDirectStaticPropParent {
    public static string $value = "parent";

    public function instanceRead(): string {
        return static::$value;
    }

    public static function staticRead(): string {
        return static::$value;
    }

    public function instanceWrite(string $value): void {
        static::$value = $value;
    }

    public static function staticWrite(string $value): void {
        static::$value = $value;
    }
}

eval('class EvalAotDirectStaticPropChild extends EvalAotDirectStaticPropParent {
    public static string $value = "child";
}
echo (new EvalAotDirectStaticPropChild())->instanceRead() . "|";
echo EvalAotDirectStaticPropChild::staticRead() . "|";
(new EvalAotDirectStaticPropChild())->instanceWrite("one");
echo EvalAotDirectStaticPropChild::$value . ":" . EvalAotDirectStaticPropParent::$value . "|";
EvalAotDirectStaticPropChild::staticWrite("two");
echo EvalAotDirectStaticPropChild::$value . ":" . EvalAotDirectStaticPropParent::$value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "child|child|one:parent|two:parent");
}

/// Verifies eval fragments resolve `parent::` through AOT parent metadata.
#[test]
fn test_eval_fragment_in_aot_method_resolves_parent_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalAotScopeParentBase {
    public static function tag(): string {
        return "parent";
    }
}

class EvalAotScopeParentChild extends EvalAotScopeParentBase {
    public function run() {
        return eval('return parent::tag();');
    }
}

echo (new EvalAotScopeParentChild())->run();
"#,
    );
    assert_eq!(out, "parent");
}

/// Verifies eval fragments read generated/AOT class constants through inherited class scope.
#[test]
fn test_eval_fragment_reads_aot_class_constants() {
    let out = compile_and_run(
        r#"<?php
class EvalAotConstBase {
    public const PUBLIC_BASE = 4;
    protected const PROTECTED_BASE = "base";
}

class EvalAotConstChild extends EvalAotConstBase {
    private const SECRET = "secret";
    public const SUM = self::PUBLIC_BASE + 6;

    public function run() {
        return eval('return self::SECRET . ":" . parent::PROTECTED_BASE . ":" . static::SUM;');
    }
}

enum EvalAotConstState {
    case Ready;
}

echo (new EvalAotConstChild())->run();
echo ":";
echo eval('return EvalAotConstChild::PUBLIC_BASE . ":" . (EvalAotConstState::Ready === EvalAotConstState::Ready ? "case" : "bad");');
"#,
    );
    assert_eq!(out, "secret:base:10:4:case");
}

/// Verifies eval Reflection hides private AOT constants inherited from a parent class.
#[test]
fn test_eval_reflection_hides_private_inherited_aot_class_constants() {
    let out = compile_and_run(
        r#"<?php
class EvalAotPrivateConstBase {
    private const HIDDEN = "hidden";
    protected const VISIBLE = "visible";
}

class EvalAotPrivateConstChild extends EvalAotPrivateConstBase {}

echo eval('$ref = new ReflectionClass("EvalAotPrivateConstChild");
$private = $ref->getConstants(ReflectionClassConstant::IS_PRIVATE);
echo $ref->hasConstant("HIDDEN") ? "bad" : "ok";
echo ":" . count($private);
echo ":" . $ref->getConstant("VISIBLE");
return "";');
"#,
    );
    assert_eq!(out, "ok:0:visible");
}

/// Verifies eval rejects direct fetches of private AOT constants through child names.
#[test]
fn test_eval_rejects_private_inherited_aot_class_constant_fetch() {
    let out = compile_and_run(
        r#"<?php
class EvalAotPrivateConstFetchBase {
    private const HIDDEN = "hidden";
}

class EvalAotPrivateConstFetchChild extends EvalAotPrivateConstFetchBase {}

echo eval('try {
    return EvalAotPrivateConstFetchChild::HIDDEN;
} catch (Error $e) {
    return get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Undefined constant EvalAotPrivateConstFetchChild::HIDDEN"
    );
}

/// Verifies eval ReflectionClass exposes generated/AOT class constants and reflectors.
#[test]
fn test_eval_reflection_aot_class_constants() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotReflectConstIface {
    public const IFACE_LIMIT = 8;
}

class EvalAotReflectConstBase {
    public const BASE = 4;
    protected const LIMIT = 5;
}

class EvalAotReflectConstChild extends EvalAotReflectConstBase implements EvalAotReflectConstIface {
    private const SECRET = "s";
    final public const OWN = self::BASE + 6;
}

enum EvalAotReflectConstEnum {
    case Ready;
    public const LEVEL = 7;
}

echo eval('$ref = new ReflectionClass("EvalAotReflectConstChild");
echo $ref->hasConstant("OWN") ? "O" : "o";
echo $ref->hasConstant("BASE") ? "B" : "b";
echo $ref->hasConstant("IFACE_LIMIT") ? "I" : "i";
echo $ref->hasConstant("own") ? "bad" : "z";
$all = $ref->getConstants();
$private = $ref->getConstants(ReflectionClassConstant::IS_PRIVATE);
$final = $ref->getReflectionConstants(filter: ReflectionClassConstant::IS_FINAL);
$own = $ref->getReflectionConstant("OWN");
$direct = new ReflectionClassConstant("EvalAotReflectConstChild", "SECRET");
echo ":" . $ref->getConstant("OWN") . ":" . $ref->getConstant("SECRET") . ":" . $all["BASE"] . ":" . $all["IFACE_LIMIT"];
echo ":" . count($private) . ":" . $private["SECRET"];
echo ":" . count($final) . ":" . $own->getName() . ":" . $own->getValue() . ":" . ($own->isFinal() ? "F" : "f");
echo ":" . $direct->getDeclaringClass()->getName() . ":" . $direct->getValue();
$enum = new ReflectionClass("EvalAotReflectConstEnum");
$case = $enum->getReflectionConstant("Ready");
echo ":" . ($case->getValue() === EvalAotReflectConstEnum::Ready ? "case" : "bad") . ":" . $enum->getConstant("LEVEL");
return "";');
"#,
    );
    assert_eq!(
        out,
        "OBIz:10:s:4:8:1:s:1:OWN:10:F:EvalAotReflectConstChild:s:case:7"
    );
}

/// Verifies eval ReflectionClass materializes generated/AOT float constant arithmetic.
#[test]
fn test_eval_reflection_aot_float_constant_arithmetic() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectFloatConstTarget {
    public const BASE = 1.5;
    public const SUM = self::BASE + 2;
    public const DIFF = 5 - self::BASE;
    public const PRODUCT = self::BASE * 4;
    public const QUOTIENT = 7 / 2;
    public const POWER = self::BASE ** 2;
    public const NEG_POWER = 2 ** -1;
}

echo eval('try {
$ref = new ReflectionClass("EvalAotReflectFloatConstTarget");
$all = $ref->getConstants();
$power = $ref->getReflectionConstant("POWER");
$negativePower = new ReflectionClassConstant("EvalAotReflectFloatConstTarget", "NEG_POWER");
echo $ref->getConstant("SUM") . ":";
echo $ref->getConstant("DIFF") . ":";
echo $all["PRODUCT"] . ":";
echo $ref->getConstant("QUOTIENT") . ":";
echo $power->getValue() . ":";
echo $negativePower->getValue();
} catch (Throwable $e) {
    echo "ERR:" . get_class($e) . ":" . $e->getMessage();
}
return "";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "3.5:3.5:6:3.5:2.25:0.5");
}

/// Verifies eval fragments can unpack numeric arrays into public AOT method calls.
#[test]
fn test_eval_fragment_can_call_this_public_method_with_spread_args() {
    let out = compile_and_run(
        r#"<?php
class EvalMethodSpreadBox {
    public int $x = 41;

    public function label(int $amount, string $suffix): string {
        return ($this->x + $amount) . $suffix;
    }

    public function run(): void {
        echo eval('return $this->label(...[9, "!"]);');
    }
}

$box = new EvalMethodSpreadBox();
$box->run();
"#,
    );
    assert_eq!(out, "50!");
}

/// Verifies eval callable arrays dispatch public AOT methods through all dynamic call surfaces.
#[test]
fn test_eval_fragment_callable_array_dispatches_this_public_method() {
    let out = compile_and_run(
        r#"<?php
class EvalCallableArrayBox {
    public int $x = 40;

    public function label(int $amount, string $suffix): string {
        return ($this->x + $amount) . $suffix;
    }

    public function run(): void {
        echo eval('$cb = [$this, "label"];
echo $cb(1, "a");
echo ":";
echo call_user_func($cb, 2, "b");
echo ":";
return call_user_func_array($cb, [3, "c"]);');
    }
}

$box = new EvalCallableArrayBox();
$box->run();
"#,
    );
    assert_eq!(out, "41a:42b:43c");
}

/// Verifies eval callable arrays bind named arguments for generated/AOT methods.
#[test]
fn test_eval_fragment_callable_array_dispatches_aot_method_with_named_args() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCallableArrayNamedBox {
    public function join(string $left, string $right): string {
        return $left . $right;
    }

    public static function joinStatic(string $left, string $right): string {
        return $left . $right;
    }

    public function run() {
        return eval('$instance = [$this, "join"];
$static = [EvalAotCallableArrayNamedBox::class, "joinStatic"];
return $instance(right: "B", left: "A") . ":" .
    call_user_func_array($instance, ["right" => "D", "left" => "C"]) . ":" .
    call_user_func_array($static, ["right" => "F", "left" => "E"]);');
    }
}

echo (new EvalAotCallableArrayNamedBox())->run();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AB:CD:EF");
}

/// Verifies eval AOT callables preserve typed by-reference argument writeback.
#[test]
fn test_eval_fragment_aot_callables_write_back_typed_by_ref_args() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCallableRefBox {
    public int $offset = 0;

    public function __construct(int $offset = 0) {
        $this->offset = $offset;
    }

    public function bump(int &$value): int {
        $value = $value + $this->offset;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->offset + $delta;
        return $value;
    }
}

class EvalAotCallableRefDriver {
    public static function run(callable $callback, int &$value, int $delta) {
        return $callback($value, $delta);
    }
}

echo eval('$box = new EvalAotCallableRefBox(5);
$method = [$box, "bump"];
$value = "7";
echo $method($value) . ":" . gettype($value) . ":" . $value . "|";
$string = "EvalAotCallableRefBox::add";
$staticValue = "3";
echo $string($staticValue, 4) . ":" . gettype($staticValue) . ":" . $staticValue . "|";
$first = EvalAotCallableRefBox::add(...);
$next = "2";
echo $first($next, 6) . ":" . gettype($next) . ":" . $next . "|";
$instanceFirst = $box->bump(...);
$instanceValue = "4";
echo $instanceFirst($instanceValue) . ":" . gettype($instanceValue) . ":" . $instanceValue . "|";
$invokable = new EvalAotCallableRefBox(10);
$invokableValue = "1";
echo $invokable($invokableValue, 2) . ":" . gettype($invokableValue) . ":" . $invokableValue . "|";
$invokableFirst = $invokable(...);
$firstValue = "2";
echo $invokableFirst($firstValue, 3) . ":" . gettype($firstValue) . ":" . $firstValue . "|";
$bridge = new EvalAotCallableRefBox(20);
$bridgeFirst = $bridge(...);
$bridgeValue = "5";
return EvalAotCallableRefDriver::run($bridgeFirst, $bridgeValue, 1) . ":" . gettype($bridgeValue) . ":" . $bridgeValue;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "12:integer:12|7:integer:7|8:integer:8|9:integer:9|13:integer:13|15:integer:15|26:integer:26"
    );
}

/// Verifies eval AOT callable by-reference writeback updates property lvalues.
#[test]
fn test_eval_fragment_aot_callables_write_back_by_ref_property_lvalues() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCallableRefLvalueBox {
    public int $value = 1;
    public static int $staticValue = 2;
    public static int $dynStaticValue = 3;
    public int $offset = 0;

    public function __construct(int $offset = 0) {
        $this->offset = $offset;
    }

    public function bump(int &$value): int {
        $value = $value + $this->offset;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->offset + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotCallableRefLvalueBox(4);
$method = [$box, "bump"];
echo $method($box->value) . ":" . $box->value . "|";
$name = "value";
echo $method($box->{$name}) . ":" . $box->value . "|";
$string = "EvalAotCallableRefLvalueBox::add";
echo $string(EvalAotCallableRefLvalueBox::$staticValue, 5) . ":" . EvalAotCallableRefLvalueBox::$staticValue . "|";
$class = "EvalAotCallableRefLvalueBox";
$staticName = "dynStaticValue";
echo $string($class::${$staticName}, 6) . ":" . EvalAotCallableRefLvalueBox::$dynStaticValue . "|";
$first = EvalAotCallableRefLvalueBox::add(...);
echo $first($box->value, 3) . ":" . $box->value . "|";
$invokable = new EvalAotCallableRefLvalueBox(10);
echo $invokable($box->{$name}, 2) . ":" . $box->value . "|";
$invokableFirst = $invokable(...);
echo $invokableFirst(EvalAotCallableRefLvalueBox::$staticValue, 1) . ":" . EvalAotCallableRefLvalueBox::$staticValue;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "5:5|9:9|7:7|9:9|12:12|24:24|18:18");
}

/// Verifies eval `call_user_func_array()` preserves AOT callable by-reference writeback.
#[test]
fn test_eval_call_user_func_array_aot_callables_write_back_by_ref_args() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCallArrayRefBox {
    public int $offset = 0;

    public function __construct(int $offset = 0) {
        $this->offset = $offset;
    }

    public function bump(int &$value): int {
        $value = $value + $this->offset;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $this->offset + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotCallArrayRefBox(5);
$method = [$box, "bump"];
$value = "7";
echo call_user_func_array($method, [&$value]) . ":" . gettype($value) . ":" . $value . "|";
$string = "EvalAotCallArrayRefBox::add";
$staticValue = "3";
echo call_user_func_array($string, [&$staticValue, 4]) . ":" . gettype($staticValue) . ":" . $staticValue . "|";
$namedValue = "5";
echo call_user_func_array($string, ["delta" => 2, "value" => &$namedValue]) . ":" . gettype($namedValue) . ":" . $namedValue . "|";
$first = EvalAotCallArrayRefBox::add(...);
$next = "2";
echo call_user_func_array($first, [&$next, 6]) . ":" . gettype($next) . ":" . $next . "|";
$invokable = new EvalAotCallArrayRefBox(10);
$invokableValue = "1";
echo call_user_func_array($invokable, [&$invokableValue, 2]) . ":" . gettype($invokableValue) . ":" . $invokableValue . "|";
$invokableFirst = $invokable(...);
$firstValue = "2";
echo call_user_func_array($invokableFirst, [&$firstValue, 3]) . ":" . gettype($firstValue) . ":" . $firstValue;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "12:integer:12|7:integer:7|7:integer:7|8:integer:8|13:integer:13|15:integer:15"
    );
}

/// Verifies eval `call_user_func()` warns and passes eval-declared by-ref params by value.
#[test]
fn test_eval_call_user_func_eval_function_by_ref_args_warn_and_use_value_copy() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_cuf_ref_string(&$value) {
    $value .= "x";
    return $value;
}
$value = "a";
echo call_user_func("eval_cuf_ref_string", $value) . ":" . $value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "ax:a");
    assert!(
        out.stderr.contains(
            "eval_cuf_ref_string(): Argument #1 ($value) must be passed by reference, value given"
        ),
        "missing by-ref warning: {}",
        out.stderr
    );
}

/// Verifies eval `call_user_func()` warns and passes AOT by-ref params by value.
#[test]
fn test_eval_call_user_func_aot_function_by_ref_args_warn_and_use_value_copy() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_aot_cuf_ref_int(int &$value): int {
    $value = $value + 2;
    return $value;
}

eval('$value = 3;
echo call_user_func("eval_aot_cuf_ref_int", $value) . ":" . $value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "5:3");
    assert!(
        out.stderr.contains(
            "eval_aot_cuf_ref_int(): Argument #1 ($value) must be passed by reference, value given"
        ),
        "missing by-ref warning: {}",
        out.stderr
    );
}

/// Verifies eval `call_user_func()` degrades eval-declared method by-ref params to by-value.
#[test]
fn test_eval_call_user_func_eval_method_by_ref_args_warn_and_use_value_copy() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalCufMethodRefBox {
    public function append(&$value) {
        $value .= "x";
        return $value;
    }

    public static function add(&$value) {
        $value += 2;
        return $value;
    }

    public function __invoke(&$value) {
        $value .= "i";
        return $value;
    }
}

$box = new EvalCufMethodRefBox();
$value = "a";
echo call_user_func([$box, "append"], $value) . ":" . $value . "|";
$num = 3;
echo call_user_func(["EvalCufMethodRefBox", "add"], $num) . ":" . $num . "|";
$inv = "q";
echo call_user_func($box, $inv) . ":" . $inv . "|";
$first = $box->append(...);
$firstValue = "b";
echo call_user_func($first, $firstValue) . ":" . $firstValue . "|";
$staticFirst = EvalCufMethodRefBox::add(...);
$staticValue = 4;
echo call_user_func($staticFirst, $staticValue) . ":" . $staticValue;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "ax:a|5:3|qi:q|bx:b|6:4");
    for warning in [
        "EvalCufMethodRefBox::append(): Argument #1 ($value) must be passed by reference, value given",
        "EvalCufMethodRefBox::add(): Argument #1 ($value) must be passed by reference, value given",
        "EvalCufMethodRefBox::__invoke(): Argument #1 ($value) must be passed by reference, value given",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing by-ref warning {warning:?}: {}",
            out.stderr
        );
    }
}

/// Verifies eval `call_user_func()` degrades AOT method by-ref params to by-value.
#[test]
fn test_eval_call_user_func_aot_method_by_ref_args_warn_and_use_value_copy() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCufMethodRefBox {
    public function bump(int &$value): int {
        $value = $value + 2;
        return $value;
    }

    public static function add(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }

    public function __invoke(int &$value, int $delta): int {
        $value = $value + $delta;
        return $value;
    }
}

echo eval('$box = new EvalAotCufMethodRefBox();
$value = 3;
echo call_user_func([$box, "bump"], $value) . ":" . $value . "|";
$staticValue = 4;
echo call_user_func("EvalAotCufMethodRefBox::add", $staticValue, 3) . ":" . $staticValue . "|";
$arrayStaticValue = 8;
echo call_user_func(["EvalAotCufMethodRefBox", "add"], $arrayStaticValue, 2) . ":" . $arrayStaticValue . "|";
$first = $box->bump(...);
$firstValue = 5;
echo call_user_func($first, $firstValue) . ":" . $firstValue . "|";
$staticFirst = EvalAotCufMethodRefBox::add(...);
$staticFirstValue = 9;
echo call_user_func($staticFirst, $staticFirstValue, 1) . ":" . $staticFirstValue . "|";
$invokable = new EvalAotCufMethodRefBox();
$invokableValue = 6;
echo call_user_func($invokable, $invokableValue, 4) . ":" . $invokableValue;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "5:3|7:4|10:8|7:5|10:9|10:6");
    for warning in [
        "EvalAotCufMethodRefBox::bump(): Argument #1 ($value) must be passed by reference, value given",
        "EvalAotCufMethodRefBox::add(): Argument #1 ($value) must be passed by reference, value given",
        "EvalAotCufMethodRefBox::__invoke(): Argument #1 ($value) must be passed by reference, value given",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing by-ref warning {warning:?}: {}",
            out.stderr
        );
    }
}

/// Verifies eval `ReflectionClass::newInstanceArgs()` preserves AOT constructor by-reference writeback.
#[test]
fn test_eval_reflection_new_instance_args_aot_constructor_writes_back_by_ref_args() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectCtorArrayRefBox {
    public int $seen = 0;

    public function __construct(int &$value, int $delta = 0) {
        $value = $value + $delta;
        $this->seen = $value;
    }
}

echo eval('$ref = new ReflectionClass("EvalAotReflectCtorArrayRefBox");
$value = "3";
$box = $ref->newInstanceArgs([&$value, 4]);
echo $box->seen . ":" . gettype($value) . ":" . $value . "|";
$named = "5";
$box = $ref->newInstanceArgs(["delta" => 2, "value" => &$named]);
echo $box->seen . ":" . gettype($named) . ":" . $named;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:integer:7|7:integer:7");
}

/// Verifies eval can pass runtime PHP callbacks to generated/AOT callable-typed methods.
#[test]
fn test_eval_fragment_passes_callable_args_to_aot_methods() {
    let out = compile_and_run_capture(
        r##"<?php
function eval_aot_callable_arg_suffix(string $value): string {
    return $value . "!";
}

class EvalAotCallableArgTarget {
    public static function suffix(string $value): string {
        return $value . "?";
    }

    public function instanceSuffix(string $value): string {
        return $value . "~";
    }

    public function __invoke(string $value): string {
        return $value . "#";
    }
}

class EvalAotCallableArgBox {
    public $value = "";

    public function __construct(callable $callback) {
        $this->value = $callback("C");
    }

    public function apply(callable $callback) {
        return $callback("M");
    }

    public static function applyStatic(callable $callback) {
        return $callback("S");
    }
}

echo eval('$box = new EvalAotCallableArgBox("eval_aot_callable_arg_suffix");
return $box->value . ":" .
    $box->apply("eval_aot_callable_arg_suffix") . ":" .
    EvalAotCallableArgBox::applyStatic("eval_aot_callable_arg_suffix");');
echo ":";
echo eval('$static = [EvalAotCallableArgTarget::class, "suffix"];
$box = new EvalAotCallableArgBox($static);
return $box->value . ":" .
    $box->apply("EvalAotCallableArgTarget::suffix") . ":" .
    EvalAotCallableArgBox::applyStatic($static);');
echo ":";
echo eval('$target = new EvalAotCallableArgTarget();
$instance = [$target, "instanceSuffix"];
$box = new EvalAotCallableArgBox($instance);
return $box->value . ":" .
    $box->apply($instance) . ":" .
    EvalAotCallableArgBox::applyStatic($instance);');
echo ":";
echo eval('$invokable = new EvalAotCallableArgTarget();
$box = new EvalAotCallableArgBox($invokable);
return $box->value . ":" .
    $box->apply($invokable) . ":" .
    EvalAotCallableArgBox::applyStatic($invokable);');
echo ":";
echo eval('$function = eval_aot_callable_arg_suffix(...);
$box = new EvalAotCallableArgBox($function);
return $box->value . ":" .
    $box->apply($function) . ":" .
    EvalAotCallableArgBox::applyStatic($function);');
echo ":";
echo eval('$static = EvalAotCallableArgTarget::suffix(...);
$box = new EvalAotCallableArgBox($static);
return $box->value . ":" .
    $box->apply($static) . ":" .
    EvalAotCallableArgBox::applyStatic($static);');
echo ":";
echo eval('$target = new EvalAotCallableArgTarget();
$instance = $target->instanceSuffix(...);
$box = new EvalAotCallableArgBox($instance);
return $box->value . ":" .
    $box->apply($instance) . ":" .
    EvalAotCallableArgBox::applyStatic($instance);');
echo ":";
echo eval('$target = new EvalAotCallableArgTarget();
$invokable = $target(...);
$box = new EvalAotCallableArgBox($invokable);
return $box->value . ":" .
    $box->apply($invokable) . ":" .
    EvalAotCallableArgBox::applyStatic($invokable);');
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "C!:M!:S!:C?:M?:S?:C~:M~:S~:C#:M#:S#:C!:M!:S!:C?:M?:S?:C~:M~:S~:C#:M#:S#"
    );
}

/// Verifies eval static calls and static callables dispatch public AOT static methods.
#[test]
fn test_eval_fragment_dispatches_aot_static_methods() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticBox {
    public static function join(string $left, string $right): string {
        return $left . $right;
    }

    public static function sum4(int $a, int $b, int $c, int $d): int {
        return $a + $b + $c + $d;
    }

    public static function sum(int $left, int $right): int {
        return $left + $right;
    }
}

eval('echo EvalAotStaticBox::join("A", "B"); echo ":";
$cb = ["EvalAotStaticBox", "join"];
echo call_user_func($cb, "C", "D"); echo ":";
$named = "EvalAotStaticBox::join";
echo $named("E", "F"); echo ":";
echo call_user_func_array(["EvalAotStaticBox", "sum"], [2, 5]); echo ":";
echo EvalAotStaticBox::sum4(1, 2, 3, 4);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AB:CD:EF:7:10");
}

/// Verifies eval reports PHP's catchable Error for static syntax on non-static AOT methods.
#[test]
fn test_eval_fragment_rejects_aot_non_static_method_called_statically() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNonStaticStaticSyntaxBox {
    public function run(): string {
        return "bad";
    }
}

try {
    eval('EvalAotNonStaticStaticSyntaxBox::run();');
} catch (Error $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Non-static method EvalAotNonStaticStaticSyntaxBox::run() cannot be called statically"
    );
}

/// Verifies eval fragments can call private AOT static methods from the declaring scope.
#[test]
fn test_eval_fragment_can_call_private_aot_static_method_from_declaring_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivateAotStaticMethodBox {
    private static function secret(int $n): int {
        return $n + 2;
    }

    public function run(): void {
        echo eval('return self::secret(3);');
    }
}

(new EvalPrivateAotStaticMethodBox())->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments reject private AOT static methods from child scopes.
#[test]
fn test_eval_fragment_rejects_private_aot_static_method_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalPrivateAotStaticMethodBase {
    private static function secret(int $n): int {
        return $n + 2;
    }
}

class EvalPrivateAotStaticMethodChild extends EvalPrivateAotStaticMethodBase {
    public function run(): void {
        echo eval('try {
            return EvalPrivateAotStaticMethodBase::secret(3);
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }');
    }
}

(new EvalPrivateAotStaticMethodChild())->run();
"#,
    );
    assert_eq!(
        out,
        "Error:Call to private method EvalPrivateAotStaticMethodBase::secret() from scope EvalPrivateAotStaticMethodChild"
    );
}

/// Verifies eval fragments can call inherited protected AOT static methods from child scopes.
#[test]
fn test_eval_fragment_can_call_protected_aot_static_method_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedAotStaticMethodBase {
    protected static function add(int $n): int {
        return $n + 2;
    }
}

class EvalProtectedAotStaticMethodChild extends EvalProtectedAotStaticMethodBase {
    public function run(): void {
        echo eval('return EvalProtectedAotStaticMethodBase::add(3);');
    }
}

(new EvalProtectedAotStaticMethodChild())->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval static method callables can call protected AOT methods from child scopes.
#[test]
fn test_eval_fragment_callable_can_call_protected_aot_static_method_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedAotStaticCallableBase {
    protected static function add(int $n): int {
        return $n + 2;
    }
}

class EvalProtectedAotStaticCallableChild extends EvalProtectedAotStaticCallableBase {
    public function run(): void {
        echo eval('return call_user_func([EvalProtectedAotStaticCallableBase::class, "add"], 3);');
    }
}

(new EvalProtectedAotStaticCallableChild())->run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval fragments reject protected AOT static methods between sibling scopes.
#[test]
fn test_eval_fragment_rejects_protected_aot_static_method_from_sibling_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalProtectedAotStaticMethodSiblingBase {}

class EvalProtectedAotStaticMethodLeft extends EvalProtectedAotStaticMethodSiblingBase {
    protected static function add(int $n): int {
        return $n + 2;
    }
}

class EvalProtectedAotStaticMethodRight extends EvalProtectedAotStaticMethodSiblingBase {
    public function run(): void {
        echo eval('try {
            return EvalProtectedAotStaticMethodLeft::add(3);
        } catch (Error $e) {
            return get_class($e) . ":" . $e->getMessage();
        }');
    }
}

(new EvalProtectedAotStaticMethodRight())->run();
"#,
    );
    assert_eq!(
        out,
        "Error:Call to protected method EvalProtectedAotStaticMethodLeft::add() from scope EvalProtectedAotStaticMethodRight"
    );
}

/// Verifies eval static dispatch passes AOT static method arguments on the caller stack.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_stack_string_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotStaticStackStringBox {
    public static function join4(string $a, string $b, string $c, string $d): string {
        return $a . $b . $c . $d;
    }
}

eval('echo EvalAotStaticStackStringBox::join4("G", "H", "I", "J");');
"#,
    );
    assert_eq!(out, "GHIJ");
}

/// Verifies eval binds named arguments before dispatching an AOT instance method.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_named_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNamedMethodBox {
    public function run() {
        return eval('return $this->join(right: "B", left: "A");');
    }

    public function join(string $left, string $right): string {
        return $left . $right;
    }
}

echo (new EvalAotNamedMethodBox())->run();
"#,
    );
    assert_eq!(out, "AB");
}

/// Verifies eval dispatches generated/AOT instance methods with untyped by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_mixed_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotMixedRefMethodBox {
    public function mutate(mixed &$value): void {
        $value = $value + 5;
    }
}

echo eval('$box = new EvalAotMixedRefMethodBox();
$value = 10;
$box->mutate($value);
return $value;');
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies eval dispatches generated/AOT instance methods with typed scalar by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_typed_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotTypedRefMethodBox {
    public function mutate(int &$value, bool &$flag): void {
        $value = $value + 5;
        $flag = !$flag;
    }
}

echo eval('$box = new EvalAotTypedRefMethodBox();
$value = 10;
$flag = true;
$box->mutate($value, $flag);
return $value . ":" . ($flag ? "T" : "F");');
"#,
    );
    assert_eq!(out, "15:F");
}

/// Verifies eval writes nullable-int by-reference AOT method results back as boxed eval values.
#[test]
fn test_eval_fragment_dispatches_aot_nullable_int_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNullableRefMethodBox {
    public function clear(?int &$value): void {
        $value = null;
    }
}

echo eval('$box = new EvalAotNullableRefMethodBox();
$value = 12;
$box->clear($value);
return $value === null ? "N" : "bad";');
"#,
    );
    assert_eq!(out, "N");
}

/// Verifies eval writes nullable scalar by-reference AOT method results back to eval variables.
#[test]
fn test_eval_fragment_dispatches_aot_nullable_scalar_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNullableScalarRefMethodBox {
    public function mutate(?string &$name, ?bool &$flag, ?float &$ratio): void {
        $name = $name === null ? "method" : "eval-method";
        $flag = true;
        $ratio = $ratio === null ? 1.5 : 3.0;
    }
}

echo eval('$box = new EvalAotNullableScalarRefMethodBox();
$name = "eval";
$flag = false;
$ratio = 2.5;
$box->mutate($name, $flag, $ratio);
$first = $name . ":" . ($flag ? "T" : "F") . ":" . $ratio;
$name = null;
$flag = null;
$ratio = null;
$box->mutate($name, $flag, $ratio);
return $first . ":" . $name . ":" . ($flag ? "T" : "F") . ":" . $ratio;');
"#,
    );
    assert_eq!(out, "eval-method:T:3:method:T:1.5");
}

/// Verifies eval dispatches generated/AOT instance methods with string by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_string_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotStringRefMethodBox {
    public function mutate(string &$value): void {
        $value = $value . "-method";
    }
}

echo eval('$box = new EvalAotStringRefMethodBox();
$value = "eval";
$box->mutate($value);
return $value;');
"#,
    );
    assert_eq!(out, "eval-method");
}

/// Verifies eval dispatches generated/AOT instance methods with array by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_array_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotArrayRefMethodBox {
    public function append(array &$items): void {
        $items[] = 3;
    }

    public function replace(array &$items): void {
        $items = [4, 5];
    }
}

echo eval('$box = new EvalAotArrayRefMethodBox();
$items = [1, 2];
$box->append($items);
$afterAppend = count($items) . ":" . $items[2];
$box->replace($items);
return $afterAppend . ":" . count($items) . ":" . $items[0] . ":" . $items[1];');
"#,
    );
    assert_eq!(out, "3:3:2:4:5");
}

/// Verifies eval dispatches generated/AOT instance methods with object by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_object_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotObjectRefMethodPayload {
    public int $value = 1;
}

class EvalAotObjectRefMethodBox {
    public function mutate(EvalAotObjectRefMethodPayload &$payload): void {
        $payload->value = 7;
    }

    public function replace(EvalAotObjectRefMethodPayload &$payload): void {
        $payload = new EvalAotObjectRefMethodPayload();
        $payload->value = 9;
    }
}

echo eval('$box = new EvalAotObjectRefMethodBox();
$payload = new EvalAotObjectRefMethodPayload();
$box->mutate($payload);
$afterMutate = $payload->value;
$box->replace($payload);
return $afterMutate . ":" . $payload->value;');
"#,
    );
    assert_eq!(out, "7:9");
}

/// Verifies eval dispatches generated/AOT instance methods with iterable by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_iterable_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotIterableRefMethodBox {
    public function replace(iterable &$items): void {
        $items = [4, 5];
    }
}

echo eval('$box = new EvalAotIterableRefMethodBox();
$items = [1, 2];
$box->replace($items);
return is_iterable($items) . ":" . count($items) . ":" . $items[0] . ":" . $items[1];');
"#,
    );
    assert_eq!(out, "1:2:4:5");
}

/// Verifies eval preserves string values passed through an untyped AOT method parameter.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_mixed_string_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotMixedStringMethodBox {
    public function relay($value) {
        return $value;
    }
}

echo eval('$obj = new EvalAotMixedStringMethodBox();
$value = $obj->relay("abc");
return gettype($value) . ":" . $value;');
"#,
    );
    assert_eq!(out, "string:abc");
}

/// Verifies eval preserves array values passed through an untyped AOT method parameter.
#[test]
fn test_eval_fragment_dispatches_aot_instance_method_with_mixed_array_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotMixedArrayMethodBox {
    public function relay($value) {
        return $value;
    }
}

echo eval('$obj = new EvalAotMixedArrayMethodBox();
$value = $obj->relay([]);
return gettype($value) . ":" . (is_array($value) ? count($value) : 9);');
"#,
    );
    assert_eq!(out, "array:0");
}

/// Verifies eval binds named arguments before dispatching an AOT static method.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_named_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNamedStaticBox {
    public static function join(string $left, string $right): string {
        return $left . $right;
    }
}

eval('echo EvalAotNamedStaticBox::join(right: "D", left: "C");');
"#,
    );
    assert_eq!(out, "CD");
}

/// Verifies eval dispatches generated/AOT static methods with untyped by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_mixed_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotMixedRefStaticBox {
    public static function mutate(mixed &$value): void {
        $value = $value + 7;
    }
}

echo eval('$value = 20;
EvalAotMixedRefStaticBox::mutate($value);
return $value;');
"#,
    );
    assert_eq!(out, "27");
}

/// Verifies eval writes nullable scalar by-reference AOT static method results back to eval variables.
#[test]
fn test_eval_fragment_dispatches_aot_nullable_scalar_static_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNullableScalarRefStaticBox {
    public static function mutate(?string &$name, ?bool &$flag, ?float &$ratio): void {
        $name = $name === null ? "static" : "eval-static";
        $flag = true;
        $ratio = $ratio === null ? 1.25 : 2.75;
    }
}

echo eval('$name = "eval";
$flag = false;
$ratio = 2.5;
EvalAotNullableScalarRefStaticBox::mutate($name, $flag, $ratio);
$first = $name . ":" . ($flag ? "T" : "F") . ":" . $ratio;
$name = null;
$flag = null;
$ratio = null;
EvalAotNullableScalarRefStaticBox::mutate($name, $flag, $ratio);
return $first . ":" . $name . ":" . ($flag ? "T" : "F") . ":" . $ratio;');
"#,
    );
    assert_eq!(out, "eval-static:T:2.75:static:T:1.25");
}

/// Verifies eval dispatches generated/AOT static methods with float by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_float_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotFloatRefStaticBox {
    public static function mutate(float &$value): void {
        $value = $value + 0.25;
    }
}

echo eval('$value = 2.5;
EvalAotFloatRefStaticBox::mutate($value);
return $value;');
"#,
    );
    assert_eq!(out, "2.75");
}

/// Verifies eval dispatches generated/AOT static methods with string by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_string_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotStringRefStaticBox {
    public static function mutate(string &$value): void {
        $value = "static-" . $value;
    }
}

echo eval('$value = "eval";
EvalAotStringRefStaticBox::mutate($value);
return $value;');
"#,
    );
    assert_eq!(out, "static-eval");
}

/// Verifies eval dispatches generated/AOT static methods with array by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_array_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotArrayRefStaticBox {
    public static function mutate(array &$items): void {
        $items[] = 8;
    }
}

echo eval('$items = [6, 7];
EvalAotArrayRefStaticBox::mutate($items);
return count($items) . ":" . $items[2];');
"#,
    );
    assert_eq!(out, "3:8");
}

/// Verifies eval dispatches generated/AOT static methods with object by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_object_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotObjectRefStaticPayload {
    public int $value = 2;
}

class EvalAotObjectRefStaticBox {
    public static function mutate(EvalAotObjectRefStaticPayload &$payload): void {
        $payload->value = 8;
    }
}

echo eval('$payload = new EvalAotObjectRefStaticPayload();
EvalAotObjectRefStaticBox::mutate($payload);
return $payload->value;');
"#,
    );
    assert_eq!(out, "8");
}

/// Verifies eval dispatches generated/AOT static methods with iterable by-reference params.
#[test]
fn test_eval_fragment_dispatches_aot_static_method_with_iterable_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalAotIterableRefStaticBox {
    public static function replace(iterable &$items): void {
        $items = ["name" => "static"];
    }
}

echo eval('$items = [6, 7];
EvalAotIterableRefStaticBox::replace($items);
return is_iterable($items) . ":" . $items["name"];');
"#,
    );
    assert_eq!(out, "1:static");
}

/// Verifies eval binds named arguments before dispatching an AOT constructor.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_named_args() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewNamedCtor {
    public string $label = "";
    public function __construct(string $left, string $right) {
        $this->label = $left . $right;
    }
}

echo eval('$box = new EvalDynamicNewNamedCtor(right: "F", left: "E"); return $box->label;');
"#,
    );
    assert_eq!(out, "EF");
}

/// Verifies eval object construction accepts runtime class-name variables.
#[test]
fn test_eval_dynamic_new_accepts_runtime_class_name() {
    let out = compile_and_run(
        r#"<?php
class EvalAotRuntimeNewTarget {
    public string $label;
    public function __construct(string $label) {
        $this->label = "aot:" . $label;
    }
}

eval('class EvalRuntimeNewTarget {
    public string $label;
    public function __construct(string $label) {
        $this->label = "eval:" . $label;
    }
}

$evalClass = "EvalRuntimeNewTarget";
$evalBox = new $evalClass("Ada");
echo $evalBox->label; echo "|";

$aotClass = "EvalAotRuntimeNewTarget";
$aotBox = new $aotClass("Turing");
echo $aotBox->label; echo "|";

$prototype = new EvalRuntimeNewTarget("proto");
$copy = new $prototype("Grace");
echo $copy->label;');
"#,
    );
    assert_eq!(out, "eval:Ada|aot:Turing|eval:Grace");
}

/// Verifies eval object construction accepts parenthesized class expressions and optional constructor parentheses.
#[test]
fn test_eval_dynamic_new_accepts_expression_class_name() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalExpressionNewTarget {
    public $label;

    public function __construct($label = "default") {
        $this->label = $label;
    }
}

function eval_expression_new_target() {
    return "EvalExpressionNewTarget";
}

$direct = new (eval_expression_new_target())("Ada");
$class = "EvalExpressionNewTarget";
$withoutDynamicParens = new $class;
$withoutNamedParens = new EvalExpressionNewTarget;
return $direct->label . "|" . $withoutDynamicParens->label . "|" . $withoutNamedParens->label;');
"#,
    );
    assert_eq!(out, "Ada|default|default");
}

/// Verifies eval object construction passes object-typed arguments to AOT constructors.
#[test]
fn test_eval_dynamic_new_passes_object_arg_to_constructor() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewObjectArgSource {
    public string $name;
    public function __construct(string $name) {
        $this->name = $name;
    }
}

class EvalDynamicNewObjectArgTarget {
    public string $label = "";
    public function __construct(EvalDynamicNewObjectArgSource $source) {
        $this->label = $source->name;
    }
}

echo eval('$source = new EvalDynamicNewObjectArgSource("Ada");
$box = new EvalDynamicNewObjectArgTarget($source);
return $box->label;');
"#,
    );
    assert_eq!(out, "Ada");
}

/// Verifies eval object construction passes array-typed arguments to AOT constructors.
#[test]
fn test_eval_dynamic_new_passes_array_arg_to_constructor() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewArrayArgTarget {
    public int $count = 0;
    public function __construct(array $items) {
        $this->count = count($items);
    }
}

echo eval('$box = new EvalDynamicNewArrayArgTarget([1, 2, 3, 4]);
return $box->count;');
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies eval-declared methods resolve `new self/static/parent` through the bridge.
#[test]
fn test_eval_declared_methods_construct_relative_class_names() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalRelativeFactoryBase {
    public string $label;
    public function __construct($label = "base") { $this->label = $label; }
    public function selfFactory() { return new self("self"); }
    public function staticFactory() { return new static("static"); }
}
class EvalRelativeFactoryChild extends EvalRelativeFactoryBase {
    public function parentFactory() { return new parent("parent"); }
}
$child = new EvalRelativeFactoryChild("root");
$self = $child->selfFactory();
$static = $child->staticFactory();
$parent = $child->parentFactory();
echo get_class($self); echo ":"; echo $self->label; echo ":";
echo get_class($static); echo ":"; echo $static->label; echo ":";
echo get_class($parent); echo ":"; echo $parent->label;');
"#,
    );
    assert_eq!(
        out,
        "EvalRelativeFactoryBase:self:EvalRelativeFactoryChild:static:EvalRelativeFactoryBase:parent"
    );
}

/// Verifies eval-declared methods resolve `new self/static/parent` inside namespaces.
#[test]
fn test_eval_declared_methods_construct_namespaced_relative_class_names() {
    let out = compile_and_run(
        r#"<?php
eval('namespace EvalRelativeNs;
class Base {
    public string $label;
    public function __construct($label = "base") { $this->label = $label; }
    public function selfFactory() { return new self("self"); }
    public function staticFactory() { return new static("static"); }
}
class Child extends Base {
    public function parentFactory() { return new parent("parent"); }
}
$child = new Child("root");
$self = $child->selfFactory();
$static = $child->staticFactory();
$parent = $child->parentFactory();
echo get_class($self); echo ":"; echo $self->label; echo ":";
echo get_class($static); echo ":"; echo $static->label; echo ":";
echo get_class($parent); echo ":"; echo $parent->label;');
"#,
    );
    assert_eq!(
        out,
        "EvalRelativeNs\\Base:self:EvalRelativeNs\\Child:static:EvalRelativeNs\\Base:parent"
    );
}

/// Verifies eval supports PHP's legacy `var` public property marker through the bridge.
#[test]
fn test_eval_declared_legacy_var_properties() {
    let out = compile_and_run(
        r#"<?php
eval('trait EvalLegacyVarTrait {
    var ?string $label = "trait";
}
class EvalLegacyVarProperty {
    use EvalLegacyVarTrait;
    var $plain = "p";
    var ?int $count = null;
}
$object = new EvalLegacyVarProperty();
$plain = new ReflectionProperty("EvalLegacyVarProperty", "plain");
$count = new ReflectionProperty("EvalLegacyVarProperty", "count");
$label = new ReflectionProperty("EvalLegacyVarProperty", "label");
$defaults = (new ReflectionClass("EvalLegacyVarProperty"))->getDefaultProperties();
echo $object->plain; echo ":";
echo $plain->isPublic() ? "P" : "p"; echo ":";
echo $plain->hasType() ? "T" : "t"; echo ":";
echo $count->isPublic() ? "C" : "c"; echo ":";
echo $count->hasType() ? $count->getType()->getName() : "none"; echo ":";
echo $count->getType()->allowsNull() ? "N" : "n"; echo ":";
echo is_null($defaults["count"]) ? "null" : "bad"; echo ":";
echo $object->label; echo ":";
echo $label->isPublic() ? "L" : "l"; echo ":";
echo $label->getType()->getName();');
"#,
    );
    assert_eq!(out, "p:P:t:C:int:N:null:trait:L:string");
}

/// Verifies eval supports PHP comma-separated instance, static, and trait properties.
#[test]
fn test_eval_declared_comma_separated_properties() {
    let out = compile_and_run(
        r#"<?php
echo eval('class EvalMultiPropertyBox {
    public int $a = 1, $b = 2;
    public static int $s = 3, $t = 4;
    public function sum() { return $this->a + $this->b + self::$s + self::$t; }
}
trait EvalMultiPropertyTrait {
    public int $x = 5, $y = 6;
}
class EvalMultiPropertyTraitBox {
    use EvalMultiPropertyTrait;
    public function sum() { return $this->x + $this->y; }
}
$box = new EvalMultiPropertyBox();
$traitBox = new EvalMultiPropertyTraitBox();
echo $box->a . $box->b . ":";
echo EvalMultiPropertyBox::$s . EvalMultiPropertyBox::$t . ":";
echo $traitBox->x . $traitBox->y . ":";
return $box->sum() + $traitBox->sum();');
"#,
    );
    assert_eq!(out, "12:34:56:21");
}

/// Verifies native callable probes can see functions declared by eval after the barrier.
#[test]
fn test_eval_declared_function_is_visible_to_callable_probes() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_probe() { return 1; }');
echo function_exists('dyn_eval_probe') ? '1' : '0';
echo is_callable('DYN_EVAL_PROBE') ? '1' : '0';
echo function_exists('missing_eval_probe') ? '1' : '0';
"#,
    );
    assert_eq!(out, "110");
}

/// Verifies eval dynamic symbol probes are false before the barrier and true after it.
#[test]
fn test_eval_barrier_dynamic_symbol_probes_are_ordered_and_namespaced() {
    let out = compile_and_run(
        r#"<?php
echo function_exists("EvalBarrierNs\\dyn_eval_barrier") ? "bad" : "f";
echo class_exists("EvalBarrierNs\\DynEvalBarrierClass") ? "bad" : "c";
echo defined("EvalBarrierNs\\DYN_EVAL_BARRIER_CONST") ? "bad" : "d";
eval('namespace EvalBarrierNs;
function dyn_eval_barrier() { return "fn"; }
class DynEvalBarrierClass {}
define(__NAMESPACE__ . "\\DYN_EVAL_BARRIER_CONST", 9);');
echo function_exists("EvalBarrierNs\\dyn_eval_barrier") ? "F" : "bad";
echo is_callable("EvalBarrierNs\\dyn_eval_barrier") ? "I" : "bad";
echo class_exists("EvalBarrierNs\\DynEvalBarrierClass") ? "C" : "bad";
echo defined("EvalBarrierNs\\DYN_EVAL_BARRIER_CONST") ? "D" : "bad";
echo ":" . call_user_func("EvalBarrierNs\\dyn_eval_barrier");
echo ":" . \EvalBarrierNs\DYN_EVAL_BARRIER_CONST;
"#,
    );
    assert_eq!(out, "fcdFICD:fn:9");
}

/// Verifies eval-aware dynamic probes do not leak into sibling functions without a barrier.
#[test]
fn test_eval_barrier_is_not_inherited_by_sibling_functions() {
    let out = compile_and_run(
        r#"<?php
function eval_no_barrier_probe() {
    return function_exists("dyn_eval_isolated_fn") ? "Y" : "N";
}
function eval_with_barrier_probe() {
    eval('function dyn_eval_isolated_fn() { return 1; }');
    return function_exists("dyn_eval_isolated_fn") ? "Y" : "N";
}
echo eval_no_barrier_probe();
echo eval_with_barrier_probe();
echo eval_no_barrier_probe();
"#,
    );
    assert_eq!(out, "NYN");
}

/// Verifies callable probes inside eval inspect dynamic functions and supported builtins.
#[test]
fn test_eval_fragment_function_probes_use_dynamic_context() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_inner_probe() { return 1; }
echo function_exists("DYN_EVAL_INNER_PROBE") . "x";
echo is_callable("dyn_eval_inner_probe") . "x";
echo function_exists("strlen") . "x";
echo function_exists("eval") . "x";
echo function_exists("missing_eval_inner_probe") . "x";');
"#,
    );
    assert_eq!(out, "1x1x1xxx");
}

/// Verifies eval `class_exists()` probes generated AOT class-name metadata.
#[test]
fn test_eval_fragment_class_exists_probes_aot_classes() {
    let out = compile_and_run(
        r#"<?php
class EvalClassExistsProbe {}
eval('echo class_exists("EvalClassExistsProbe") ? "Y" : "N";
echo class_exists("evalclassexistsprobe") ? "Y" : "N";
echo class_exists("\EvalClassExistsProbe") ? "Y" : "N";
echo call_user_func("class_exists", "EvalClassExistsProbe") ? "Y" : "N";
echo call_user_func_array("class_exists", ["autoload" => false, "class" => "\EvalClassExistsProbe"]) ? "Y" : "N";
echo class_exists(class: "MissingEvalClassExistsProbe", autoload: false) ? "Y" : "N";');
"#,
    );
    assert_eq!(out, "YYYYYN");
}

/// Verifies eval `get_declared_*()` exposes generated AOT class-like names.
#[test]
fn test_eval_get_declared_symbols_exposes_aot_metadata() {
    let out = compile_and_run(
        r#"<?php
interface EvalDeclaredAotIface {}
trait EvalDeclaredAotTrait {}
enum EvalDeclaredAotEnum { case Ready; }
class EvalDeclaredAotClass implements EvalDeclaredAotIface { use EvalDeclaredAotTrait; }

eval('$classHit = false;
$enumHit = false;
foreach (get_declared_classes() as $name) {
    if ($name === "EvalDeclaredAotClass") { $classHit = true; }
    if ($name === "EvalDeclaredAotEnum") { $enumHit = true; }
}
$interfaceHit = false;
foreach (get_declared_interfaces() as $name) {
    if ($name === "EvalDeclaredAotIface") { $interfaceHit = true; }
}
$traitHit = false;
foreach (get_declared_traits() as $name) {
    if ($name === "EvalDeclaredAotTrait") { $traitHit = true; }
}
echo $classHit ? "C" : "c";
echo $enumHit ? "E" : "e";
echo ":";
echo $interfaceHit ? "I" : "i";
echo ":";
echo $traitHit ? "T" : "t";');
"#,
    );
    assert_eq!(out, "CE:I:T");
}

/// Verifies eval `interface_exists()` probes generated AOT interface metadata.
#[test]
fn test_eval_fragment_interface_exists_probes_aot_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface EvalInterfaceExistsProbe {}
class EvalInterfaceExistsImpl implements EvalInterfaceExistsProbe {}

eval('echo interface_exists("EvalInterfaceExistsProbe") ? "Y" : "N";
echo interface_exists("evalinterfaceexistsprobe") ? "Y" : "N";
echo interface_exists("\EvalInterfaceExistsProbe") ? "Y" : "N";
echo interface_exists("EvalInterfaceExistsImpl") ? "Y" : "N";
echo interface_exists("UnitEnum") ? "U" : "u";
echo interface_exists("BackedEnum") ? "B" : "b";
echo class_exists("UnitEnum") ? "C" : "c";
echo call_user_func("interface_exists", "EvalInterfaceExistsProbe") ? "Y" : "N";
echo call_user_func_array("interface_exists", ["autoload" => false, "interface" => "\EvalInterfaceExistsProbe"]) ? "Y" : "N";
echo function_exists("interface_exists");');
"#,
    );
    assert_eq!(out, "YYYNUBcYY1");
}

/// Verifies eval `trait_exists()` and `enum_exists()` probe generated AOT metadata.
#[test]
fn test_eval_fragment_trait_enum_exists_probe_aot_metadata() {
    let out = compile_and_run(
        r#"<?php
trait EvalTraitExistsProbe {}
enum EvalEnumExistsProbe { case Ready; }

eval('echo trait_exists("EvalTraitExistsProbe") ? "T" : "t";
echo trait_exists("evaltraitexistsprobe") ? "T" : "t";
echo trait_exists("\EvalEnumExistsProbe") ? "T" : "t";
echo enum_exists("EvalEnumExistsProbe") ? "E" : "e";
echo enum_exists("evalenumexistsprobe") ? "E" : "e";
echo enum_exists("EvalTraitExistsProbe") ? "E" : "e";
echo call_user_func("trait_exists", "EvalTraitExistsProbe") ? "T" : "t";
echo call_user_func_array("enum_exists", ["autoload" => false, "enum" => "\EvalEnumExistsProbe"]) ? "E" : "e";
echo trait_exists(trait: "MissingEvalTrait", autoload: false) ? "T" : "t";
echo enum_exists(enum: "MissingEvalEnum", autoload: false) ? "E" : "e";
echo function_exists("trait_exists"); echo function_exists("enum_exists");');
"#,
    );
    assert_eq!(out, "TTtEEeTEte11");
}

/// Verifies eval fragments can declare and use backed enums through the bridge.
#[test]
fn test_eval_fragment_declares_enum_cases_and_methods() {
    let out = compile_and_run(
        r#"<?php
eval('interface EvalDynLabel { function label(); }
enum EvalDynColor: string implements EvalDynLabel {
    case Red = "r";
    case Green = "g";
    public const PREFIX = "color";
    public function label() { return self::PREFIX . ":" . $this->name . ":" . $this->value; }
    public static function fallback() { return self::Red; }
}
$cases = EvalDynColor::cases();
echo enum_exists("evaldyncolor") ? "E" : "e";
echo class_exists("EvalDynColor") ? "C" : "c";
echo count($cases);
echo $cases[1] === EvalDynColor::Green ? "G" : "g";
echo EvalDynColor::Green->label();
echo EvalDynColor::from("r") === EvalDynColor::Red ? "F" : "f";
echo is_null(EvalDynColor::tryFrom("missing")) ? "N" : "n";
echo is_a(EvalDynColor::Red, "EvalDynLabel") ? "I" : "i";');
"#,
    );
    assert_eq!(out, "EC2Gcolor:Green:gFNI");
}

/// Verifies eval enums can import trait methods and report direct trait metadata.
#[test]
fn test_eval_declared_enum_trait_use() {
    let out = compile_and_run(
        r#"<?php
eval('trait EvalDynEnumTrait {
    public function label() { return $this->name; }
    public static function suffix() { return "S"; }
}
enum EvalDynTraitEnum {
    use EvalDynEnumTrait {
        label as private hiddenLabel;
    }
    case Ready;
    public function read() { return $this->label() . ":" . $this->hiddenLabel(); }
}
echo EvalDynTraitEnum::Ready->read(); echo ":";
echo EvalDynTraitEnum::suffix(); echo ":";
$ref = new ReflectionClass("EvalDynTraitEnum");
$traits = $ref->getTraitNames();
echo count($traits) . ":" . $traits[0] . ":";
$aliases = $ref->getTraitAliases();
echo $aliases["hiddenLabel"] . ":";
$uses = class_uses(EvalDynTraitEnum::Ready);
echo count($uses) . ":" . $uses["EvalDynEnumTrait"] . ":";
echo EvalDynTraitEnum::Ready->label();');
"#,
    );
    assert_eq!(
        out,
        "Ready:Ready:S:1:EvalDynEnumTrait:EvalDynEnumTrait::label:1:EvalDynEnumTrait:Ready"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalDynEnumPropertyTrait {
    public int $x = 1;
}
enum EvalDynInvalidTraitEnum {
    use EvalDynEnumPropertyTrait;
    case Ready;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval enum synthetic methods hide conflicting trait imports like PHP.
#[test]
fn test_eval_declared_enum_trait_synthetic_method_precedence() {
    let out = compile_and_run(
        r#"<?php
eval('trait EvalDynEnumSyntheticTrait {
    public function cases() { return "trait-cases"; }
    public static function from($value) { return "trait-from"; }
    public static function tryFrom($value) { return "trait-try"; }
}
enum EvalDynPureSynthetic {
    use EvalDynEnumSyntheticTrait {
        cases as traitCases;
    }
    case Ready;
}
enum EvalDynBackedSynthetic: string {
    use EvalDynEnumSyntheticTrait {
        cases as traitCases;
        from as traitFrom;
    }
    case Ready = "ready";
}
echo is_array(EvalDynPureSynthetic::Ready->cases()) ? "cases" : "bad"; echo ":";
echo EvalDynPureSynthetic::Ready->traitCases(); echo ":";
echo EvalDynPureSynthetic::from("x"); echo ":";
echo EvalDynPureSynthetic::Ready->from("x"); echo ":";
echo EvalDynBackedSynthetic::from("ready")->value; echo ":";
echo EvalDynBackedSynthetic::Ready->from("ready")->value; echo ":";
echo EvalDynBackedSynthetic::tryFrom("missing") === null ? "null" : "bad"; echo ":";
echo EvalDynBackedSynthetic::traitFrom("x"); echo ":";
echo EvalDynBackedSynthetic::Ready->traitCases(); echo ":";
echo is_callable([EvalDynBackedSynthetic::Ready, "cases"]) ? "callable" : "bad";');
"#,
    );
    assert_eq!(
        out,
        "cases:trait-cases:trait-from:trait-from:ready:ready:null:trait-from:trait-cases:callable"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('enum EvalDynInvalidBackedFrom: string {
    case Ready = "ready";
    public static function from($value) { return self::Ready; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval ReflectionMethod supports enum synthetic method metadata and invocation.
#[test]
fn test_eval_reflection_enum_synthetic_methods() {
    let out = compile_and_run(
        r#"<?php
eval('enum EvalDynReflectSyntheticEnum: string {
    case Ready = "ready";
}
enum EvalDynReflectPureSyntheticEnum {
    case Ready;
}
$ref = new ReflectionClass("EvalDynReflectSyntheticEnum");
$methods = $ref->getMethods(ReflectionMethod::IS_STATIC);
echo count($methods) . ":";
echo $methods[0]->getName() . "/" . $methods[1]->getName() . "/" . $methods[2]->getName() . ":";
$cases = $ref->getMethod("cases");
echo $cases->getReturnType() . ":";
echo count($cases->invoke(null)) . ":";
$from = new ReflectionMethod("EvalDynReflectSyntheticEnum", "from");
$params = $from->getParameters();
echo $from->getDeclaringClass()->getName() . ":";
echo $from->getNumberOfParameters() . "/" . $from->getNumberOfRequiredParameters() . ":";
echo $params[0]->getName() . "/" . $params[0]->getType() . ":";
echo $from->getReturnType() . ":";
echo $from->invoke(null, "ready")->name . ":";
$try = ReflectionMethod::createFromMethodName("EvalDynReflectSyntheticEnum::tryFrom");
echo $try->getReturnType() . ":";
echo ($try->invokeArgs(null, ["missing"]) === null ? "null" : "bad") . ":";
$pure = new ReflectionClass("EvalDynReflectPureSyntheticEnum");
echo count($pure->getMethods()) . ":";
echo $pure->hasMethod("from") ? "bad" : "nofrom";');
"#,
    );
    assert_eq!(
        out,
        "3:cases/from/tryFrom:array:1:EvalDynReflectSyntheticEnum:1/1:value/string|int:static:Ready:?static:null:1:nofrom"
    );
}

/// Verifies eval enums support user interfaces derived from PHP enum marker interfaces.
#[test]
fn test_eval_declared_enum_marker_interface_inheritance() {
    let out = compile_and_run(
        r#"<?php
eval('interface EvalDynUnitMarker extends UnitEnum {}
interface EvalDynBackedMarker extends BackedEnum {}
enum EvalDynMarkedUnit implements EvalDynUnitMarker {
    case Ready;
}
enum EvalDynMarkedBacked: string implements EvalDynBackedMarker {
    case Ready = "ready";
}
echo is_a(EvalDynMarkedUnit::Ready, "EvalDynUnitMarker") ? "U" : "u";
echo is_a(EvalDynMarkedBacked::Ready, "EvalDynBackedMarker") ? "B" : "b";
$unitInterfaces = class_implements("EvalDynMarkedUnit");
echo count($unitInterfaces) . ":" . $unitInterfaces["EvalDynUnitMarker"] . ":";
echo $unitInterfaces["UnitEnum"] . ":";
$backedInterfaces = (new ReflectionClass("EvalDynMarkedBacked"))->getInterfaceNames();
echo count($backedInterfaces) . ":" . $backedInterfaces[0] . ":";
echo $backedInterfaces[1] . ":" . $backedInterfaces[2] . ":";
echo EvalDynMarkedBacked::Ready->value;');
"#,
    );
    assert_eq!(
        out,
        "UB2:EvalDynUnitMarker:UnitEnum:3:EvalDynBackedMarker:UnitEnum:BackedEnum:ready"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('enum EvalDynExplicitUnitEnum implements UnitEnum {
    case Ready;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalDynBackedMarkerBad extends BackedEnum {}
enum EvalDynPureBackedMarker implements EvalDynBackedMarkerBad {
    case Ready;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared classes cannot implement PHP's special Throwable contract.
#[test]
fn test_eval_declared_class_rejects_throwable_interfaces() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalDynInvalidThrowable implements Throwable {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalDynThrowableMarker extends Throwable {}
class EvalDynInvalidThrowableMarker implements EvalDynThrowableMarker {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared classes must satisfy PHP builtin interface methods.
#[test]
fn test_eval_declared_class_rejects_missing_builtin_interface_methods() {
    let out = compile_and_run(
        r#"<?php
eval('abstract class EvalDynAbstractCountable implements Countable {}
class EvalDynValidCountable implements Countable {
    public function count(): int { return 4; }
}
echo count(new EvalDynValidCountable());');
"#,
    );
    assert_eq!(out, "4");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalDynMissingCountable implements Countable {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval enum `from()` misses throw catchable `ValueError` objects.
#[test]
fn test_eval_fragment_enum_from_miss_throws_value_error() {
    let out = compile_and_run(
        r#"<?php
eval('enum EvalDynStatus: string {
    case Draft = "draft";
}
try {
    EvalDynStatus::from("live");
    echo "bad";
} catch (ValueError $e) {
    echo get_class($e), ":", $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "ValueError:\"live\" is not a valid backing value for enum EvalDynStatus"
    );
}

/// Verifies eval can construct, catch, inspect, and call inherited methods on
/// the builtin `UnhandledMatchError` class through the native bridge.
#[test]
fn test_eval_fragment_constructs_unhandled_match_error() {
    let out = compile_and_run(
        r#"<?php
eval('try {
    throw new UnhandledMatchError("eval");
} catch (UnhandledMatchError $error) {
    echo ($error instanceof Error ? "yes:" : "no:") . $error->getMessage();
}');
"#,
    );
    assert_eq!(out, "yes:eval");
}

/// Verifies eval-declared enums reject magic methods PHP forbids on enums.
#[test]
fn test_eval_fragment_rejects_forbidden_enum_magic_method() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('enum EvalDynBadMagic {
    case Ready;
    public function __destruct() {}
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval `is_a()` and `is_subclass_of()` use generated AOT relation metadata.
#[test]
fn test_eval_fragment_is_a_relation_probes_aot_metadata() {
    let out = compile_and_run(
        r#"<?php
interface EvalRelationIface {}
class EvalRelationParent {}
class EvalRelationChild extends EvalRelationParent implements EvalRelationIface {}

eval('$object = new EvalRelationChild();
echo is_a($object, "EvalRelationChild") ? "Y" : "N";
echo is_a($object, "EvalRelationParent") ? "Y" : "N";
echo is_a($object, "EvalRelationIface") ? "Y" : "N";
echo is_subclass_of($object, "EvalRelationChild") ? "Y" : "N";
echo is_subclass_of($object, "EvalRelationParent") ? "Y" : "N";
echo is_subclass_of($object, "EvalRelationIface") ? "Y" : "N";
echo call_user_func("is_a", $object, "EvalRelationParent") ? "Y" : "N";
echo call_user_func_array("is_subclass_of", ["object_or_class" => $object, "class" => "EvalRelationParent"]) ? "Y" : "N";
echo is_a(object_or_class: $object, class: "MissingEvalRelation", allow_string: false) ? "Y" : "N";
echo function_exists("is_a"); echo function_exists("is_subclass_of");');
"#,
    );
    assert_eq!(out, "YYYNYYYYN11");
}

/// Verifies eval class-relation builtins materialize generated/AOT metadata.
#[test]
fn test_eval_class_relation_builtins_expose_aot_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotRelationBaseIface {}
interface EvalAotRelationChildIface extends EvalAotRelationBaseIface {}
trait EvalAotRelationInnerTrait {}
trait EvalAotRelationOuterTrait {
    use EvalAotRelationInnerTrait;
}
class EvalAotRelationBase {}
class EvalAotRelationMid extends EvalAotRelationBase {}
class EvalAotRelationChild extends EvalAotRelationMid implements EvalAotRelationChildIface {
    use EvalAotRelationOuterTrait;
}

eval('$object = new EvalAotRelationChild();
$implements = class_implements($object);
ksort($implements);
foreach ($implements as $name) { echo $name . ","; }
echo ":";
$stringImplements = call_user_func("class_implements", "EvalAotRelationChild");
ksort($stringImplements);
foreach ($stringImplements as $name) { echo $name . ","; }
echo ":";
$interfaceParents = class_implements("EvalAotRelationChildIface");
foreach ($interfaceParents as $name) { echo $name . ","; }
echo ":";
$uses = class_uses("EvalAotRelationChild");
foreach ($uses as $name) { echo $name . ","; }
echo ":";
$traitUses = class_uses("EvalAotRelationOuterTrait");
foreach ($traitUses as $name) { echo $name . ","; }
echo ":";
$parents = class_parents($object);
foreach ($parents as $name) { echo $name . ","; }
echo ":";
$stringParents = call_user_func_array("class_parents", ["object_or_class" => "EvalAotRelationChild"]);
foreach ($stringParents as $name) { echo $name . ","; }
echo ":";
echo function_exists("class_implements"); echo function_exists("class_parents");');
class_alias("EvalAotRelationChild", "EvalAotRelationAlias");
eval('echo ":";
$aliasImplements = class_implements("EvalAotRelationAlias");
ksort($aliasImplements);
foreach ($aliasImplements as $name) { echo $name . ","; }
echo ":";
$aliasParents = class_parents("EvalAotRelationAlias");
foreach ($aliasParents as $name) { echo $name . ","; }');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotRelationBaseIface,EvalAotRelationChildIface,:EvalAotRelationBaseIface,EvalAotRelationChildIface,:EvalAotRelationBaseIface,:EvalAotRelationOuterTrait,:EvalAotRelationInnerTrait,:EvalAotRelationMid,EvalAotRelationBase,:EvalAotRelationMid,EvalAotRelationBase,:11:EvalAotRelationBaseIface,EvalAotRelationChildIface,:EvalAotRelationChild,EvalAotRelationMid,EvalAotRelationBase,"
    );
}

/// Verifies eval `class_uses()` accepts eval-declared trait targets and aliases.
#[test]
fn test_eval_class_uses_exposes_eval_trait_targets() {
    let out = compile_and_run(
        r#"<?php
eval('trait EvalRelationInnerTrait {}
trait EvalRelationOuterTrait {
    use EvalRelationInnerTrait;
}
$uses = class_uses("EvalRelationOuterTrait");
echo count($uses) . ":";
echo $uses["EvalRelationInnerTrait"] . ":";
class_alias("EvalRelationOuterTrait", "EvalRelationOuterTraitAlias");
$aliasUses = call_user_func("class_uses", "EvalRelationOuterTraitAlias");
echo count($aliasUses) . ":";
echo $aliasUses["EvalRelationInnerTrait"];');
"#,
    );
    assert_eq!(out, "1:EvalRelationInnerTrait:1:EvalRelationInnerTrait");
}

/// Verifies eval `instanceof` probes AOT and eval-declared class metadata.
#[test]
fn test_eval_fragment_instanceof_probes_class_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalInstanceAotIface {}
class EvalInstanceAotParent {}
class EvalInstanceAotChild extends EvalInstanceAotParent implements EvalInstanceAotIface {}

eval('interface EvalInstanceDynIface {}
class EvalInstanceDynBase {}
class EvalInstanceDynChild extends EvalInstanceDynBase implements EvalInstanceDynIface {}
$aot = new EvalInstanceAotChild();
$dyn = new EvalInstanceDynChild();
$dynName = "EvalInstanceDynChild";
$dynTargets = ["EvalInstanceDynIface"];
$prefix = "EvalInstanceDyn";
$suffix = "Base";
$dynTargetObject = new EvalInstanceDynChild();
echo $aot instanceof EvalInstanceAotChild ? "A" : "a";
echo $aot instanceof EvalInstanceAotParent ? "P" : "p";
echo $aot instanceof EvalInstanceAotIface ? "I" : "i";
echo $dyn instanceof EvalInstanceDynChild ? "C" : "c";
echo $dyn instanceof EvalInstanceDynBase ? "B" : "b";
echo $dyn instanceof EvalInstanceDynIface ? "F" : "f";
echo $dyn instanceof $dynName ? "D" : "d";
echo $dyn instanceof $dynTargets[0] ? "T" : "t";
echo $dyn instanceof ($prefix . $suffix) ? "X" : "x";
echo $dyn instanceof $dynTargetObject ? "O" : "o";
echo 7 instanceof MissingEvalInstance ? "bad" : "S";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "APICBFDTXOS");
}

/// Verifies eval-declared classes can extend generated/AOT classes at runtime.
#[test]
fn test_eval_declared_class_extends_aot_parent() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeParentBase {
    public int $x;
    public function __construct($x) { $this->x = $x; }
    public function read() { return $this->x; }
}
eval('class EvalRuntimeParentChild extends EvalRuntimeParentBase {
    public function own() { return $this->read() + 1; }
}
$box = new EvalRuntimeParentChild(6);
echo get_class($box); echo ":";
echo get_parent_class($box); echo ":";
echo is_a($box, "EvalRuntimeParentChild") ? "D" : "d"; echo ":";
echo is_a($box, "EvalRuntimeParentBase") ? "P" : "p"; echo ":";
echo is_subclass_of($box, "EvalRuntimeParentBase") ? "S" : "s"; echo ":";
echo is_subclass_of("EvalRuntimeParentChild", "EvalRuntimeParentBase") ? "N" : "n"; echo ":";
echo $box->read(); echo ":";
echo $box->own(); echo ":";
$parent = (new ReflectionClass("EvalRuntimeParentChild"))->getParentClass();
echo $parent ? $parent->getName() : "missing";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalRuntimeParentChild:EvalRuntimeParentBase:D:P:S:N:6:7:EvalRuntimeParentBase"
    );
}

/// Verifies eval-declared children can call inherited protected AOT instance methods.
#[test]
fn test_eval_declared_child_calls_inherited_protected_aot_instance_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeProtectedMethodParent {
    protected function add(int $n): int {
        return $n + 2;
    }
}

eval('class EvalRuntimeProtectedMethodChild extends EvalRuntimeProtectedMethodParent {
    public function run(): void {
        echo $this->add(3);
    }
}
$box = new EvalRuntimeProtectedMethodChild();
$box->run();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "5");
}

/// Verifies eval-declared children can call inherited protected AOT static methods.
#[test]
fn test_eval_declared_child_calls_inherited_protected_aot_static_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeProtectedStaticMethodParent {
    protected static function add(int $n): int {
        return $n + 2;
    }
}

eval('class EvalRuntimeProtectedStaticMethodChild extends EvalRuntimeProtectedStaticMethodParent {
    public function run(): void {
        echo self::add(4);
    }
}
$box = new EvalRuntimeProtectedStaticMethodChild();
$box->run();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "6");
}

/// Verifies `parent::` in eval children can call inherited non-static AOT methods on `$this`.
#[test]
fn test_eval_declared_child_parent_static_syntax_calls_aot_instance_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeParentSyntaxInstanceParent {
    protected function label(): string {
        return "parent";
    }
}

eval('class EvalRuntimeParentSyntaxInstanceChild extends EvalRuntimeParentSyntaxInstanceParent {
    public function run(): void {
        echo parent::label();
    }
}
$box = new EvalRuntimeParentSyntaxInstanceChild();
$box->run();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "parent");
}

/// Verifies `parent::` in eval children bridges protected static AOT method scope.
#[test]
fn test_eval_declared_child_parent_static_syntax_calls_aot_static_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeParentSyntaxStaticParent {
    protected static function label(): string {
        return "parent-static";
    }
}

eval('class EvalRuntimeParentSyntaxStaticChild extends EvalRuntimeParentSyntaxStaticParent {
    public function run(): void {
        echo parent::label();
    }
}
$box = new EvalRuntimeParentSyntaxStaticChild();
$box->run();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "parent-static");
}

/// Verifies eval-declared children can read and write inherited protected AOT properties.
#[test]
fn test_eval_declared_child_accesses_inherited_protected_aot_property() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeProtectedPropertyParent {
    protected int $x = 1;
}

eval('class EvalRuntimeProtectedPropertyChild extends EvalRuntimeProtectedPropertyParent {
    public function run(): void {
        echo isset($this->x) ? "I:" : "i:";
        $this->x = 7;
        echo $this->x;
    }
}
$box = new EvalRuntimeProtectedPropertyChild();
$box->run();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "I:7");
}

/// Verifies eval-declared children can read and write inherited protected AOT static properties.
#[test]
fn test_eval_declared_child_accesses_inherited_protected_aot_static_property() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeProtectedStaticPropertyParent {
    protected static int $x = 1;
}

eval('class EvalRuntimeProtectedStaticPropertyChild extends EvalRuntimeProtectedStaticPropertyParent {
    public function run(): void {
        echo isset(self::$x) ? "S:" : "s:";
        self::$x = 8;
        echo self::$x;
    }
}
$box = new EvalRuntimeProtectedStaticPropertyChild();
$box->run();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "S:8");
}

/// Verifies eval-declared children can read inherited protected AOT class constants.
#[test]
fn test_eval_declared_child_reads_inherited_protected_aot_class_constant() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeProtectedConstantParent {
    protected const X = 9;
}

eval('class EvalRuntimeProtectedConstantChild extends EvalRuntimeProtectedConstantParent {
    public function run(): void {
        echo self::X;
    }
}
$box = new EvalRuntimeProtectedConstantChild();
$box->run();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "9");
}

/// Verifies eval-declared children can run inherited protected AOT constructors.
#[test]
fn test_eval_declared_child_runs_inherited_protected_aot_constructor() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalRuntimeProtectedConstructorParent {
    public int $x = 0;
    protected function __construct(int $x) {
        $this->x = $x + 2;
    }
}

eval('class EvalRuntimeProtectedConstructorChild extends EvalRuntimeProtectedConstructorParent {
    public static function make() {
        return new self(3);
    }
}
$box = EvalRuntimeProtectedConstructorChild::make();
echo $box->x;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "5");
}

/// Verifies eval-declared classes inherit AOT callable object and method behavior.
#[test]
fn test_eval_declared_class_inherits_aot_invokable_parent_callables() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCallableParent {
    public string $prefix;
    public function __construct(string $prefix = "box") { $this->prefix = $prefix; }
    public function read(string $value = "R"): string { return $this->prefix . ":" . $value; }
    public function __invoke(string $left = "A", string $right = "B"): string {
        return $this->prefix . ":" . $left . $right;
    }
}

eval('class EvalRuntimeCallableChild extends EvalAotCallableParent {}
$box = new EvalRuntimeCallableChild("box");
echo is_callable($box) ? "I:" : "bad:";
echo $box(right: "D", left: "C") . ":";
$first = $box(...);
echo $first("E", "F") . ":";
echo call_user_func($box, "G", "H") . ":";
echo call_user_func_array($box, ["right" => "J", "left" => "I"]) . ":";
echo is_callable([$box, "read"]) ? "M:" : "bad:";
echo call_user_func([$box, "read"], "K") . ":";
echo call_user_func_array([$box, "read"], ["value" => "L"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "I:box:CD:box:EF:box:GH:box:IJ:M:box:K:box:L");
}

/// Verifies eval first-class callables retain access to inherited protected AOT methods.
#[test]
fn test_eval_declared_child_first_class_callable_inherited_protected_aot_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalProtectedCallableParent {
    public function read(string $value = "R"): string {
        return "P:" . $value;
    }
    protected function hidden(string $value): string {
        return "H:" . $value;
    }
}

eval('class EvalProtectedCallableChild extends EvalProtectedCallableParent {
    public function makeHidden() {
        return $this->hidden(...);
    }
}
$box = new EvalProtectedCallableChild();
$public = $box->read(...);
echo $public("A") . ":";
$hidden = $box->makeHidden();
echo is_callable($hidden) ? "callable:" : "bad:";
echo call_user_func($hidden, "B") . ":";
echo $hidden("C");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "P:A:callable:H:B:H:C");
}

/// Verifies first-class AOT object callables keep eval-child late-static scope.
#[test]
fn test_eval_declared_child_first_class_aot_method_callable_preserves_late_static_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCallableScopeParent {
    protected function hiddenScope() {
        return eval('return self::class . ":" . static::class . ":" . get_called_class();');
    }
}

eval('class EvalAotCallableScopeChild extends EvalAotCallableScopeParent {
    public function makeHidden() {
        return $this->hiddenScope(...);
    }
}
$box = new EvalAotCallableScopeChild();
$hidden = $box->makeHidden();
echo $hidden() . "|";
echo call_user_func($hidden);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotCallableScopeParent:EvalAotCallableScopeChild:EvalAotCallableScopeChild|\
EvalAotCallableScopeParent:EvalAotCallableScopeChild:EvalAotCallableScopeChild"
    );
}

/// Verifies eval-declared classes expose inherited AOT members to OOP introspection.
#[test]
fn test_eval_declared_class_inherits_aot_parent_member_introspection() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotIntrospectionParent {
    public int $pub = 1;
    protected int $prot = 2;
    protected function guarded() {}
    public function read() {}
    public function parentView() {
        return eval('$methods = get_class_methods("EvalRuntimeIntrospectionChild");
$objectMethods = get_class_methods($this);
$objectRef = new ReflectionObject($this);
return (in_array("guarded", $methods) ? "parentProtected" : "bad") . ":" .
    get_class($this) . ":" .
    get_parent_class($this) . ":" .
    $objectRef->getName() . ":" .
    (in_array("childRead", $objectMethods) ? "objectChild" : "bad");');
    }
}

eval('class EvalRuntimeIntrospectionChild extends EvalAotIntrospectionParent {
    public function childRead() {}
    public function childView() {
        $methods = get_class_methods($this);
        echo in_array("guarded", $methods) ? "P" : "p";
    }
}
$box = new EvalRuntimeIntrospectionChild();
echo method_exists("EvalRuntimeIntrospectionChild", "guarded") ? "classProtected:" : "bad:";
echo method_exists($box, "guarded") ? "objectProtected:" : "bad:";
echo property_exists($box, "pub") ? "objectPublicProp:" : "bad:";
echo property_exists("EvalRuntimeIntrospectionChild", "prot") ? "classProtectedProp:" : "bad:";
$outside = get_class_methods("EvalRuntimeIntrospectionChild");
echo in_array("read", $outside) ? "outsideParent:" : "bad:";
echo in_array("guarded", $outside) ? "bad:" : "outsideNoProtected:";
echo in_array("childRead", $outside) ? "outsideChild:" : "bad:";
$box->childView();
echo ":";
echo $box->parentView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "classProtected:objectProtected:objectPublicProp:classProtectedProp:outsideParent:outsideNoProtected:outsideChild:P:parentProtected:EvalRuntimeIntrospectionChild:EvalAotIntrospectionParent:EvalRuntimeIntrospectionChild:objectChild"
    );
}

/// Verifies eval-declared class-like symbols remain visible in generated eval contexts.
#[test]
fn test_eval_declared_class_likes_are_visible_in_aot_nested_eval_context() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalClassLikeAotView {
    public function view() {
        return eval('return (class_exists("EvalGlobalContextClass") ? "C" : "c") .
    (class_exists("EvalGlobalContextClassAlias") ? "A" : "a") . ":" .
    (interface_exists("EvalGlobalContextIface") ? "I" : "i") .
    (interface_exists("EvalGlobalContextIfaceAlias") ? "A" : "a") . ":" .
    (trait_exists("EvalGlobalContextTrait") ? "T" : "t") .
    (trait_exists("EvalGlobalContextTraitAlias") ? "A" : "a") . ":" .
    (enum_exists("EvalGlobalContextEnum") ? "E" : "e") .
    (enum_exists("EvalGlobalContextEnumAlias") ? "A" : "a") .
    (class_exists("EvalGlobalContextEnum") ? "C" : "c") .
    (class_exists("EvalGlobalContextEnumAlias") ? "A" : "a") . ":" .
    (is_a("EvalGlobalContextClass", "EvalGlobalContextIface", true) ? "R" : "r");');
    }
}

eval('interface EvalGlobalContextIface {}
trait EvalGlobalContextTrait {}
enum EvalGlobalContextEnum { case Ready; }
class EvalGlobalContextClass implements EvalGlobalContextIface { use EvalGlobalContextTrait; }
class_alias("EvalGlobalContextClass", "EvalGlobalContextClassAlias");
class_alias("EvalGlobalContextIface", "EvalGlobalContextIfaceAlias");
class_alias("EvalGlobalContextTrait", "EvalGlobalContextTraitAlias");
class_alias("EvalGlobalContextEnum", "EvalGlobalContextEnumAlias");');
$view = new EvalClassLikeAotView();
echo $view->view();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "CA:IA:TA:EACA:R");
}

/// Verifies eval-declared class inheritance uses dynamic methods and metadata.
#[test]
fn test_eval_declared_class_inherits_methods_and_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalDynIface {}

eval('class EvalDynBase {
    public int $base = 1;
    public function __construct($base) { $this->base = $base; }
    public function sum($n) { return $this->base + $this->tail + $n; }
}
class EvalDynChild extends EvalDynBase implements EvalDynIface {
    public int $tail = 4;
    public function read($n) { return $this->sum($n); }
}
$box = new EvalDynChild(3);
echo $box->read(5) . ":";
echo get_parent_class($box) . ":";
echo is_a($box, "EvalDynBase") ? "isa" : "bad"; echo ":";
echo is_a($box, "EvalDynIface") ? "iface" : "bad"; echo ":";
echo is_subclass_of($box, "EvalDynChild") ? "bad" : "self"; echo ":";
echo is_subclass_of($box, "EvalDynBase") ? "sub" : "bad"; echo ":";
$parents = class_parents($box);
echo count($parents) . ":" . $parents["EvalDynBase"] . ":";
$implements = class_implements("EvalDynChild");
echo count($implements) . ":" . $implements["EvalDynIface"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "12:EvalDynBase:isa:iface:self:sub:1:EvalDynBase:1:EvalDynIface"
    );
}

/// Verifies eval static method calls preserve PHP forwarding and late-static binding.
#[test]
fn test_eval_declared_static_method_calls_preserve_forwarding() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalStaticForwardA {
    public static function who() { return static::tag(); }
    public static function relayNamed() { return EvalStaticForwardA::who(); }
    public static function relaySelf() { return self::who(); }
    public static function tag() { return "A"; }
}
class EvalStaticForwardB extends EvalStaticForwardA {
    public static function relayParent() { return parent::who(); }
    public static function relayStatic() { return static::who(); }
    public static function tag() { return "B"; }
}
echo EvalStaticForwardB::relayNamed(); echo ":";
echo EvalStaticForwardB::relaySelf(); echo ":";
echo EvalStaticForwardB::relayParent(); echo ":";
echo EvalStaticForwardB::relayStatic();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "A:B:B:B");
}

/// Verifies eval-declared interfaces are usable by eval-declared classes.
#[test]
fn test_eval_declared_interface_metadata_and_implementation() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalDynReader {
    function read($n);
}
interface EvalDynNamedReader extends EvalDynReader {
    function label();
}
class EvalDynReaderBox implements EvalDynNamedReader {
    public function read($n) { return $n + 1; }
    public function label() { return "box"; }
}
$box = new EvalDynReaderBox();
echo interface_exists("EvalDynReader") ? "iface" : "bad"; echo ":";
echo class_exists("EvalDynReader") ? "bad" : "notclass"; echo ":";
$declaredInterfaces = get_declared_interfaces();
$readerDeclared = false;
$namedDeclared = false;
foreach ($declaredInterfaces as $name) {
    if ($name === "EvalDynReader") { $readerDeclared = true; }
    if ($name === "EvalDynNamedReader") { $namedDeclared = true; }
}
echo ($readerDeclared && $namedDeclared) ? "declared" : "missing"; echo ":";
echo $box->read(4) . ":";
echo $box->label() . ":";
echo is_a($box, "EvalDynNamedReader") ? "isa" : "bad"; echo ":";
echo is_subclass_of("EvalDynReaderBox", "EvalDynReader") ? "str" : "bad"; echo ":";
$implements = class_implements($box);
echo count($implements) . ":" . $implements["EvalDynNamedReader"] . ":" . $implements["EvalDynReader"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "iface:notclass:declared:5:box:isa:str:2:EvalDynNamedReader:EvalDynReader"
    );
}

/// Verifies eval-declared method overrides enforce covariant return types.
#[test]
fn test_eval_declared_method_return_type_override_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReturnBase {
    public function id(): ?int { return 1; }
    public function make(): EvalReturnBase { return $this; }
    public function selfType(): self { return $this; }
}
class EvalReturnChild extends EvalReturnBase {
    public function id(): int { return 2; }
    public function make(): EvalReturnChild { return $this; }
    public function selfType(): static { return $this; }
}
class EvalReturnParentRoot {}
class EvalReturnParentBase extends EvalReturnParentRoot {
    public function parentKeyword(): EvalReturnParentRoot { return new EvalReturnParentRoot(); }
}
class EvalReturnParentChild extends EvalReturnParentBase {
    public function parentKeyword(): parent { return new EvalReturnParentBase(); }
}
class EvalReturnMixedBase {
    public function maybe(): mixed { return null; }
}
class EvalReturnMixedChild extends EvalReturnMixedBase {
    public function maybe(): ?int { return null; }
}
$child = new EvalReturnChild();
echo $child->id();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnNarrowBase {
    public function id(): int { return 1; }
}
class EvalReturnWiderNullable extends EvalReturnNarrowBase {
    public function id(): ?int { return 2; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnStaticBase {
    public function make(): static { return $this; }
}
class EvalReturnSelfChild extends EvalReturnStaticBase {
    public function make(): self { return $this; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnNullableBase {
    public function maybe(): ?int { return null; }
}
class EvalReturnMixedChildBad extends EvalReturnNullableBase {
    public function maybe(): mixed { return null; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared method overrides enforce contravariant parameter types.
#[test]
fn test_eval_declared_method_parameter_type_override_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalParamBase {
    public function anyInt(int $value) { return $value; }
    public function maybeInt(int $value) { return $value; }
    public function untypedInt(int $value) { return $value; }
}
class EvalParamChild extends EvalParamBase {
    public function anyInt(mixed $value) { return $value . ":mixed"; }
    public function maybeInt(?int $value) { return $value; }
    public function untypedInt($value) { return $value; }
}
$child = new EvalParamChild();
echo $child->anyInt(7) . ":";
echo $child->untypedInt("ok") . ":";
echo $child->maybeInt(null) === null ? "null" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:mixed:ok:null");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalParamTypeBase {
    public function read(int $value) { return $value; }
}
class EvalParamStringChild extends EvalParamTypeBase {
    public function read(string $value) { return $value; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalParamNullableBase {
    public function maybe(?int $value) { return $value; }
}
class EvalParamNonNullChild extends EvalParamNullableBase {
    public function maybe(int $value) { return $value; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalParamUntypedBase {
    public function read($value) { return $value; }
}
class EvalParamTypedChild extends EvalParamUntypedBase {
    public function read(int $value) { return $value; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared interface methods enforce covariant return types.
#[test]
fn test_eval_declared_interface_return_type_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalReturnReadable {
    function read(): int|string;
}
class EvalReturnReader implements EvalReturnReadable {
    public function read(): int {
        return 7;
    }
}
interface EvalReturnRootSelf {
    function linked(): self;
}
interface EvalReturnChildSelf extends EvalReturnRootSelf {}
class EvalReturnSelfImpl implements EvalReturnChildSelf {
    public function linked(): EvalReturnRootSelf {
        return $this;
    }
}
$reader = new EvalReturnReader();
echo $reader->read();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalNeedsReturn {
    function read(): string;
}
class EvalMissingReturnImpl implements EvalNeedsReturn {
    public function read() { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalNeedsStringReturn {
    function read(): string;
}
class EvalWiderReturnImpl implements EvalNeedsStringReturn {
    public function read(): int|string { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared interface methods enforce contravariant parameter types.
#[test]
fn test_eval_declared_interface_parameter_type_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalParamContract {
    function read(int $value);
}
class EvalParamContractReader implements EvalParamContract {
    public function read(mixed $value) {
        return $value . ":ok";
    }
}
$reader = new EvalParamContractReader();
echo $reader->read(8);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "8:ok");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalParamStringContract {
    function read(int $value);
}
class EvalParamStringReader implements EvalParamStringContract {
    public function read(string $value) { return $value; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalParamUntypedContract {
    function read($value);
}
class EvalParamTypedReader implements EvalParamUntypedContract {
    public function read(int $value) { return $value; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared abstract classes validate declared interface method signatures.
#[test]
fn test_eval_declared_abstract_interface_method_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalAbstractIfaceDeferred {
    function read(int $value): int;
}
abstract class EvalAbstractIfaceDeferredBase implements EvalAbstractIfaceDeferred {}
abstract class EvalAbstractIfaceDeferredTyped implements EvalAbstractIfaceDeferred {
    abstract public function read(mixed $value): int;
}
echo "ok";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "ok");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalAbstractIfaceParam {
    function read(int $value);
}
abstract class EvalAbstractIfaceParamBase implements EvalAbstractIfaceParam {
    abstract public function read(string $value);
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalInheritedIfaceMethod {
    function read(int $value);
}
abstract class EvalInheritedIfaceMethodBase {
    public function read(string $value) {}
}
abstract class EvalInheritedIfaceMethodChild extends EvalInheritedIfaceMethodBase implements EvalInheritedIfaceMethod {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared methods enforce declared return values at runtime.
#[test]
fn test_eval_declared_method_return_type_values() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReturnRuntimeBase {
    public function id(): int { return "12"; }
    public function makeSelf(): self { return new EvalReturnRuntimeBase(); }
    public function done(): void { return; }
}
class EvalReturnRuntimeChild extends EvalReturnRuntimeBase {}
$child = new EvalReturnRuntimeChild();
echo $child->id();
echo ":" . get_class($child->makeSelf()) . ":";
$child->done();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "12:EvalReturnRuntimeBase:");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnBadScalar {
    public function id(): int { return "nope"; }
}
$box = new EvalReturnBadScalar();
echo $box->id();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnBadVoid {
    public function done(): void { return null; }
}
$box = new EvalReturnBadVoid();
$box->done();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnStaticRuntimeBase {
    public function make(): static { return new EvalReturnStaticRuntimeBase(); }
}
class EvalReturnStaticRuntimeChild extends EvalReturnStaticRuntimeBase {}
$child = new EvalReturnStaticRuntimeChild();
$child->make();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReturnImplicitBad {
    public function id(): ?int {}
}
$box = new EvalReturnImplicitBad();
$box->id();');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared abstract classes can defer interface methods to concrete children.
#[test]
fn test_eval_declared_abstract_class_and_final_method_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalAbstractContract {
    function read($n);
}
abstract class EvalAbstractBase implements EvalAbstractContract {
    abstract public function read($n);
    final public function label() { return "base"; }
    public function wrap($n) { return $this->read($n) + 1; }
}
class EvalAbstractChild extends EvalAbstractBase {
    public function read($n) { return $n + 2; }
}
$box = new EvalAbstractChild();
echo $box->wrap(5) . ":";
echo $box->label() . ":";
echo is_a($box, "EvalAbstractContract") ? "iface" : "bad"; echo ":";
echo is_subclass_of($box, "EvalAbstractBase") ? "abstract" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "8:base:iface:abstract");
}

/// Verifies eval-declared final classes cannot be extended.
#[test]
fn test_eval_declared_final_class_extension_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('final class EvalFinalBase {}
class EvalFinalChild extends EvalFinalBase {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared traits contribute methods, properties, and metadata through the bridge.
#[test]
fn test_eval_declared_trait_methods_properties_and_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalDynamicTrait {
    public int $seed = 2;
    public function add($n) { return $this->seed + $n; }
}
class EvalDynamicTraitBox {
    use EvalDynamicTrait;
    public function read($n) { return $this->add($n) + 1; }
}
$box = new EvalDynamicTraitBox();
echo $box->read(4) . ":";
echo trait_exists("EvalDynamicTrait") ? "trait" : "bad"; echo ":";
$traits = get_declared_traits();
echo count($traits) . ":" . $traits[0] . ":";
$uses = class_uses($box);
echo count($uses) . ":" . $uses["EvalDynamicTrait"] . ":";
echo $box->seed;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "7:trait:1:EvalDynamicTrait:1:EvalDynamicTrait:2"
    );
}

/// Verifies eval-declared trait adaptations resolve conflicts, aliases, and visibility.
#[test]
fn test_eval_declared_trait_adaptations() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalAdaptA {
    public function talk() { return "A"; }
    public function hidden() { return "secret"; }
}
trait EvalAdaptB {
    public function talk() { return "B"; }
}
class EvalAdaptBox {
    use EvalAdaptA, EvalAdaptB {
        EvalAdaptA::talk insteadof EvalAdaptB;
        EvalAdaptB::talk as talkB;
        EvalAdaptA::hidden as private;
    }
    public function read() {
        return $this->talk() . ":" . $this->talkB() . ":" . $this->hidden();
    }
}
$box = new EvalAdaptBox();
echo $box->read() . ":";
echo $box->talk();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "A:B:secret:A");
}

/// Verifies eval trait declarations can compose other eval traits.
#[test]
fn test_eval_declared_trait_uses_trait_composition() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalNestedInner {
    public const WORD = "in";
    public function word() { return self::WORD; }
}
trait EvalNestedOuter {
    use EvalNestedInner {
        word as private hiddenWord;
    }
    public function read() { return $this->word() . $this->hiddenWord(); }
}
class EvalNestedBox {
    use EvalNestedOuter;
}
$box = new EvalNestedBox();
echo $box->read() . ":";
$ref = new ReflectionClass("EvalNestedOuter");
$traits = $ref->getTraitNames();
echo count($traits) . ":" . $traits[0] . ":";
$aliases = $ref->getTraitAliases();
echo $aliases["hiddenWord"] . ":";
$uses = class_uses($box);
echo count($uses) . ":" . $uses["EvalNestedOuter"] . ":";
echo $box->word();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "inin:1:EvalNestedInner:EvalNestedInner::word:1:EvalNestedOuter:in"
    );
}

/// Verifies eval-declared trait visibility adaptations affect bridge access checks.
#[test]
fn test_eval_declared_trait_visibility_adaptation_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalAdaptHidden {
    public function hidden() { return "secret"; }
}
class EvalAdaptHiddenBox {
    use EvalAdaptHidden {
        EvalAdaptHidden::hidden as private;
    }
}
$box = new EvalAdaptHiddenBox();
echo $box->hidden();');
"#,
    );
    assert!(
        err.contains("Fatal error: uncaught exception"),
        "stderr did not contain uncaught throwable diagnostic: {err}"
    );
}

/// Verifies eval-declared trait aliases follow PHP collision and no-op rules.
#[test]
fn test_eval_declared_trait_alias_collision_rules() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalAliasSource {
    public function source() { return "T"; }
}
class EvalAliasClassCollisionBox {
    use EvalAliasSource {
        source as target;
    }
    public function target() { return "C"; }
    public function read() { return $this->source() . $this->target(); }
}
class EvalAliasNoopBox {
    use EvalAliasSource {
        source as source;
    }
}
$box = new EvalAliasClassCollisionBox();
echo $box->read() . ":";
echo (new EvalAliasNoopBox())->source();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "TC:T");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalAliasVisibilitySource {
    public function source() { return "T"; }
}
class EvalAliasVisibilityBox {
    use EvalAliasVisibilitySource {
        source as private source;
    }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared trait adaptations reject missing method targets.
#[test]
fn test_eval_declared_invalid_trait_adaptation_target_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalAdaptMissingMethod {
    public function source() { return "T"; }
}
class EvalAdaptMissingMethodBox {
    use EvalAdaptMissingMethod {
        EvalAdaptMissingMethod::missing insteadof EvalAdaptMissingMethod;
    }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared `insteadof` rejects excluding the selected trait itself.
#[test]
fn test_eval_declared_trait_insteadof_self_exclusion_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalAdaptSelfExcluded {
    public function source() { return "T"; }
}
class EvalAdaptSelfExcludedBox {
    use EvalAdaptSelfExcluded {
        EvalAdaptSelfExcluded::source insteadof EvalAdaptSelfExcluded;
    }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared trait abstract methods must be implemented by concrete classes.
#[test]
fn test_eval_declared_trait_abstract_method_requirement_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalTraitNeedsConcrete {
    abstract public function read();
}
class EvalTraitMissingConcrete {
    use EvalTraitNeedsConcrete;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared trait property conflicts follow PHP compatibility rules.
#[test]
fn test_eval_declared_trait_property_conflict_rules() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalTraitPropCompatA {
    public int $value;
}
trait EvalTraitPropCompatB {
    public int $value;
}
class EvalTraitPropCompatBox {
    use EvalTraitPropCompatA, EvalTraitPropCompatB;
    public int $value;
    public function __construct($value) { $this->value = $value; }
}
$box = new EvalTraitPropCompatBox(9);
echo $box->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "9");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalTraitPropBad {
    public int $value;
}
class EvalTraitPropBadBox {
    use EvalTraitPropBad;
    public string $value;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared private/protected members are usable from valid class scopes.
#[test]
fn test_eval_declared_private_and_protected_members() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalVisibilityBase {
    private int $secret = 4;
    protected int $base = 5;
    private function bump($n) { return $this->secret + $n; }
    protected function add($n) { return $this->base + $n; }
    public function readPrivate($n) { return $this->bump($n); }
}
class EvalVisibilityChild extends EvalVisibilityBase {
    public function readProtected($n) { return $this->add($n); }
}
$box = new EvalVisibilityChild();
echo $box->readPrivate(3) . ":";
echo $box->readProtected(2);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:7");
}

/// Verifies eval OOP introspection builtins preserve PHP visibility and scope rules.
#[test]
fn test_eval_declared_oop_introspection_builtins() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalOopIntrospectBase {
    private $baseSecret = "bp";
    protected $baseProtected = "bq";
    public $basePublic = "br";
    private function basePrivate() {}
    protected function baseProtectedMethod() {}
    public function basePublicMethod() {}
    public function parentView() {
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
class EvalOopIntrospectChild extends EvalOopIntrospectBase {
    private $childSecret = "cp";
    protected $childProtected = "cq";
    public $childPublic = "cr";
    private function childPrivate() {}
    protected function childProtectedMethod() {}
    public function childPublicMethod() {}
    public function childView() {
        $methods = get_class_methods($this);
        sort($methods);
        echo implode(",", $methods); echo "|";
        $vars = get_object_vars($this);
        ksort($vars);
        echo implode(",", array_keys($vars));
    }
}
$object = new EvalOopIntrospectChild();
$object->dynamic = "dyn";
echo method_exists("EvalOopIntrospectChild", "basePrivate") ? "bad" : "noParentPrivateMethod"; echo ":";
echo method_exists($object, "basePrivate") ? "objectParentPrivateMethod" : "bad"; echo ":";
echo method_exists("EvalOopIntrospectChild", "baseProtectedMethod") ? "classProtectedMethod" : "bad"; echo ":";
echo property_exists("EvalOopIntrospectChild", "baseSecret") ? "bad" : "noParentPrivateProperty"; echo ":";
echo property_exists($object, "baseSecret") ? "bad" : "noObjectParentPrivateProperty"; echo ":";
echo property_exists($object, "dynamic") ? "dynamicProperty" : "bad"; echo ":";
$methods = get_class_methods("EvalOopIntrospectChild");
sort($methods);
echo implode(",", $methods); echo ":";
$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
$object->childView(); echo ":";
$object->parentView(); echo ":";
echo call_user_func("method_exists", $object, "childPrivate") ? "callMethod" : "bad"; echo ":";
echo call_user_func_array("property_exists", ["property" => "dynamic", "object_or_class" => $object]) ? "namedProperty" : "bad"; echo ":";
echo function_exists("method_exists"); echo function_exists("property_exists");
echo function_exists("get_class_methods"); echo function_exists("get_object_vars");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "noParentPrivateMethod:objectParentPrivateMethod:classProtectedMethod:noParentPrivateProperty:noObjectParentPrivateProperty:dynamicProperty:basePublicMethod,childPublicMethod,childView,parentView:basePublic,childPublic,dynamic:baseProtectedMethod,basePublicMethod,childPrivate,childProtectedMethod,childPublicMethod,childView,parentView|baseProtected,basePublic,childProtected,childPublic,childSecret,dynamic:baseProtected,basePublic,baseSecret,childProtected,childPublic,dynamic:callMethod:namedProperty:1111"
    );
}

/// Verifies eval `get_class_vars()` exposes visible class-like defaults like PHP.
#[test]
fn test_eval_declared_get_class_vars_builtin() {
    let out = compile_and_run_capture(
        r#"<?php
eval('trait EvalClassVarsTrait {
    public $traitPublic = "tp";
    protected $traitProtected = "tq";
}
enum EvalClassVarsBacked: int { case Ready = 1; }
class EvalClassVarsBase {
    public $basePublic = "bp";
    protected $baseProtected = "bq";
    private $basePrivate = "bs";
    public static $baseStatic = "static";
    public int $typed;
}
class EvalClassVarsChild extends EvalClassVarsBase {
    use EvalClassVarsTrait;
    public $childPublic = "cp";
    protected $childProtected = "cq";
    private $childPrivate = "cs";
    public function childView() {
        $vars = get_class_vars(self::class);
        ksort($vars);
        foreach ($vars as $name => $value) {
            echo $name . "=" . (is_null($value) ? "null" : $value) . "|";
        }
    }
    public function baseView() {
        $vars = get_class_vars(EvalClassVarsBase::class);
        ksort($vars);
        foreach ($vars as $name => $value) {
            echo $name . "=" . (is_null($value) ? "null" : $value) . "|";
        }
    }
}
$outside = get_class_vars("EvalClassVarsChild");
ksort($outside);
foreach ($outside as $name => $value) { echo $name . "=" . (is_null($value) ? "null" : $value) . "|"; }
echo ":";
(new EvalClassVarsChild())->childView();
echo ":";
(new EvalClassVarsChild())->baseView();
echo ":";
$trait = call_user_func("get_class_vars", "EvalClassVarsTrait");
ksort($trait);
foreach ($trait as $name => $value) { echo $name . "=" . (is_null($value) ? "null" : $value) . "|"; }
echo ":";
$enum = call_user_func_array("get_class_vars", ["class" => "EvalClassVarsBacked"]);
ksort($enum);
foreach ($enum as $name => $value) { echo $name . "=" . (is_null($value) ? "null" : $value) . "|"; }
echo ":";
echo function_exists("get_class_vars") ? "F" : "f";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "basePublic=bp|baseStatic=static|childPublic=cp|traitPublic=tp|typed=null|:baseProtected=bq|basePublic=bp|baseStatic=static|childPrivate=cs|childProtected=cq|childPublic=cp|traitProtected=tq|traitPublic=tp|typed=null|:baseProtected=bq|basePublic=bp|baseStatic=static|typed=null|:traitPublic=tp|:name=null|value=null|:F"
    );
}

/// Verifies eval `get_class_vars()` exposes generated/AOT class defaults by scope.
#[test]
fn test_eval_get_class_vars_exposes_aot_defaults_by_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotClassVarsBase {
    public $basePublic = "bp";
    protected $baseProtected = "bq";
    private $basePrivate = "bs";
    public static $baseStatic = "static";
    public array $baseArray = ["a" => 1];
    public int $baseTyped;
    public ?int $baseNullable = null;
    public function parentView() {
        return eval('$vars = get_class_vars(EvalAotClassVarsChild::class);
ksort($vars);
foreach ($vars as $name => $value) {
    $rendered = is_array($value) ? $value["a"] : (is_null($value) ? "null" : $value);
    echo $name . "=" . $rendered . "|";
}');
    }
}
class EvalAotClassVarsChild extends EvalAotClassVarsBase {
    public $childPublic = "cp";
    protected $childProtected = "cq";
    private $childPrivate = "cs";
    public static $childStatic = "childStatic";
    public function childView() {
        return eval('$vars = get_class_vars(self::class);
ksort($vars);
foreach ($vars as $name => $value) {
    $rendered = is_array($value) ? $value["a"] : (is_null($value) ? "null" : $value);
    echo $name . "=" . $rendered . "|";
}');
    }
}
eval('$outside = get_class_vars(EvalAotClassVarsChild::class);
ksort($outside);
foreach ($outside as $name => $value) {
    $rendered = is_array($value) ? $value["a"] : (is_null($value) ? "null" : $value);
    echo $name . "=" . $rendered . "|";
}
echo ":";');
(new EvalAotClassVarsChild())->childView();
echo ":";
(new EvalAotClassVarsChild())->parentView();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "baseArray=1|baseNullable=null|basePublic=bp|baseStatic=static|baseTyped=null|childPublic=cp|childStatic=childStatic|:baseArray=1|baseNullable=null|baseProtected=bq|basePublic=bp|baseStatic=static|baseTyped=null|childPrivate=cs|childProtected=cq|childPublic=cp|childStatic=childStatic|:baseArray=1|baseNullable=null|basePrivate=bs|baseProtected=bq|basePublic=bp|baseStatic=static|baseTyped=null|childProtected=cq|childPublic=cp|childStatic=childStatic|"
    );
}

/// Verifies eval OOP introspection builtins honor AOT inherited private-member rules.
#[test]
fn test_eval_oop_introspection_builtins_for_aot_inherited_private_members() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotIntrospectBase {
    private int $baseSecret;
    protected int $baseProtectedProp;
    public int $basePublicProp;
    private function basePrivate() {}
    protected function baseProtected() {}
    public function basePublic() {}
}
class EvalAotIntrospectChild extends EvalAotIntrospectBase {
    private int $childSecret;
    public int $childPublicProp;
    private function childPrivate() {}
    public function childPublic() {}
}

eval('$object = new EvalAotIntrospectChild();
echo method_exists("EvalAotIntrospectChild", "basePrivate") ? "bad" : "noClassParentPrivateMethod"; echo ":";
echo method_exists($object, "basePrivate") ? "objectParentPrivateMethod" : "bad"; echo ":";
echo method_exists("EvalAotIntrospectChild", "baseProtected") ? "classProtectedMethod" : "bad"; echo ":";
echo method_exists($object, "childPrivate") ? "objectChildPrivateMethod" : "bad"; echo ":";
echo property_exists("EvalAotIntrospectChild", "baseSecret") ? "bad" : "noClassParentPrivateProperty"; echo ":";
echo property_exists($object, "baseSecret") ? "bad" : "noObjectParentPrivateProperty"; echo ":";
echo property_exists("EvalAotIntrospectChild", "baseProtectedProp") ? "classProtectedProperty" : "bad"; echo ":";
echo property_exists($object, "childSecret") ? "objectChildPrivateProperty" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "noClassParentPrivateMethod:objectParentPrivateMethod:classProtectedMethod:objectChildPrivateMethod:noClassParentPrivateProperty:noObjectParentPrivateProperty:classProtectedProperty:objectChildPrivateProperty"
    );
}

/// Verifies eval `property_exists()` exposes parent private object properties only from parent scope.
#[test]
fn test_eval_property_exists_exposes_declared_parent_private_object_property_by_scope() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalPropertyExistsScopeBase {
    private $basePrivate;
    private static $baseStatic;
    public function parentView() {
        echo property_exists($this, "basePrivate") ? "parentPrivate" : "bad";
        echo ",";
        echo property_exists($this, "baseStatic") ? "bad" : "noParentStatic";
        echo ",";
        echo property_exists(EvalPropertyExistsScopeChild::class, "basePrivate") ? "bad" : "noClassPrivate";
    }
}
class EvalPropertyExistsScopeChild extends EvalPropertyExistsScopeBase {
    public function childView() {
        echo property_exists($this, "basePrivate") ? "bad" : "noChildParentPrivate";
    }
}
$object = new EvalPropertyExistsScopeChild();
echo property_exists($object, "basePrivate") ? "bad" : "noOutsideObject";
echo ":";
echo property_exists(EvalPropertyExistsScopeChild::class, "basePrivate") ? "bad" : "noOutsideClass";
echo ":";
$object->childView();
echo ":";
$object->parentView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "noOutsideObject:noOutsideClass:noChildParentPrivate:parentPrivate,noParentStatic,noClassPrivate"
    );
}

/// Verifies eval `property_exists()` applies parent private object-property scope to AOT metadata.
#[test]
fn test_eval_property_exists_exposes_aot_parent_private_object_property_by_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotPropertyExistsScopeBase {
    private $basePrivate;
    private static $baseStatic;
    public function parentView() {
        return eval('return (property_exists($this, "basePrivate") ? "parentPrivate" : "bad")
            . "," . (property_exists($this, "baseStatic") ? "bad" : "noParentStatic")
            . "," . (property_exists(EvalAotPropertyExistsScopeChild::class, "basePrivate") ? "bad" : "noClassPrivate");');
    }
}
class EvalAotPropertyExistsScopeChild extends EvalAotPropertyExistsScopeBase {
    public function childView() {
        return eval('return property_exists($this, "basePrivate") ? "bad" : "noChildParentPrivate";');
    }
}
eval('$object = new EvalAotPropertyExistsScopeChild();
echo property_exists($object, "basePrivate") ? "bad" : "noOutsideObject";
echo ":";
echo property_exists(EvalAotPropertyExistsScopeChild::class, "basePrivate") ? "bad" : "noOutsideClass";
echo ":";
echo $object->childView();
echo ":";
echo $object->parentView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "noOutsideObject:noOutsideClass:noChildParentPrivate:parentPrivate,noParentStatic,noClassPrivate"
    );
}

/// Verifies eval `get_class_methods()` follows PHP scope visibility for eval-declared metadata.
#[test]
fn test_eval_get_class_methods_exposes_declared_methods_by_scope() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalClassMethodsBase {
    private function basePrivate() {}
    protected function baseProtected() {}
    public function basePublic() {}
    public function parentView() {
        $methods = get_class_methods(EvalClassMethodsChild::class);
        sort($methods);
        echo implode(",", $methods);
    }
}
class EvalClassMethodsChild extends EvalClassMethodsBase {
    private function childPrivate() {}
    protected static function childProtectedStatic() {}
    public function childPublic() {}
    public function childView() {
        $methods = get_class_methods($this);
        sort($methods);
        echo implode(",", $methods);
    }
}
$outside = get_class_methods("EvalClassMethodsChild");
sort($outside);
echo implode(",", $outside);
echo ":";
(new EvalClassMethodsChild())->childView();
echo ":";
(new EvalClassMethodsChild())->parentView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "basePublic,childPublic,childView,parentView:baseProtected,basePublic,childPrivate,childProtectedStatic,childPublic,childView,parentView:basePrivate,baseProtected,basePublic,childProtectedStatic,childPublic,childView,parentView"
    );
}

/// Verifies eval `get_class_methods()` follows PHP scope visibility for generated/AOT metadata.
#[test]
fn test_eval_get_class_methods_exposes_aot_methods_by_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotClassMethodsBase {
    private function basePrivate() {}
    protected function baseProtected() {}
    public function basePublic() {}
    public static function baseStaticPublic() {}
    public function parentView() {
        return eval('$methods = get_class_methods(EvalAotClassMethodsChild::class);
sort($methods);
echo implode(",", $methods);');
    }
}
class EvalAotClassMethodsChild extends EvalAotClassMethodsBase {
    private function childPrivate() {}
    protected static function childProtectedStatic() {}
    public function childPublic() {}
    public function childView() {
        return eval('$methods = get_class_methods($this);
sort($methods);
echo implode(",", $methods);');
    }
}
eval('$outside = get_class_methods("EvalAotClassMethodsChild");
sort($outside);
echo implode(",", $outside);
echo ":";');
(new EvalAotClassMethodsChild())->childView();
echo ":";
(new EvalAotClassMethodsChild())->parentView();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "basepublic,basestaticpublic,childpublic,childview,parentview:baseprotected,basepublic,basestaticpublic,childprivate,childprotectedstatic,childpublic,childview,parentview:baseprivate,baseprotected,basepublic,basestaticpublic,childprotectedstatic,childpublic,childview,parentview"
    );
}

/// Verifies eval `get_object_vars()` skips uninitialized typed properties like PHP.
#[test]
fn test_eval_get_object_vars_skips_uninitialized_declared_properties() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalUninitializedObjectVars {
    public int $a;
    public ?int $b = null;
}
$object = new EvalUninitializedObjectVars();
echo property_exists($object, "a") ? "PA" : "pa"; echo ":";
$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
$object->a = 5;
$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
unset($object->a);
$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars));');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PA:b:a,b:b");
}

/// Verifies direct reads of uninitialized eval-declared typed properties throw PHP errors.
#[test]
fn test_eval_uninitialized_typed_instance_property_reads_throw_error() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalUninitializedTypedRead {
    public int $typed;
    public ?int $nullable;
    public ?int $defaultNull = null;
    public $plain;
}
$object = new EvalUninitializedTypedRead();
try {
    echo $object->typed;
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    echo $object->nullable;
} catch (Error $e) {
    echo $e->getMessage();
}
echo "|";
echo is_null($object->defaultNull) ? "default-null" : "bad";
echo "|";
echo is_null($object->plain) ? "plain-null" : "bad";
echo "|";
$object->typed = 0;
echo $object->typed;
echo "|";
unset($object->typed);
try {
    echo $object->typed;
} catch (Error $e) {
    echo $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Error:Typed property EvalUninitializedTypedRead::$typed must not be accessed before initialization|\
Typed property EvalUninitializedTypedRead::$nullable must not be accessed before initialization|\
default-null|plain-null|0|\
Typed property EvalUninitializedTypedRead::$typed must not be accessed before initialization"
    );
}

/// Verifies eval `get_object_vars()` exposes initialized generated/AOT properties by scope.
#[test]
fn test_eval_get_object_vars_exposes_initialized_aot_properties_by_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotObjectVarsBase {
    private $baseSecret = "bp";
    protected $baseProtected = "bq";
    public $basePublic = "br";
    public int $baseTyped;
    public ?int $baseNullable = null;
    public function parentView() {
        return eval('$vars = get_object_vars($this);
ksort($vars);
return implode(",", array_keys($vars));');
    }
}
class EvalAotObjectVarsChild extends EvalAotObjectVarsBase {
    private $childSecret = "cp";
    protected $childProtected = "cq";
    public $childPublic = "cr";
    public int $childTyped;
    public ?string $childNullable = null;
    public $implicit;
    public static $static = "s";
    public function childView() {
        return eval('$vars = get_object_vars($this);
ksort($vars);
return implode(",", array_keys($vars));');
    }
}
$object = new EvalAotObjectVarsChild();
eval('$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
echo $object->childView(); echo ":";
echo $object->parentView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "baseNullable,basePublic,childNullable,childPublic,implicit:baseNullable,baseProtected,basePublic,childNullable,childProtected,childPublic,childSecret,implicit:baseNullable,baseProtected,basePublic,baseSecret,childNullable,childProtected,childPublic,implicit"
    );
}

/// Verifies AOT `get_object_vars()` lets parent scopes see shadowed private slots.
#[test]
fn test_eval_get_object_vars_uses_aot_private_parent_shadowing_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotShadowVarsParent {
    private $value = "p";
    public function parentView() {
        return eval('$vars = get_object_vars($this);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
return $vars["value"];');
    }
}
class EvalAotShadowVarsChild extends EvalAotShadowVarsParent {
    public $value = "c";
    public function childView() {
        return eval('$vars = get_object_vars($this);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":";
return $vars["value"];');
    }
}
$object = new EvalAotShadowVarsChild();
eval('$vars = get_object_vars($object);
ksort($vars);
echo implode(",", array_keys($vars)); echo ":"; echo $vars["value"]; echo "|";
echo $object->childView(); echo "|";
echo $object->parentView();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "value:c|value:c|value:p");
}

/// Verifies eval-declared private parent properties keep separate storage when a child shadows them.
#[test]
fn test_eval_declared_private_parent_property_shadowing() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalShadowGrand {
    private $value = 1;
    public function grandValue() { return $this->value; }
}
class EvalShadowParent extends EvalShadowGrand {
    public $value = 2;
    public function parentValue() { return $this->value; }
}
class EvalShadowChild extends EvalShadowParent {
    public $value = 3;
}
$box = new EvalShadowChild();
echo $box->grandValue() . ":";
echo $box->parentValue() . ":";
echo $box->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1:3:3");
}

/// Verifies eval-declared readonly properties can be initialized only in constructors.
#[test]
fn test_eval_declared_readonly_property_rules() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReadonlyBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$box = new EvalReadonlyBox(7);
echo $box->id();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7");

    let err = compile_and_run_capture(
        r#"<?php
eval('class EvalReadonlyFailBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyFailBox(7);
try {
    $box->replace(8);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        err.success,
        "program failed: stdout={:?} stderr={}",
        err.stdout, err.stderr
    );
    assert_eq!(
        err.stdout,
        "Error:Cannot modify readonly property EvalReadonlyFailBox::$id"
    );

    let unset = compile_and_run_capture(
        r#"<?php
eval('class EvalReadonlyUnsetBox {
    public readonly int $id;
    public function __construct($id) { $this->id = $id; }
}
$box = new EvalReadonlyUnsetBox(7);
try {
    unset($box->id);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        unset.success,
        "program failed: stdout={:?} stderr={}",
        unset.stdout, unset.stderr
    );
    assert_eq!(
        unset.stdout,
        "Error:Cannot unset readonly property EvalReadonlyUnsetBox::$id"
    );
}

/// Verifies eval-declared readonly classes initialize typed properties and can inherit readonly parents.
#[test]
fn test_eval_declared_readonly_class_initializes_and_inherits() {
    let out = compile_and_run_capture(
        r#"<?php
eval('readonly class EvalReadonlyClassBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
readonly class EvalReadonlyClassChild extends EvalReadonlyClassBox {}
$box = new EvalReadonlyClassBox(7);
$child = new EvalReadonlyClassChild(9);
echo $box->id() . ":" . $child->id();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:9");
}

/// Verifies eval-declared readonly classes leave static properties mutable.
#[test]
fn test_eval_declared_readonly_class_static_properties_remain_mutable() {
    let static_out = compile_and_run_capture(
        r#"<?php
eval('readonly class EvalReadonlyStaticPropertyBox {
    public static int $count = 1;
}
EvalReadonlyStaticPropertyBox::$count = EvalReadonlyStaticPropertyBox::$count + 1;
echo EvalReadonlyStaticPropertyBox::$count;');
"#,
    );
    assert!(
        static_out.success,
        "program failed: stdout={:?} stderr={}",
        static_out.stdout, static_out.stderr
    );
    assert_eq!(static_out.stdout, "2");
}

/// Verifies eval-declared readonly classes reject untyped instance properties.
#[test]
fn test_eval_declared_readonly_class_rejects_untyped_property() {
    let untyped_err = compile_and_run_expect_failure(
        r#"<?php
eval('readonly class EvalReadonlyUntypedPropertyBox {
    public $id;
}');
"#,
    );
    assert!(
        untyped_err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {untyped_err}"
    );
}

/// Verifies eval-declared readonly class properties reject writes after construction.
#[test]
fn test_eval_declared_readonly_class_rejects_second_write() {
    let err = compile_and_run_capture(
        r#"<?php
eval('readonly class EvalReadonlyClassFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
    public function replace($id) { $this->id = $id; }
}
$box = new EvalReadonlyClassFailBox(7);
try {
    $box->replace(8);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        err.success,
        "program failed: stdout={:?} stderr={}",
        err.stdout, err.stderr
    );
    assert_eq!(
        err.stdout,
        "Error:Cannot modify readonly property EvalReadonlyClassFailBox::$id"
    );
}

/// Verifies eval-declared readonly classes reject dynamic property creation.
#[test]
fn test_eval_declared_readonly_class_rejects_dynamic_property() {
    let dynamic_err = compile_and_run_capture(
        r#"<?php
eval('readonly class EvalReadonlyClassDynamicFailBox {
    public int $id;
    public function __construct($id) { $this->id = $id; }
}
$box = new EvalReadonlyClassDynamicFailBox(7);
try {
    $box->dynamic = 8;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        dynamic_err.success,
        "program failed: stdout={:?} stderr={}",
        dynamic_err.stdout, dynamic_err.stderr
    );
    assert_eq!(
        dynamic_err.stdout,
        "Error:Cannot create dynamic property EvalReadonlyClassDynamicFailBox::$dynamic"
    );
}

/// Verifies eval-declared readonly classes reject the global `AllowDynamicProperties` marker.
#[test]
fn test_eval_declared_readonly_class_rejects_allow_dynamic_properties() {
    let attribute_err = compile_and_run_expect_failure(
        r#"<?php
eval('#[\AllowDynamicProperties] readonly class EvalReadonlyAllowDynamicAttrBox {}');
"#,
    );
    assert!(
        attribute_err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {attribute_err}"
    );
}

/// Verifies eval-declared readonly classes may route missing writes through `__set`.
#[test]
fn test_eval_declared_readonly_class_allows_magic_set() {
    let magic = compile_and_run_capture(
        r#"<?php
eval('readonly class EvalReadonlyClassMagicSetBox {
    public function __set($name, $value) {
        echo $name . ":" . $value;
    }
}
$box = new EvalReadonlyClassMagicSetBox();
$box->dynamic = 8;');
"#,
    );
    assert!(
        magic.success,
        "program failed: stdout={:?} stderr={}",
        magic.stdout, magic.stderr
    );
    assert_eq!(magic.stdout, "dynamic:8");
}

/// Verifies eval-declared readonly classes cannot extend non-readonly parents.
#[test]
fn test_eval_declared_readonly_class_rejects_non_readonly_parent() {
    let parent_err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReadonlyClassBase {}
readonly class EvalReadonlyClassChild extends EvalReadonlyClassBase {}');
"#,
    );
    assert!(
        parent_err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {parent_err}"
    );
}

/// Verifies eval-declared property hooks route get/set access through accessors.
#[test]
fn test_eval_declared_property_hooks() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalHookName {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalHookChild extends EvalHookName {
    public function shout() { return $this->value . "?"; }
}
$box = new EvalHookChild();
$box->value = "Ada";
echo $box->value . ":" . $box->shout();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada!:Ada!?");

    let err = compile_and_run_capture(
        r#"<?php
eval('class EvalHookReadOnly {
    public int $answer {
        get => 42;
    }
}
$box = new EvalHookReadOnly();
try {
    $box->answer = 7;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        err.success,
        "program failed: stdout={:?} stderr={}",
        err.stdout, err.stderr
    );
    assert_eq!(
        err.stdout,
        "Error:Property EvalHookReadOnly::$answer is read-only"
    );
}

/// Verifies eval-declared by-reference get hook syntax reads through the accessor.
#[test]
fn test_eval_declared_by_ref_get_property_hook() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalByRefGetHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        &get => $this->first . " " . $this->last;
    }
}
$person = new EvalByRefGetHookPerson();
echo $person->full;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada Lovelace");
}

/// Verifies eval-declared short set property hooks store their expression result.
#[test]
fn test_eval_declared_short_set_property_hooks() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalShortSetHookName {
    public string $value {
        get => $this->value;
        set => trim($value);
    }
}
class EvalShortSetHookLabel {
    public string $text {
        get => $this->text;
        set(string $raw) => strtoupper($raw);
    }
}
$name = new EvalShortSetHookName();
$name->value = "  Ada  ";
echo "[" . $name->value . "]:";
$label = new EvalShortSetHookLabel();
$label->text = "hi";
echo $label->text;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "[Ada]:HI");
}

/// Verifies eval-declared set-hook parameter types stay compatible with property writes.
#[test]
fn test_eval_declared_property_set_hook_parameter_type_compatibility() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalWideSetHookParam {
    public string $value {
        get => $this->value;
        set(mixed $raw) => $raw;
    }
}
$box = new EvalWideSetHookParam();
$box->value = "Ada";
echo $box->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada");

    for source in [
        r#"<?php
eval('class EvalUntypedExplicitSetHookParam {
    public string $value {
        set($raw) => $raw;
    }
}');
"#,
        r#"<?php
eval('class EvalNarrowSetHookParam {
    public mixed $value {
        set(string $raw) => $raw;
    }
}');
"#,
    ] {
        let err = compile_and_run_expect_failure(source);
        assert!(
            err.contains("Fatal error: eval()"),
            "stderr did not contain eval fatal diagnostic: {err}"
        );
    }
}

/// Verifies eval-declared nullsafe and mixed-case property hook reads stay routed.
#[test]
fn test_eval_declared_nullsafe_and_mixed_case_property_hooks() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalNullsafeHookPerson {
    public string $first = "Ada";
    public string $last = "Lovelace";
    public string $full {
        get => $this->first . " " . $this->last;
    }
}
class EvalMixedCaseHookBox {
    private int $store = 0;
    public int $Total {
        get { return $this->store; }
    }
    public function set(int $value) { $this->store = $value; }
}
function eval_hook_describe($person) {
    return $person?->full ?? "(none)";
}
$person = new EvalNullsafeHookPerson();
$box = new EvalMixedCaseHookBox();
$box->set(5);
echo eval_hook_describe($person) . "|" . eval_hook_describe(null) . "|" . $box->Total;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada Lovelace|(none)|5");
}

/// Verifies eval-declared magic property methods handle missing and inaccessible properties.
#[test]
fn test_eval_declared_magic_property_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicPropertyBox {
    private string $secret = "raw";
    public string $events = "";
    public function readOwn() { return $this->secret; }
    protected function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return "read:" . $name;
    }
    private function __set($name, $value) {
        $this->events = $this->events . "set:" . $name . "=" . $value . ";";
    }
}
$box = new EvalMagicPropertyBox();
echo $box->readOwn() . ":";
echo $box->secret . ":";
echo $box->missing . ":";
$box->secret = "new";
$box->other = "B";
$box->events = $box->events . "public;";
echo $box->events;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "raw:read:secret:read:missing:get:secret;get:missing;set:secret=new;set:other=B;public;"
    );
}

/// Verifies eval reads existing dynamic properties before falling back to `__get`.
#[test]
fn test_eval_declared_magic_get_preserves_existing_dynamic_property() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicExistingDynamicBox {
    public function __get($name) {
        return "magic:" . $name;
    }
}
$box = new EvalMagicExistingDynamicBox();
$box->known = "plain";
echo $box->known . ":";
echo $box->missing;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "plain:magic:missing");
}

/// Verifies eval property probes and unsets dispatch through `__isset` and `__unset`.
#[test]
fn test_eval_declared_magic_isset_empty_and_unset_property_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicPropertyProbeBox {
    public string $events = "";
    public string $present = "ready";
    public $nullish = null;
    private string $secret = "raw";
    private function __isset($name) {
        $this->events = $this->events . "isset:" . $name . ";";
        return $name !== "no";
    }
    protected function __get($name) {
        $this->events = $this->events . "get:" . $name . ";";
        return $name === "empty" ? "" : "value:" . $name;
    }
    private function __unset($name) {
        $this->events = $this->events . "unset:" . $name . ";";
    }
}
$box = new EvalMagicPropertyProbeBox();
echo isset($box->present) ? "P" : "p"; echo ":";
echo isset($box->nullish) ? "N" : "n"; echo ":";
echo isset($box->secret) ? "S" : "s"; echo ":";
echo isset($box->no) ? "bad" : "no"; echo ":";
echo empty($box->secret) ? "bad" : "filled"; echo ":";
echo empty($box->empty) ? "empty" : "bad"; echo ":";
unset($box->present);
unset($box->secret);
unset($box->missing);
echo isset($box->present) ? "bad" : "unset"; echo ":";
echo $box->events;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "P:n:S:no:filled:empty:unset:isset:secret;isset:no;isset:secret;get:secret;isset:empty;get:empty;unset:secret;unset:missing;"
    );
}

/// Verifies eval-declared interface property hook contracts validate class properties.
#[test]
fn test_eval_declared_interface_property_hook_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalIfaceHookContract {
    public string $value { get; set; }
}
interface EvalIfaceNamedHookContract extends EvalIfaceHookContract {
    public string $name { get; }
}
class EvalIfaceHookBox implements EvalIfaceNamedHookContract {
    public string $name = "box";
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalIfacePlainBox implements EvalIfaceHookContract {
    public string $value = "Grace";
}
$box = new EvalIfaceHookBox();
$box->value = "Ada";
$plain = new EvalIfacePlainBox();
echo $box->name . ":" . $box->value . ":" . $plain->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "box:Ada!:Grace");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalIfaceHookSetContract {
    public int $answer { get; set; }
}
class EvalIfaceHookReadOnlyBox implements EvalIfaceHookSetContract {
    public int $answer {
        get => 42;
    }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalIfaceGetInt {
    public int $value { get; }
}
abstract class EvalIfaceGetWideBad implements EvalIfaceGetInt {
    abstract public int|string $value { get; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalIfaceSetWide {
    public int|string $value { set; }
}
abstract class EvalIfaceSetNarrowBad implements EvalIfaceSetWide {
    abstract public int $value { set; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalIfaceInheritedGet {
    public int $value { get; }
}
abstract class EvalIfaceInheritedPropertyBase {
    public string $value = "bad";
}
abstract class EvalIfaceInheritedPropertyChild extends EvalIfaceInheritedPropertyBase implements EvalIfaceInheritedGet {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects PHP-forbidden callable/static type atoms by declaration position.
#[test]
fn test_eval_rejects_invalid_property_and_parameter_type_atoms() {
    for source in [
        r#"<?php
eval('class EvalBadCallableProperty {
    public callable $value;
}');
"#,
        r#"<?php
eval('interface EvalBadCallableInterfaceProperty {
    public callable $value { get; }
}');
"#,
        r#"<?php
eval('class EvalBadCallablePromoted {
    public function __construct(public callable $value) {}
}');
"#,
        r#"<?php
eval('function eval_bad_static_parameter(static $value) {}');
"#,
        r#"<?php
eval('function eval_bad_self_return(): self {}');
"#,
        r#"<?php
eval('function eval_bad_static_return(): static {}');
"#,
        r#"<?php
eval('class EvalBadStaticMethodParam {
    public function read(static $value) {}
}');
"#,
        r#"<?php
eval('class EvalBadStaticPromoted {
    public function __construct(public static $value) {}
}');
"#,
    ] {
        let err = compile_and_run_expect_failure(source);
        assert!(
            err.contains("Fatal error: eval() fragment uses an unsupported construct"),
            "stderr did not contain eval unsupported-construct diagnostic: {err}"
        );
    }
}

/// Verifies eval-declared plain abstract properties can be concretized by child storage.
#[test]
fn test_eval_declared_plain_abstract_property_concretization() {
    let out = compile_and_run(
        r#"<?php
eval('abstract class EvalPlainAbstractShape {
    abstract public int $sides { get; set; }

    public function show() {
        return $this->sides;
    }
}
abstract class EvalPlainAbstractPolygon extends EvalPlainAbstractShape {}
class EvalPlainAbstractSquare extends EvalPlainAbstractPolygon {
    public int $sides = 4;
}

abstract class EvalPlainAbstractEntity {
    abstract public int $id { get; set; }
}
class EvalPlainAbstractUser extends EvalPlainAbstractEntity {
    public function __construct(public int $id) {}
}

abstract class EvalPlainAbstractReadonlyBase {
    abstract public int $value { get; }
}
class EvalPlainAbstractReadonlyBox extends EvalPlainAbstractReadonlyBase {
    public readonly int $value;

    public function __construct(int $value) {
        $this->value = $value;
    }
}

$shape = new EvalPlainAbstractSquare();
$user = new EvalPlainAbstractUser(7);
$box = new EvalPlainAbstractReadonlyBox(42);
echo $shape->show() . ":" . $user->id . ":" . $box->value;');
"#,
    );
    assert_eq!(out, "4:7:42");
}

/// Verifies eval-declared abstract property hook contracts validate concrete subclasses.
#[test]
fn test_eval_declared_abstract_property_hook_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalAbstractHookBase {
    abstract public string $value { get; set; }
}
class EvalAbstractHookBox extends EvalAbstractHookBase {
    public string $value {
        get => $this->value;
        set { $this->value = $value . "!"; }
    }
}
class EvalPlainAbstractHookBox extends EvalAbstractHookBase {
    public string $value = "Grace";
}
$box = new EvalAbstractHookBox();
$box->value = "Ada";
$plain = new EvalPlainAbstractHookBox();
echo $box->value . ":" . $plain->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada!:Grace");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('abstract class EvalMissingAbstractHookBase {
    abstract public string $value { get; }
}
class EvalMissingAbstractHookBox extends EvalMissingAbstractHookBase {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared static properties and static methods work through the bridge.
#[test]
fn test_eval_declared_static_members_and_late_static_binding() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalStaticCounter {
    public static int $count = 1;
    public static function bump($step) {
        self::$count += $step;
        return self::$count;
    }
}
class EvalStaticBase {
    protected static int $n = 2;
    public static function add($x) {
        static::$n += $x;
        return static::$n;
    }
    public static function baseRead() {
        return self::$n;
    }
}
class EvalStaticChild extends EvalStaticBase {
    protected static int $n = 10;
}
echo EvalStaticCounter::$count . ":";
echo EvalStaticCounter::bump(2) . ":";
echo EvalStaticCounter::$count . ":";
echo EvalStaticChild::add(4) . ":";
echo EvalStaticBase::add(3) . ":";
echo EvalStaticBase::baseRead();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1:3:3:14:5:5");

    let errors = compile_and_run_capture(
        r#"<?php
eval('class EvalInvalidStaticPropBox {
    public int $instance = 1;
    public static int $typed;
}
try {
    EvalInvalidStaticPropBox::$missing;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalInvalidStaticPropBox::$instance;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalInvalidStaticPropBox::$typed;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalInvalidStaticPropBox::$missing = 9;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        errors.success,
        "program failed: stdout={:?} stderr={}",
        errors.stdout, errors.stderr
    );
    assert_eq!(
        errors.stdout,
        "Error:Access to undeclared static property EvalInvalidStaticPropBox::$missing|\
Error:Access to undeclared static property EvalInvalidStaticPropBox::$instance|\
Error:Typed static property EvalInvalidStaticPropBox::$typed must not be accessed before initialization|\
Error:Access to undeclared static property EvalInvalidStaticPropBox::$missing"
    );
}

/// Verifies invalid eval-declared static method calls throw catchable Error objects.
#[test]
fn test_eval_declared_invalid_static_method_calls_throw_error() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalInvalidStaticCallBox {
    public function read() { return 1; }
}
class EvalMissingStaticCallBox {}
abstract class EvalAbstractStaticCallBox {
    abstract public static function abs();
}
try {
    EvalInvalidStaticCallBox::read();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalMissingStaticCallBox::missing();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    EvalAbstractStaticCallBox::abs();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Error:Non-static method EvalInvalidStaticCallBox::read() cannot be called statically|\
Error:Call to undefined method EvalMissingStaticCallBox::missing()|\
Error:Cannot call abstract method EvalAbstractStaticCallBox::abs()"
    );
}

/// Verifies eval-declared static interface methods are validated and reflected.
#[test]
fn test_eval_declared_static_interface_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalStaticContract {
    public static function make($value);
}
class EvalStaticContractImpl implements EvalStaticContract {
    public static function make($value) {
        return "S:" . $value;
    }
}
echo EvalStaticContractImpl::make("box") . ":";
$listed = (new ReflectionClass(EvalStaticContract::class))->getMethods()[0];
echo $listed->getName() . ":";
echo $listed->isStatic() ? "static" : "instance";
echo ":";
$method = new ReflectionMethod(EvalStaticContract::class, "make");
echo $method->isStatic() ? "S" : "s";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "S:box:make:static:S");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalStaticMismatch {
    public static function read();
}
class EvalStaticMismatchImpl implements EvalStaticMismatch {
    public function read() {}
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared constructors and methods bind named arguments.
#[test]
fn test_eval_declared_method_named_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalNamedMethodBox {
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function read($left, $right) {
        return $this->label . ":" . $left . ":" . $right;
    }
    public static function join($left, $right) {
        return $left . "-" . $right;
    }
}
$box = new EvalNamedMethodBox(right: "B", left: "A");
echo $box->read(right: "D", left: "C") . ":";
$args = ["right" => "F", "left" => "E"];
echo $box->read(...$args) . ":";
echo EvalNamedMethodBox::join(right: "H", left: "G");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AB:C:D:AB:E:F:G-H");
}

/// Verifies eval-declared constructors and methods bind constant-expression defaults.
#[test]
fn test_eval_declared_method_constant_default_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('define("EVAL_METHOD_DEFAULT_GLOBAL", "G");
class EvalDefaultConstBase {
    const LABEL = "base";
}
interface EvalDefaultConstIface {
    const WORD = "iface";
}
class EvalDefaultConstDep {
    public function __construct($label = "dep") {
        $this->label = $label;
    }
    public function read() {
        return $this->label;
    }
}
class EvalDefaultConstBox extends EvalDefaultConstBase {
    const LABEL = "box";
    public function __construct($label = self::LABEL) {
        $this->label = $label;
    }
    public function read($global = EVAL_METHOD_DEFAULT_GLOBAL, $parent = parent::LABEL, $iface = EvalDefaultConstIface::WORD, $class = self::class, $parentClass = parent::class, $items = [self::LABEL => 1 + 2, "fallback" => null ?? "fallback"], $method = __METHOD__, $dep = new EvalDefaultConstDep(label: "dep"), $clone = new self("inner")) {
        return $this->label . ":" . $global . ":" . $parent . ":" . $iface . ":" . $class . ":" . $parentClass . ":" . $items[self::LABEL] . ":" . $items["fallback"] . ":" . $method . ":" . $dep->read() . ":" . $clone->label;
    }
    public static function join($label = self::LABEL, $parent = parent::LABEL) {
        return $label . "-" . $parent;
    }
}
$box = new EvalDefaultConstBox();
echo $box->read() . ":";
echo EvalDefaultConstBox::join();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "box:G:base:iface:EvalDefaultConstBox:EvalDefaultConstBase:3:fallback:EvalDefaultConstBox::read:dep:inner:box-base"
    );
}

/// Verifies eval-declared constructors and methods bind variadic arguments.
#[test]
fn test_eval_declared_method_variadic_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalVariadicMethodBox {
    public function __construct(...$parts) {
        $this->label = $parts[0] . $parts["right"];
    }
    public function read($head, ...$tail) {
        echo count($tail) . ":";
        return $this->label . ":" . $head . ":" . $tail[0] . ":" . $tail["named"] . ":" . $tail["tail"];
    }
    public static function join(...$items) {
        return $items[0] . $items[1];
    }
}
$box = new EvalVariadicMethodBox("A", right: "B");
echo $box->read("C", "D", named: "E", tail: "F") . ":";
echo EvalVariadicMethodBox::join("G", "H");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "3:AB:C:D:E:F:GH");
}

/// Verifies eval-declared method parameter type hints are enforced through the bridge.
#[test]
fn test_eval_declared_method_parameter_type_hints() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalTypedReadable {}
class EvalTypedDep implements EvalTypedReadable {}
class EvalTypedMethodBox {
    public function read(EvalTypedReadable $dep, int ...$items) {
        echo get_class($dep) . ":";
        return $items[0] + $items[1];
    }
}
$dep = new EvalTypedDep();
$box = new EvalTypedMethodBox();
echo $box->read($dep, "3", 4);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalTypedDep:7");
}

/// Verifies eval-declared methods write back by-reference arguments through compiled eval calls.
#[test]
fn test_eval_declared_method_by_ref_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalByRefMethodBox {
    public function __construct(&$value) {
        $value = $value . "-ctor";
    }
    public function change(&$value) {
        $value = $value . "-method";
    }
    public static function changeStatic(&$value) {
        $value = $value . "-static";
    }
    public function changeVariadic(&...$items) {
        $items[0] = $items[0] . "-variadic";
        $items["named"] = $items["named"] . "-named";
    }
}
class EvalByRefPropertyBox {
    public string $value = "D";
}
$ctor = "Z";
$box = new EvalByRefMethodBox($ctor);
$value = "A";
$box->change($value);
EvalByRefMethodBox::changeStatic($value);
$named = "B";
$box->changeVariadic($value, named: $named);
$items = ["k" => "C"];
$box->change($items["k"]);
$prop = new EvalByRefPropertyBox();
$box->change($prop->value);
$propName = "value";
$box->change($prop->{$propName});
echo $ctor . ":" . $value . ":" . $named . ":" . $items["k"] . ":" . $prop->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Z-ctor:A-method-static-variadic:B-named:C-method:D-method-method"
    );
}

/// Verifies eval-declared methods can mutate eval static properties passed by reference.
#[test]
fn test_eval_declared_method_by_ref_static_property_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotByRefStaticPropertyBox {
    public static $value = "aot";
}
eval('class EvalByRefStaticPropertyChanger {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function pair(&$left, &$right) {
        $left = "left";
        $right = "right";
        return $left;
    }
}
class EvalByRefStaticPropertyBox {
    public static $value = "old";
    public static $other = "second";
    public static $third = "third";
    private static $secret = "private";
    public static function updatePrivate($changer) {
        $changer->set(self::$secret, "secret");
        return self::$secret;
    }
}
$changer = new EvalByRefStaticPropertyChanger();
$changer->set(EvalByRefStaticPropertyBox::$value, "changed");
echo $changer->pair(EvalByRefStaticPropertyBox::$value, EvalByRefStaticPropertyBox::$value) . ":";
echo EvalByRefStaticPropertyBox::$value . ":";
$class = "EvalByRefStaticPropertyBox";
$changer->set($class::$other, "dynamic");
$name = "third";
$changer->set($class::${$name}, "name");
echo EvalByRefStaticPropertyBox::$other . ":";
echo EvalByRefStaticPropertyBox::$third . ":";
echo EvalByRefStaticPropertyBox::updatePrivate($changer) . ":";
$changer->set(EvalAotByRefStaticPropertyBox::$value, "aot-changed");
echo EvalAotByRefStaticPropertyBox::$value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "right:right:dynamic:name:secret:aot-changed");
}

/// Verifies eval methods mutate property array elements passed by reference.
#[test]
fn test_eval_declared_method_by_ref_property_array_element_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotByRefPropertyArrayElementBox {
    public static $items = ["aot" => "old"];
}
eval('class EvalByRefPropertyArrayElementChanger {
    public function set(&$value, $next) {
        $value = $next;
    }
    public function pair(&$left, &$right) {
        $left = "left";
        $right = "right";
        return $left;
    }
}
class EvalByRefPropertyArrayElementBox {
    public $items = ["first" => "old", "same" => "same"];
    public $other = null;
    public static $staticItems = ["first" => "static-old", "same" => "static-same"];
}
$changer = new EvalByRefPropertyArrayElementChanger();
$box = new EvalByRefPropertyArrayElementBox();
$changer->set($box->items["first"], "changed");
$name = "items";
$changer->set($box->{$name}["dynamic"], "dynamic");
$changer->set($box->other["created"], "created");
echo $box->items["first"] . ":" . $box->items["dynamic"] . ":" . $box->other["created"] . ":";
echo $changer->pair($box->items["same"], $box->items["same"]) . ":" . $box->items["same"] . ":";
$changer->set(EvalByRefPropertyArrayElementBox::$staticItems["first"], "static");
$class = "EvalByRefPropertyArrayElementBox";
$staticName = "staticItems";
$changer->set($class::${$staticName}["dynamic"], "static-dynamic");
echo EvalByRefPropertyArrayElementBox::$staticItems["first"] . ":";
echo EvalByRefPropertyArrayElementBox::$staticItems["dynamic"] . ":";
echo $changer->pair(
    EvalByRefPropertyArrayElementBox::$staticItems["same"],
    EvalByRefPropertyArrayElementBox::$staticItems["same"]
) . ":" . EvalByRefPropertyArrayElementBox::$staticItems["same"] . ":";
$changer->set(EvalAotByRefPropertyArrayElementBox::$items["aot"], "aot-changed");
echo EvalAotByRefPropertyArrayElementBox::$items["aot"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "changed:dynamic:created:right:right:static:static-dynamic:right:right:aot-changed"
    );
}

/// Verifies eval dynamic static callables dispatch eval-declared static methods.
#[test]
fn test_eval_declared_static_method_dynamic_callables() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalStaticCallableBox {
    public static function join($left, $right) {
        return $left . $right;
    }
}
$cb = ["EvalStaticCallableBox", "join"];
echo $cb(right: "B", left: "A") . ":";
echo call_user_func($cb, "C", "D") . ":";
echo call_user_func_array($cb, ["right" => "F", "left" => "E"]) . ":";
$named = "EvalStaticCallableBox::join";
echo $named(right: "H", left: "G");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "AB:CD:EF:GH");
}

/// Verifies eval first-class callable syntax dispatches functions and methods.
#[test]
fn test_eval_first_class_callables_dispatch_functions_and_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_fc_double($value) {
    return $value * 2;
}
class EvalFirstClassCallableBase {
    public function __construct($offset = 1) {
        $this->offset = $offset;
    }
    public function add($value) {
        return $value + $this->offset;
    }
    public function keep($value) {
        return $value > 2;
    }
    public function sum($carry, $value) {
        return $carry + $value + $this->offset;
    }
    public function show($value, $key) {
        echo $key . $value;
    }
    public static function join($left, $right) {
        return $left . $right;
    }
    public static function compareDesc($left, $right) {
        return $right - $left;
    }
    public static function label($value) {
        return "base:" . $value;
    }
    public static function relay($value) {
        $fn = static::label(...);
        return $fn($value);
    }
}
class EvalFirstClassCallableChild extends EvalFirstClassCallableBase {
    public static function label($value) {
        return "child:" . $value;
    }
}
$function = eval_fc_double(...);
echo $function(4) . ":";
echo (strlen(...))("abcd") . ":";
$box = new EvalFirstClassCallableBase(3);
$method = $box->add(...);
echo $method(4) . ":";
echo call_user_func($box->add(...), 5) . ":";
$static = EvalFirstClassCallableBase::join(...);
echo $static(right: "B", left: "A") . ":";
$mapped = array_map($box->add(...), [1, 2]);
echo $mapped[0] . $mapped[1] . ":";
$filtered = array_filter([1, 2, 3, 4], $box->keep(...));
echo count($filtered) . ":";
echo array_reduce([1, 2], $box->sum(...), 0) . ":";
        $walkedForWalk = ["a" => 1];
        array_walk($walkedForWalk, $box->show(...));
echo ":";
$sorted = [3, 1, 2];
usort($sorted, EvalFirstClassCallableBase::compareDesc(...));
echo $sorted[0] . $sorted[1] . $sorted[2] . ":";
echo EvalFirstClassCallableChild::relay("ok");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "8:4:7:8:AB:45:2:9:a1:321:child:ok");
}

/// Verifies eval first-class static callables preserve late-static forwarding metadata.
#[test]
fn test_eval_first_class_static_callables_preserve_called_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalFirstClassStaticForwardBase {
    public static function who() {
        return static::tag();
    }
    public static function tag() {
        return "base";
    }
    public static function relaySelf() {
        $fn = self::who(...);
        return $fn();
    }
}
class EvalFirstClassStaticForwardChild extends EvalFirstClassStaticForwardBase {
    public static function relayParent() {
        $fn = parent::who(...);
        return $fn();
    }
    public static function tag() {
        return "child";
    }
}
echo EvalFirstClassStaticForwardChild::relayParent() . ":";
echo EvalFirstClassStaticForwardChild::relaySelf();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "child:child");
}

/// Verifies eval `get_called_class()` follows late-static scopes and remains call-time scoped.
#[test]
fn test_eval_get_called_class_preserves_late_static_scope() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalGetCalledClassBase {
    public function instanceWho() {
        return get_called_class();
    }
    public function instanceCall() {
        return call_user_func("get_called_class");
    }
    public static function staticWho() {
        return get_called_class();
    }
    public static function staticCallArray() {
        return call_user_func_array("get_called_class", []);
    }
    public static function makeCallable() {
        return get_called_class(...);
    }
}
class EvalGetCalledClassChild extends EvalGetCalledClassBase {}
$child = new EvalGetCalledClassChild();
echo $child->instanceWho() . ":";
echo $child->instanceCall() . ":";
echo EvalGetCalledClassChild::staticWho() . ":";
echo EvalGetCalledClassChild::staticCallArray() . ":";
echo EvalGetCalledClassBase::staticWho() . ":";
try {
    get_called_class();
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
$fn = EvalGetCalledClassChild::makeCallable();
try {
    $fn();
} catch (Error $e) {
    echo "callable:";
}
echo function_exists("get_called_class") . ":" . is_callable("get_called_class");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalGetCalledClassChild:EvalGetCalledClassChild:EvalGetCalledClassChild:EvalGetCalledClassChild:EvalGetCalledClassBase:Error:get_called_class() must be called from within a class:callable:1:1"
    );
}

/// Verifies eval dynamic static receivers dispatch methods, properties, constants, and `::class`.
#[test]
fn test_eval_dynamic_static_receivers() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotDynamicStaticReceiver {
    public const KIND = "aot";
    public static $label = "A";
    public static function make($value) {
        return self::KIND . ":" . self::$label . ":" . $value;
    }
}
eval('class EvalDynamicStaticReceiver {
    public const KIND = "eval";
    public static $label = "E";
    public static function make($value) {
        return self::KIND . ":" . self::$label . ":" . $value;
    }
}
$evalClass = "EvalDynamicStaticReceiver";
$method = "make";
echo $evalClass::make("one"); echo "|";
echo $evalClass::$method("two"); echo "|";
echo $evalClass::$label; echo "|";
echo $evalClass::KIND; echo "|";
$prototype = new EvalDynamicStaticReceiver();
echo $prototype::make("object"); echo "|";
echo $prototype::class; echo "|";
$aotClass = "EvalAotDynamicStaticReceiver";
echo $aotClass::make("three"); echo "|";
echo $aotClass::$label; echo "|";
echo $aotClass::KIND;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "eval:E:one|eval:E:two|E|eval|eval:E:object|EvalDynamicStaticReceiver|aot:A:three|A|aot"
    );
}

/// Verifies eval dynamic static receivers write eval-declared and AOT static properties.
#[test]
fn test_eval_dynamic_static_receiver_property_writes() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotDynamicStaticWrite {
    public static $value = "aot";
    public static $count = 10;
}
eval('class EvalDynamicStaticWrite {
    public static $value = "start";
    public static $count = 1;
}
$evalClass = "EvalDynamicStaticWrite";
$evalClass::$value = "set";
$evalClass::$value .= ":tail";
$evalClass::$count += 4;
echo EvalDynamicStaticWrite::$value; echo "|";
echo $evalClass::$count; echo "|";
$aotClass = "EvalAotDynamicStaticWrite";
$aotClass::$value = "A";
$aotClass::$value .= "B";
$aotClass::$count += 5;
echo EvalAotDynamicStaticWrite::$value; echo "|";
echo $aotClass::$count;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "set:tail|5|AB|15");
}

/// Verifies eval static property increment/decrement works with nominal and dynamic receivers.
#[test]
fn test_eval_static_property_inc_dec_statements() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticIncDec {
    public static $count = 10;
}
eval('class EvalDynamicStaticIncDec {
    public static $count = 1;
}
EvalDynamicStaticIncDec::$count++;
++EvalDynamicStaticIncDec::$count;
$evalClass = "EvalDynamicStaticIncDec";
$evalClass::$count++;
--$evalClass::$count;
$i = 0;
for (; $i < 3; $evalClass::$count++) {
    $i++;
}
echo EvalDynamicStaticIncDec::$count; echo "|";
EvalAotStaticIncDec::$count++;
$aot = "EvalAotStaticIncDec";
$aot::$count--;
--$aot::$count;
$j = 0;
for (; $j < 2; EvalAotStaticIncDec::$count++) {
    $j++;
}
echo EvalAotStaticIncDec::$count;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "6|11");
}

/// Verifies eval static property array writes and appends update static storage.
#[test]
fn test_eval_static_property_array_write_statements() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticPropertyArrayWrite {
    public static array $items = [];
}
eval('class EvalDynamicStaticPropertyArrayWrite {
    public static $items = [];
    public static $dyn = [];
}
class EvalStaticPropertyArrayAccessBox implements ArrayAccess {
    public array $data = [];
    public function offsetExists(mixed $offset): bool {
        return true;
    }
    public function offsetGet(mixed $offset): mixed {
        return $this->data[$offset];
    }
    public function offsetSet(mixed $offset, mixed $value): void {
        if ($offset === null) {
            $this->data[] = $value;
        } else {
            $this->data[$offset] = $value;
        }
    }
    public function offsetUnset(mixed $offset): void {
        unset($this->data[$offset]);
    }
}
class EvalStaticPropertyArrayAccessHolder {
    public static $box;
}
EvalDynamicStaticPropertyArrayWrite::$items[0] = "a";
EvalDynamicStaticPropertyArrayWrite::$items[] = "b";
EvalDynamicStaticPropertyArrayWrite::$items[0] .= "A";
$class = "EvalDynamicStaticPropertyArrayWrite";
$class::$dyn[1] = "x";
$class::$dyn[] = "y";
echo EvalDynamicStaticPropertyArrayWrite::$items[0] . ":";
echo EvalDynamicStaticPropertyArrayWrite::$items[1] . ":";
echo EvalDynamicStaticPropertyArrayWrite::$dyn[1] . ":";
echo EvalDynamicStaticPropertyArrayWrite::$dyn[2] . "|";
EvalAotStaticPropertyArrayWrite::$items[0] = "m";
EvalAotStaticPropertyArrayWrite::$items[] = "n";
EvalAotStaticPropertyArrayWrite::$items[0] .= "M";
echo EvalAotStaticPropertyArrayWrite::$items[0] . ":";
echo EvalAotStaticPropertyArrayWrite::$items[1] . "|";
EvalStaticPropertyArrayAccessHolder::$box = new EvalStaticPropertyArrayAccessBox();
EvalStaticPropertyArrayAccessHolder::$box[] = "q";
EvalStaticPropertyArrayAccessHolder::$box[0] .= "Q";
echo EvalStaticPropertyArrayAccessHolder::$box[0];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "aA:b:x:y|mM:n|qQ");
}

/// Verifies eval unsets object-property and static-property array elements.
#[test]
fn test_eval_property_and_static_property_array_unset_statements() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalArrayUnsetAccessBox implements ArrayAccess {
    public $removed = "";

    public function offsetExists(mixed $offset): bool {
        return true;
    }

    public function offsetGet(mixed $offset): mixed {
        return "K";
    }

    public function offsetSet(mixed $offset, mixed $value): void {
    }

    public function offsetUnset(mixed $offset): void {
        $this->removed = $offset;
    }
}

class EvalAotPropertyArrayUnset {
    public array $items = ["a", "b"];
    public static array $staticItems = ["x", "y"];
}

eval('class EvalDynamicPropertyArrayUnset {
    public array $items = ["a", "b"];
    public static $staticItems = ["x", "y"];
    public $box;
    public static $staticBox;
}
$dyn = new EvalDynamicPropertyArrayUnset();
unset($dyn->items[0]);
$name = "items";
unset($dyn->{$name}[1]);
echo isset($dyn->items[0]) ? "bad" : "d0"; echo ":";
echo isset($dyn->items[1]) ? "bad" : "d1"; echo "|";
unset(EvalDynamicPropertyArrayUnset::$staticItems[0]);
$class = "EvalDynamicPropertyArrayUnset";
unset($class::$staticItems[1]);
echo isset(EvalDynamicPropertyArrayUnset::$staticItems[0]) ? "bad" : "s0"; echo ":";
echo isset(EvalDynamicPropertyArrayUnset::$staticItems[1]) ? "bad" : "s1"; echo "|";
$aot = new EvalAotPropertyArrayUnset();
unset($aot->items[0]);
unset(EvalAotPropertyArrayUnset::$staticItems[0]);
echo isset($aot->items[0]) ? "bad" : "a0"; echo ":";
echo isset(EvalAotPropertyArrayUnset::$staticItems[0]) ? "bad" : "as0"; echo "|";
$dyn->box = new EvalArrayUnsetAccessBox();
unset($dyn->box["drop"]);
echo $dyn->box->removed . ":" . $dyn->box["keep"] . "|";
EvalDynamicPropertyArrayUnset::$staticBox = new EvalArrayUnsetAccessBox();
unset(EvalDynamicPropertyArrayUnset::$staticBox["drop"]);
echo EvalDynamicPropertyArrayUnset::$staticBox->removed . ":";
echo EvalDynamicPropertyArrayUnset::$staticBox["keep"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "d0:d1|s0:s1|a0:as0|drop:K|drop:K");
}

/// Verifies eval `isset()` and `empty()` probe static properties without normal read fatals.
#[test]
fn test_eval_static_property_isset_empty_probes() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticProbe {
    public static $nullish = null;
    public static $empty = "";
    public static $value = "x";
}
eval('class EvalDynamicStaticProbe {
    public static $nullish = null;
    public static $empty = "";
    public static $value = "x";
}
$evalClass = "EvalDynamicStaticProbe";
echo isset(EvalDynamicStaticProbe::$value) ? "nominal" : "bad"; echo "|";
echo isset($evalClass::$value) ? "eval-set" : "bad"; echo "|";
echo isset($evalClass::$nullish) ? "bad" : "eval-null"; echo "|";
echo empty($evalClass::$empty) ? "eval-empty" : "bad"; echo "|";
echo empty($evalClass::$value) ? "bad" : "eval-value"; echo "|";
echo isset($evalClass::$missing) ? "bad" : "eval-missing"; echo "|";
echo empty($evalClass::$missing) ? "eval-missing-empty" : "bad"; echo "|";
$propName = "value";
$emptyName = "empty";
$missingName = "missing";
echo isset($evalClass::${$propName}) ? "eval-name-set" : "bad"; echo "|";
echo empty($evalClass::${$emptyName}) ? "eval-name-empty" : "bad"; echo "|";
echo empty($evalClass::${$missingName}) ? "eval-name-missing-empty" : "bad"; echo "|";
$aotClass = "EvalAotStaticProbe";
echo isset($aotClass::$value) ? "aot-set" : "bad"; echo "|";
echo isset($aotClass::$nullish) ? "bad" : "aot-null"; echo "|";
echo empty($aotClass::$empty) ? "aot-empty" : "bad"; echo "|";
echo empty($aotClass::$value) ? "bad" : "aot-value"; echo "|";
echo isset($aotClass::$missing) ? "bad" : "aot-missing"; echo "|";
echo empty($aotClass::$missing) ? "aot-missing-empty" : "bad"; echo "|";
echo isset($aotClass::${$propName}) ? "aot-name-set" : "bad"; echo "|";
echo empty($aotClass::${$emptyName}) ? "aot-name-empty" : "bad"; echo "|";
echo empty($aotClass::${$missingName}) ? "aot-name-missing-empty" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "nominal|eval-set|eval-null|eval-empty|eval-value|eval-missing|eval-missing-empty|eval-name-set|eval-name-empty|eval-name-missing-empty|aot-set|aot-null|aot-empty|aot-value|aot-missing|aot-missing-empty|aot-name-set|aot-name-empty|aot-name-missing-empty"
    );
}

/// Verifies eval static property unsets throw PHP's catchable Error.
#[test]
fn test_eval_static_property_unset_throws_error() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticUnset {
    public static $value = "aot";
}
eval('class EvalDynamicStaticUnset {
    public static $value = "eval";
}
try {
    unset(EvalDynamicStaticUnset::$value);
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
$class = "EvalDynamicStaticUnset";
try {
    unset($class::$value);
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
$name = "value";
try {
    unset($class::${$name});
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
$aot = "EvalAotStaticUnset";
try {
    unset($aot::$value);
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    unset($aot::${$name});
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    unset(EvalMissingStaticUnset::$value);
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Error:Attempt to unset static property EvalDynamicStaticUnset::$value|Error:Attempt to unset static property EvalDynamicStaticUnset::$value|Error:Attempt to unset static property EvalDynamicStaticUnset::$value|Error:Attempt to unset static property EvalAotStaticUnset::$value|Error:Attempt to unset static property EvalAotStaticUnset::$value|Error:Class \"EvalMissingStaticUnset\" not found"
    );
}

/// Verifies eval invokable objects dispatch through variable and callback call paths.
#[test]
fn test_eval_declared_invokable_object_dynamic_callables() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_plain_call_side_effect() {
    echo "bad";
    return "x";
}
class EvalInvokableBox {
    public function __construct($label = "box") {
        $this->label = $label;
    }
    private function __invoke($left = "A", $right = "B") {
        return $this->label . ":" . $left . $right;
    }
}
class EvalPlainCallableProbe {}
$box = new EvalInvokableBox("box");
$plain = new EvalPlainCallableProbe();
echo is_callable($box) ? "Y:" : "N:";
echo is_callable($plain) ? "bad:" : "plain:";
echo $box(right: "D", left: "C") . ":";
try {
    $plain(eval_plain_call_side_effect());
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo ":";
echo (new EvalInvokableBox("new"))("E", "F") . ":";
echo call_user_func($box, "G", "H") . ":";
$first = $box(...);
echo $first("K", "L") . ":";
echo call_user_func_array($box, ["right" => "J", "left" => "I"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Y:plain:box:CD:Error:Object of type EvalPlainCallableProbe is not callable:new:EF:box:GH:box:KL:box:IJ"
    );
}

/// Verifies eval AOT invokable objects dispatch through variable and callback call paths.
#[test]
fn test_eval_aot_invokable_object_dynamic_callables() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_aot_invokable_side_effect() {
    echo "bad";
    return "x";
}

class EvalAotInvokableBox {
    public function __invoke(string $left = "A", string $right = "B"): string {
        return $left . $right;
    }
}

class EvalAotPlainInvokableProbe {}

eval('$box = new EvalAotInvokableBox();
echo is_callable($box) ? "Y:" : "N:";
echo $box(right: "D", left: "C") . ":";
echo $box("E") . ":";
echo call_user_func($box, "F", "G") . ":";
$first = $box(...);
echo $first("J", "K") . ":";
echo call_user_func_array($box, ["right" => "I", "left" => "H"]) . ":";
try {
    (new EvalAotPlainInvokableProbe())(eval_aot_invokable_side_effect());
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Y:CD:EB:FG:JK:HI:Error:Object of type EvalAotPlainInvokableProbe is not callable"
    );
}

/// Verifies eval call_user_func rejects non-invokable objects with PHP's TypeError.
#[test]
fn test_eval_call_user_func_rejects_non_invokable_object() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotPlainCallbackError {}

eval('class EvalPlainCallbackError {}
$plain = new EvalPlainCallbackError();
try {
    call_user_func($plain);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func_array($plain, []);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
$aotPlain = new EvalAotPlainCallbackError();
try {
    call_user_func($aotPlain);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, no array or string given|\
TypeError:call_user_func_array(): Argument #1 ($callback) must be a valid callback, no array or string given|\
TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, no array or string given"
    );
}

/// Verifies eval call_user_func rejects invalid method callable arrays with PHP's TypeError.
#[test]
fn test_eval_call_user_func_rejects_invalid_method_callable_arrays() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMissingCallbackArray {}
class EvalPrivateCallbackArray {
    private function hidden() {
        return "bad";
    }
}
class EvalInstanceCallbackArray {
    public function inst() {
        return "bad";
    }
}
$missing = new EvalMissingCallbackArray();
try {
    call_user_func([$missing, "MiSsInG"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func_array([$missing, "missing"], []);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func([new EvalPrivateCallbackArray(), "hidden"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func(["EvalInstanceCallbackArray", "inst"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, class EvalMissingCallbackArray does not have a method \"MiSsInG\"|\
TypeError:call_user_func_array(): Argument #1 ($callback) must be a valid callback, class EvalMissingCallbackArray does not have a method \"missing\"|\
TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method EvalPrivateCallbackArray::hidden()|\
TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, non-static method EvalInstanceCallbackArray::inst() cannot be called statically"
    );
}

/// Verifies eval call_user_func validates method callable arrays for AOT classes too.
#[test]
fn test_eval_call_user_func_rejects_invalid_aot_method_callable_arrays() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotMissingCallbackArray {}
class EvalAotPrivateCallbackArray {
    private function hidden() {
        return "bad";
    }
}
class EvalAotInstanceCallbackArray {
    public function inst() {
        return "bad";
    }
}

eval('$missing = new EvalAotMissingCallbackArray();
try {
    call_user_func([$missing, "missing"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func([new EvalAotPrivateCallbackArray(), "hidden"]);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    call_user_func_array(["EvalAotInstanceCallbackArray", "inst"], []);
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, class EvalAotMissingCallbackArray does not have a method \"missing\"|\
TypeError:call_user_func(): Argument #1 ($callback) must be a valid callback, cannot access private method EvalAotPrivateCallbackArray::hidden()|\
TypeError:call_user_func_array(): Argument #1 ($callback) must be a valid callback, non-static method EvalAotInstanceCallbackArray::inst() cannot be called statically"
    );
}

/// Verifies eval `is_callable()` probes method callable arrays for AOT classes.
#[test]
fn test_eval_is_callable_checks_aot_method_callable_arrays() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotCallableProbe {
    public function inst() {}
    public static function stat() {}
    private function hidden() {}
}
class EvalAotCallableMagicProbe {
    public function __call($method, $args) {}
    public static function __callStatic($method, $args) {}
}

eval('$probe = new EvalAotCallableProbe();
$magic = new EvalAotCallableMagicProbe();
echo is_callable([$probe, "inst"]) ? "OI" : "bad"; echo ":";
echo is_callable(["EvalAotCallableProbe", "stat"]) ? "SS" : "bad"; echo ":";
echo is_callable(["EvalAotCallableProbe", "inst"]) ? "bad" : "NS"; echo ":";
echo is_callable([$probe, "hidden"]) ? "bad" : "PV"; echo ":";
echo is_callable([$magic, "missing"]) ? "OM" : "bad"; echo ":";
echo is_callable(["EvalAotCallableMagicProbe", "static_missing"]) ? "SM" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "OI:SS:NS:PV:OM:SM");
}

/// Verifies eval callable arrays use `__call` and `__callStatic` magic fallbacks.
#[test]
fn test_eval_call_user_func_method_callable_arrays_use_magic_fallbacks() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicCallbackArray {
    public function __call($method, $args) {
        return $method . ":" . $args[0];
    }
    public static function __callStatic($method, $args) {
        return $method . ":" . $args[0];
    }
}
$box = new EvalMagicCallbackArray();
echo is_callable([$box, "missing"]) ? "Y:" : "N:";
echo call_user_func([$box, "missing"], "A") . ":";
echo call_user_func_array([$box, "missing"], ["B"]) . ":";
echo is_callable(["EvalMagicCallbackArray", "static_missing"]) ? "S:" : "s:";
echo call_user_func(["EvalMagicCallbackArray", "static_missing"], "C");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Y:missing:A:missing:B:S:static_missing:C");
}

/// Verifies eval object method fallback dispatches missing and inaccessible methods through `__call`.
#[test]
fn test_eval_declared_magic_call_method_fallback() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicCallBox {
    private function hidden($value) { return "bad"; }
    protected function __call($method, $args) {
        return $method . ":" . $args[0] . ":" . $args["name"];
    }
}
$box = new EvalMagicCallBox();
echo $box->DoThing("A", name: "B") . ":";
echo $box->hidden("C", name: "D");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "DoThing:A:B:hidden:C:D");
}

/// Verifies missing eval-declared instance methods throw catchable PHP errors.
#[test]
fn test_eval_declared_missing_instance_method_throws_error() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMissingInstanceCallBox {}
$box = new EvalMissingInstanceCallBox();
try {
    echo $box->missing();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Error:Call to undefined method EvalMissingInstanceCallBox::missing()"
    );
}

/// Verifies eval static method fallback dispatches missing and inaccessible methods through `__callStatic`.
#[test]
fn test_eval_declared_magic_call_static_method_fallback() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMagicStaticBox {
    private static function hidden($value) { return "bad"; }
    private static function __callStatic($method, $args) {
        return $method . ":" . $args[0] . ":" . $args["name"];
    }
}
echo EvalMagicStaticBox::DoStatic("A", name: "B") . ":";
echo EvalMagicStaticBox::Hidden("C", name: "D");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "DoStatic:A:B:Hidden:C:D");
}

/// Verifies eval supports variable static method names with a named receiver.
#[test]
fn test_eval_named_receiver_dynamic_static_method_name() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalNamedReceiverDynamicStaticMethod {
    public static function read($value) {
        return "read:" . $value;
    }

    public static function __callStatic($method, $args) {
        return $method . ":" . $args[0];
    }
}
$method = "read";
$missing = "later";
echo EvalNamedReceiverDynamicStaticMethod::$method("A") . "|";
echo EvalNamedReceiverDynamicStaticMethod::$missing("B");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "read:A|later:B");
}

/// Verifies eval supports braced dynamic static method names.
#[test]
fn test_eval_braced_dynamic_static_method_name() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalBracedDynamicStaticMethod {
    public static function read($value) {
        return "read:" . $value;
    }

    public static function __callStatic($method, $args) {
        return $method . ":" . $args[0];
    }
}
$method = "read";
$class = "EvalBracedDynamicStaticMethod";
echo EvalBracedDynamicStaticMethod::{$method}("A") . "|";
echo $class::{"missing"}("B");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "read:A|missing:B");
}

/// Verifies eval supports braced dynamic class constant names.
#[test]
fn test_eval_braced_dynamic_class_constant_name() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalBracedDynamicClassConstant {
    public const READ = "read";
    public const FALLBACK = "fallback";
}
$constant = "READ";
$class = "EvalBracedDynamicClassConstant";
echo EvalBracedDynamicClassConstant::{$constant} . "|";
echo $class::{"FALLBACK"};');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "read|fallback");
}

/// Verifies eval supports parenthesized expression static receivers for reads and calls.
#[test]
fn test_eval_expression_static_receiver_members() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalExpressionStaticReceiver {
    public static $count = 5;
    public const WORD = "word";

    public static function read($value) {
        return "read:" . $value;
    }
}

function eval_expression_static_receiver() {
    return "EvalExpressionStaticReceiver";
}

$constant = "WORD";
echo (eval_expression_static_receiver())::read("A") . "|";
echo (eval_expression_static_receiver())::WORD . "|";
echo (eval_expression_static_receiver())::{$constant} . "|";
echo (eval_expression_static_receiver())::$count;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "read:A|word|word|5");
}

/// Verifies eval supports expression static receivers for static property writes.
#[test]
fn test_eval_expression_static_receiver_property_writes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalExpressionStaticReceiverWrites {
    public static $count = 1;
    public static $items = [];
}

function eval_expression_static_receiver_writes() {
    return "EvalExpressionStaticReceiverWrites";
}

(eval_expression_static_receiver_writes())::$count = 2;
(eval_expression_static_receiver_writes())::$count += 3;
(eval_expression_static_receiver_writes())::$items = [1];
(eval_expression_static_receiver_writes())::$items[] = 4;
(eval_expression_static_receiver_writes())::$items[0] = 5;
(eval_expression_static_receiver_writes())::$count++;
++(eval_expression_static_receiver_writes())::$count;
--(eval_expression_static_receiver_writes())::$count;
echo (eval_expression_static_receiver_writes())::$count . "|";
echo (eval_expression_static_receiver_writes())::$items[0] . ":";
echo (eval_expression_static_receiver_writes())::$items[1];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "6|5:4");
}

/// Verifies eval supports `${...}` static property names for reads and writes.
#[test]
fn test_eval_dynamic_static_property_name_expressions() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalDynamicStaticPropertyNameExpression {
    public static $count = 1;
    public static $items = [];
}

function eval_dynamic_static_property_name_expression() {
    return "count";
}

function eval_dynamic_static_property_items_expression() {
    return "items";
}

$class = "EvalDynamicStaticPropertyNameExpression";
$name = "count";
echo EvalDynamicStaticPropertyNameExpression::${$name} . "|";
$class::${eval_dynamic_static_property_name_expression()} = 2;
$class::${eval_dynamic_static_property_name_expression()} += 3;
++$class::${eval_dynamic_static_property_name_expression()};
$class::${eval_dynamic_static_property_items_expression()} = [1];
$class::${eval_dynamic_static_property_items_expression()}[] = 4;
$class::${eval_dynamic_static_property_items_expression()}[0] = 5;
echo $class::${eval_dynamic_static_property_name_expression()} . "|";
echo $class::${eval_dynamic_static_property_items_expression()}[0] . ":";
echo $class::${eval_dynamic_static_property_items_expression()}[1];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1|6|5:4");
}

/// Verifies eval-declared static properties can bind to local variables by reference.
#[test]
fn test_eval_declared_static_property_reference_binding() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalStaticPropertyReferenceBindBox {
    public static $value = "old";
    public static $other = "old";
    public static $third = "old";
}

$source = "A";
EvalStaticPropertyReferenceBindBox::$value =& $source;
$source = "B";
echo EvalStaticPropertyReferenceBindBox::$value . "|";
EvalStaticPropertyReferenceBindBox::$value = "C";
echo $source . "|";

$class = "EvalStaticPropertyReferenceBindBox";
$dynamic = "D";
$class::$other =& $dynamic;
$dynamic = "E";
echo EvalStaticPropertyReferenceBindBox::$other . "|";
$class::$other = "F";
echo $dynamic . "|";

$name = "third";
$third = "G";
EvalStaticPropertyReferenceBindBox::${$name} =& $third;
$third = "H";
echo EvalStaticPropertyReferenceBindBox::$third . "|";
$class::${$name} = "I";
echo $third;');
"#,
    );
    assert_eq!(out, "B|C|E|F|H|I");
}

/// Verifies eval can bind generated/AOT static properties to eval variables by reference.
#[test]
fn test_eval_aot_static_property_reference_binding() {
    let out = compile_and_run(
        r#"<?php
class EvalAotStaticPropertyReferenceBindBox {
    public static $value = "old";
    public static $other = "old";
}
eval('$source = "A";
EvalAotStaticPropertyReferenceBindBox::$value =& $source;
$source = "B";
echo EvalAotStaticPropertyReferenceBindBox::$value . "|";
EvalAotStaticPropertyReferenceBindBox::$value = "C";
echo $source . "|";

$class = "EvalAotStaticPropertyReferenceBindBox";
$dynamic = "D";
$class::$other =& $dynamic;
$dynamic = "E";
echo EvalAotStaticPropertyReferenceBindBox::$other . "|";
$class::$other = "F";
echo $dynamic;');
"#,
    );
    assert_eq!(out, "B|C|E|F");
}

/// Verifies eval AOT method fallback dispatches missing and inaccessible methods through `__call`.
#[test]
fn test_eval_aot_magic_call_method_fallback() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotMagicCallBox {
    private function hidden($value) { return "bad"; }
    public function __call($method, $args) {
        return $method . ":" . $args[0] . ":" . $args[1];
    }
}
class EvalAotMagicStaticBox {
    private static function hidden($value) { return "bad"; }
    public static function __callStatic($method, $args) {
        return $method . ":" . $args[0] . ":" . $args[1];
    }
}
eval('$box = new EvalAotMagicCallBox();
echo $box->DoThing("A", "B") . ":";
echo $box->hidden("C", "D") . ":";
echo EvalAotMagicStaticBox::DoStatic("E", "F") . ":";
echo EvalAotMagicStaticBox::Hidden("G", "H") . ":";
echo is_callable([$box, "hidden"]) ? "OC:" : "bad:";
echo call_user_func([$box, "hidden"], "I", "J") . ":";
echo is_callable(["EvalAotMagicStaticBox", "Hidden"]) ? "SC:" : "bad:";
echo call_user_func(["EvalAotMagicStaticBox", "Hidden"], "K", "L");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "DoThing:A:B:hidden:C:D:DoStatic:E:F:Hidden:G:H:OC:hidden:I:J:SC:Hidden:K:L"
    );
}

/// Verifies eval-declared subclasses expose inherited AOT `__callStatic` to callback probes.
#[test]
fn test_eval_declared_child_inherits_aot_magic_call_static_callbacks() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotMagicStaticParent {
    public static function __callStatic($method, $args) {
        return "S:" . $method . ":" . $args[0];
    }
}
eval('class EvalAotMagicStaticChild extends EvalAotMagicStaticParent {}
echo EvalAotMagicStaticChild::direct("A") . ":";
$callback = ["EvalAotMagicStaticChild", "dynamic"];
echo is_callable($callback) ? "C:" : "bad:";
echo $callback("B") . ":";
echo call_user_func($callback, "C") . ":";
echo call_user_func_array($callback, ["D"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "S:direct:A:C:S:dynamic:B:S:dynamic:C:S:dynamic:D"
    );
}

/// Verifies inherited native `__callStatic` does not mask inherited non-static AOT methods.
#[test]
fn test_eval_declared_child_aot_magic_call_static_does_not_mask_non_static_callbacks() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticMagicNonStaticParent {
    public function run(string $value): string {
        return "I:" . $value;
    }

    public static function __callStatic($method, $args) {
        return "S:" . $method . ":" . $args[0];
    }
}
eval('class EvalAotStaticMagicNonStaticChild extends EvalAotStaticMagicNonStaticParent {}
$callback = ["EvalAotStaticMagicNonStaticChild", "run"];
echo is_callable($callback) ? "callable:" : "not:";
try {
    echo call_user_func($callback, "A");
} catch (TypeError) {
    echo "TypeError";
}
echo ":";
echo EvalAotStaticMagicNonStaticChild::missing("B");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "not:TypeError:S:missing:B");
}

/// Verifies eval-declared subclasses expose inherited public AOT static methods to callbacks.
#[test]
fn test_eval_declared_child_inherits_aot_static_method_callbacks() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotStaticCallbackParent {
    public static function read(string $value): string {
        return "R:" . $value;
    }
}
eval('class EvalAotStaticCallbackChild extends EvalAotStaticCallbackParent {}
echo EvalAotStaticCallbackChild::read("A") . ":";
$callback = ["EvalAotStaticCallbackChild", "read"];
echo is_callable($callback) ? "C:" : "bad:";
echo $callback("B") . ":";
echo call_user_func($callback, "C") . ":";
echo call_user_func_array($callback, ["D"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "R:A:C:R:B:R:C:R:D");
}

/// Verifies eval first-class static callables capture inherited protected AOT methods.
#[test]
fn test_eval_declared_child_first_class_callable_inherited_protected_aot_static_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotProtectedStaticCallableParent {
    protected static function hidden(string $value): string {
        return "H:" . $value;
    }
}
eval('class EvalAotProtectedStaticCallableChild extends EvalAotProtectedStaticCallableParent {
    public static function makeHidden() {
        return self::hidden(...);
    }
}
$callback = EvalAotProtectedStaticCallableChild::makeHidden();
echo is_callable($callback) ? "C:" : "bad:";
echo $callback("A") . ":";
echo call_user_func($callback, "B") . ":";
echo call_user_func_array($callback, ["C"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "C:H:A:H:B:H:C");
}

/// Verifies one invalid eval magic-method declaration fails during runtime class registration.
fn assert_eval_magic_method_contract_rejected(source: &str) {
    let err = compile_and_run_expect_failure(&format!("<?php\n{source}\n"));
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects variadic `__call` declarations.
#[test]
fn test_eval_rejects_invalid_magic_variadic_call_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidMagic {
    public function __call($method, ...$args) {
        return "bad";
    }
}');"#,
    );
}

/// Verifies eval rejects non-string `__toString` return declarations.
#[test]
fn test_eval_rejects_invalid_magic_to_string_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidToStringReturn {
    public function __toString(): int {
        return 1;
    }
}');"#,
    );
}

/// Verifies eval rejects non-bool `__isset` return declarations.
#[test]
fn test_eval_rejects_invalid_magic_isset_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidIssetReturn {
    public function __isset($name): string {
        return "yes";
    }
}');"#,
    );
}

/// Verifies eval rejects by-reference `__get` parameters.
#[test]
fn test_eval_rejects_invalid_magic_get_by_ref_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidGetByRef {
    public function __get(&$name) {
        return "x";
    }
}');"#,
    );
}

/// Verifies eval rejects non-string-compatible `__get` parameter declarations.
#[test]
fn test_eval_rejects_invalid_magic_get_param_type_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidGetParamType {
    public function __get(int $name) {
        return "x";
    }
}');"#,
    );
}

/// Verifies eval rejects return types on `__unset`.
#[test]
fn test_eval_rejects_invalid_magic_unset_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidUnsetReturn {
    public function __unset($name): int {
        return 1;
    }
}');"#,
    );
}

/// Verifies eval rejects return types on `__set`.
#[test]
fn test_eval_rejects_invalid_magic_set_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidSetReturn {
    public function __set($name, $value): int {
        return 1;
    }
}');"#,
    );
}

/// Verifies eval rejects non-array `__call` arguments parameters.
#[test]
fn test_eval_rejects_invalid_magic_call_args_type_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidCallArgsType {
    public function __call(string $name, string $args) {}
}');"#,
    );
}

/// Verifies eval rejects non-array `__sleep` return declarations.
#[test]
fn test_eval_rejects_invalid_magic_sleep_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidSleepReturn {
    public function __sleep(): string {
        return "x";
    }
}');"#,
    );
}

/// Verifies eval rejects static `__serialize` declarations.
#[test]
fn test_eval_rejects_invalid_magic_serialize_static_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidSerializeStatic {
    public static function __serialize(): array {
        return [];
    }
}');"#,
    );
}

/// Verifies eval rejects wrong-arity `__unserialize` declarations.
#[test]
fn test_eval_rejects_invalid_magic_unserialize_arity_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidUnserializeArity {
    public function __unserialize(): void {}
}');"#,
    );
}

/// Verifies eval rejects non-array `__debugInfo` return declarations.
#[test]
fn test_eval_rejects_invalid_magic_debug_info_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidDebugInfoReturn {
    public function __debugInfo(): string {
        return "x";
    }
}');"#,
    );
}

/// Verifies eval rejects instance `__set_state` declarations.
#[test]
fn test_eval_rejects_invalid_magic_set_state_instance_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidSetStateInstance {
    public function __set_state($data) {}
}');"#,
    );
}

/// Verifies eval rejects return types on `__clone`.
#[test]
fn test_eval_rejects_invalid_magic_clone_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidCloneReturn {
    public function __clone(): int {}
}');"#,
    );
}

/// Verifies eval rejects return types on `__construct`.
#[test]
fn test_eval_rejects_invalid_magic_construct_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidConstructReturn {
    public function __construct(): void {}
}');"#,
    );
}

/// Verifies eval rejects return types on `__destruct`.
#[test]
fn test_eval_rejects_invalid_magic_destruct_return_contract() {
    assert_eval_magic_method_contract_rejected(
        r#"eval('class EvalInvalidDestructReturn {
    public function __destruct(): void {}
}');"#,
    );
}

/// Verifies eval-declared `#[Override]` methods require a parent or interface target.
#[test]
fn test_eval_declared_override_attribute_validation() {
    let out = compile_and_run(
        r#"<?php
eval('interface EvalOverrideContract {
    public function label(): string;
}
class EvalOverrideBase {
    public function name(): string { return "base"; }
}
class EvalOverrideChild extends EvalOverrideBase implements EvalOverrideContract {
    #[\Override]
    public function name(): string { return "child"; }
    #[Override]
    public function label(): string { return "contract"; }
}
$box = new EvalOverrideChild();
echo $box->name() . ":" . $box->label();');
"#,
    );
    assert_eq!(out, "child:contract");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalOverrideMissing {
    #[\Override]
    public function missing(): string { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared interface `#[Override]` methods require parent methods.
#[test]
fn test_eval_declared_interface_override_attribute_validation() {
    let out = compile_and_run(
        r#"<?php
eval('interface EvalIfaceOverrideParent {
    public function label(): string;
}
interface EvalIfaceOverrideChild extends EvalIfaceOverrideParent {
    #[\Override]
    public function label(): string;
}
class EvalIfaceOverrideImpl implements EvalIfaceOverrideChild {
    public function label(): string { return "child"; }
}
$box = new EvalIfaceOverrideImpl();
echo $box->label();');
"#,
    );
    assert_eq!(out, "child");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalIfaceOverrideMissing {
    #[\Override]
    public function missing(): string;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval reflection exposes methods for AOT interfaces that only eval uses.
#[test]
fn test_eval_reflects_unimplemented_aot_interface_methods() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotOnlyReflectableContract {
    public function aotLabel(): string;
}
eval('$r = new ReflectionClass("EvalAotOnlyReflectableContract");
echo $r->hasMethod("aotLabel") ? "H" : "h"; echo ":";
echo count($r->getMethods()); echo ":";
echo get_class_methods("EvalAotOnlyReflectableContract")[0] ?? "none";');
"#,
    );
    assert_eq!(out, "H:1:aotlabel");
}

/// Verifies eval interface `#[Override]` can target a generated/AOT parent interface.
#[test]
fn test_eval_declared_interface_override_attribute_accepts_aot_parent() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotIfaceOverrideParent {
    public function aotLabel(): string;
}
eval('interface EvalIfaceOverrideAotChild extends EvalAotIfaceOverrideParent {
    #[\Override]
    public function aotLabel(): string;
}
class EvalIfaceOverrideAotImpl implements EvalIfaceOverrideAotChild {
    public function aotLabel(): string { return "aot"; }
}
$box = new EvalIfaceOverrideAotImpl();
echo $box->aotLabel();');
"#,
    );
    assert_eq!(out, "aot");
}

/// Verifies eval classes can implement generated/AOT interface method contracts.
#[test]
fn test_eval_declared_class_implements_aot_interface_methods() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotImplementedContract {
    public function label(string $name): string;
    public static function staticLabel(string $name): string;
}
eval('class EvalAotImplementedBox implements EvalAotImplementedContract {
    #[\Override]
    public function label(string $name): string { return "I:" . $name; }
    #[\Override]
    public static function staticLabel(string $name): string { return "S:" . $name; }
}
$box = new EvalAotImplementedBox();
echo $box->label("Ada") . ":" . EvalAotImplementedBox::staticLabel("Bob");');
"#,
    );
    assert_eq!(out, "I:Ada:S:Bob");
}

/// Verifies eval rejects concrete classes missing generated/AOT interface methods.
#[test]
fn test_eval_declared_class_rejects_missing_aot_interface_methods() {
    let err = compile_and_run_expect_failure(
        r#"<?php
interface EvalAotMissingContract {
    public function required(): string;
}
eval('class EvalAotMissingBox implements EvalAotMissingContract {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval validates generated/AOT interface method signatures.
#[test]
fn test_eval_declared_class_rejects_incompatible_aot_interface_method_signature() {
    let err = compile_and_run_expect_failure(
        r#"<?php
interface EvalAotSignatureContract {
    public function required(string $name): string;
}
eval('class EvalAotSignatureBox implements EvalAotSignatureContract {
    public function required(int $name): string { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval classes can implement generated/AOT interface property contracts.
#[test]
fn test_eval_declared_class_implements_aot_interface_properties() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotPropertyContract {
    public string $name { get; set; }
}
eval('class EvalAotPropertyBox implements EvalAotPropertyContract {
    public string $name = "Ada";
}
$box = new EvalAotPropertyBox();
$box->name = "Grace";
echo $box->name;');
"#,
    );
    assert_eq!(out, "Grace");
}

/// Verifies eval rejects concrete classes missing generated/AOT interface properties.
#[test]
fn test_eval_declared_class_rejects_missing_aot_interface_property() {
    let err = compile_and_run_expect_failure(
        r#"<?php
interface EvalAotMissingPropertyContract {
    public string $name { get; }
}
eval('class EvalAotMissingPropertyBox implements EvalAotMissingPropertyContract {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval validates generated/AOT interface property types.
#[test]
fn test_eval_declared_class_rejects_incompatible_aot_interface_property_type() {
    let err = compile_and_run_expect_failure(
        r#"<?php
interface EvalAotPropertyTypeContract {
    public string $name { get; set; }
}
eval('class EvalAotPropertyTypeBox implements EvalAotPropertyTypeContract {
    public int $name = 1;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval readonly properties cannot satisfy generated/AOT set contracts.
#[test]
fn test_eval_declared_class_rejects_readonly_aot_interface_set_property() {
    let err = compile_and_run_expect_failure(
        r#"<?php
interface EvalAotSetPropertyContract {
    public string $name { set; }
}
eval('class EvalAotReadonlySetPropertyBox implements EvalAotSetPropertyContract {
    public readonly string $name;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval reflection exposes generated/AOT interface property-hook metadata.
#[test]
fn test_eval_reflection_exposes_aot_interface_property_hooks() {
    let out = compile_and_run(
        r#"<?php
interface EvalAotReflectPropertyParent {
    public string $parentName { get; }
}
interface EvalAotReflectPropertyContract extends EvalAotReflectPropertyParent {
    public string $name { get; set; }
}
echo eval('$ref = new ReflectionClass("EvalAotReflectPropertyContract");
echo $ref->hasProperty("name") ? "H:" : "h:";
echo $ref->hasProperty("parentName") ? "P:" : "p:";
$properties = $ref->getProperties();
echo count($properties) . ":";
$property = $ref->getProperty("name");
$parent = $ref->getProperty("parentName");
$direct = new ReflectionProperty("EvalAotReflectPropertyContract", "name");
$getCase = PropertyHookType::Get;
$setCase = PropertyHookType::Set;
echo $property->getName() . ":";
echo $property->getDeclaringClass()->getName() . ":";
echo $property->getType()->getName() . ":";
echo ($property->hasHooks() ? "hooks" : "plain") . ":";
echo ($property->hasHook($getCase) ? "G" : "g") . ":";
echo ($property->hasHook($setCase) ? "S" : "s") . ":";
$hooks = $property->getHooks();
echo count($hooks) . ":";
echo $hooks["get"]->getName() . ":";
echo $hooks["set"]->getName() . ":";
echo ($property->getHook($getCase)->isAbstract() ? "A" : "a") . ":";
echo ($direct->hasHook($setCase) ? "D" : "d") . ":";
echo $parent->getDeclaringClass()->getName() . ":";
echo ($parent->hasHook($getCase) ? "PG" : "pg") . ":";
echo ($parent->hasHook($setCase) ? "bad" : "ps");');
"#,
    );
    assert_eq!(
        out,
        "H:P:2:name:EvalAotReflectPropertyContract:string:hooks:G:S:2:$name::get:$name::set:A:D:EvalAotReflectPropertyParent:PG:ps"
    );
}

/// Verifies eval `#[Override]` can target generated/AOT parent class methods.
#[test]
fn test_eval_declared_class_override_attribute_accepts_aot_parent_method() {
    let out = compile_and_run(
        r#"<?php
class EvalAotOverrideParent {
    public function label(): string { return "parent"; }
}
eval('class EvalAotOverrideChild extends EvalAotOverrideParent {
    #[\Override]
    public function label(): string { return "child"; }
}
echo (new EvalAotOverrideChild())->label();');
"#,
    );
    assert_eq!(out, "child");
}

/// Verifies eval `#[Override]` can target AOT methods through eval parents.
#[test]
fn test_eval_declared_class_override_attribute_accepts_inherited_aot_parent_method() {
    let out = compile_and_run(
        r#"<?php
class EvalAotInheritedOverrideParent {
    public function label(): string { return "parent"; }
}
eval('class EvalAotInheritedOverrideMiddle extends EvalAotInheritedOverrideParent {}
class EvalAotInheritedOverrideChild extends EvalAotInheritedOverrideMiddle {
    #[\Override]
    public function label(): string { return "child"; }
}
echo (new EvalAotInheritedOverrideChild())->label();');
"#,
    );
    assert_eq!(out, "child");
}

/// Verifies eval rejects overriding final generated/AOT parent methods.
#[test]
fn test_eval_declared_class_rejects_final_aot_parent_method_override() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotFinalOverrideParent {
    final public function label(): string { return "parent"; }
}
eval('class EvalAotFinalOverrideChild extends EvalAotFinalOverrideParent {
    public function label(): string { return "child"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects final AOT method overrides through eval parents.
#[test]
fn test_eval_declared_class_rejects_final_inherited_aot_parent_method_override() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotFinalInheritedOverrideParent {
    final public function label(): string { return "parent"; }
}
eval('class EvalAotFinalInheritedOverrideMiddle extends EvalAotFinalInheritedOverrideParent {}
class EvalAotFinalInheritedOverrideChild extends EvalAotFinalInheritedOverrideMiddle {
    public function label(): string { return "child"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects incompatible generated/AOT parent method signatures.
#[test]
fn test_eval_declared_class_rejects_incompatible_aot_parent_method_signature() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotParentSignatureBase {
    public function label(string $name): string { return $name; }
}
eval('class EvalAotParentSignatureChild extends EvalAotParentSignatureBase {
    public function label(int $name): string { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval concrete classes can implement generated/AOT abstract parent methods.
#[test]
fn test_eval_declared_class_implements_aot_abstract_parent_method() {
    let out = compile_and_run(
        r#"<?php
abstract class EvalAotAbstractMethodBaseOk {
    abstract public function label(string $name): string;
}
eval('class EvalAotAbstractMethodChildOk extends EvalAotAbstractMethodBaseOk {
    public function label(string $name): string { return $name; }
}
echo (new EvalAotAbstractMethodChildOk())->label("ok");');
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies eval concrete classes must implement generated/AOT abstract parent methods.
#[test]
fn test_eval_declared_class_rejects_missing_aot_abstract_parent_method() {
    let err = compile_and_run_expect_failure(
        r#"<?php
abstract class EvalAotAbstractMethodBaseMissing {
    abstract public function label(string $name): string;
}
eval('class EvalAotAbstractMethodChildMissing extends EvalAotAbstractMethodBaseMissing {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval carries generated/AOT abstract method requirements through eval parents.
#[test]
fn test_eval_declared_class_rejects_missing_aot_abstract_parent_method_via_eval_parent() {
    let err = compile_and_run_expect_failure(
        r#"<?php
abstract class EvalAotAbstractMethodBaseViaEval {
    abstract public function label(): string;
}
eval('abstract class EvalAotAbstractMethodMiddle extends EvalAotAbstractMethodBaseViaEval {}');
eval('class EvalAotAbstractMethodLeaf extends EvalAotAbstractMethodMiddle {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval checks generated/AOT abstract signatures through eval parents.
#[test]
fn test_eval_declared_class_rejects_incompatible_aot_abstract_parent_method_via_eval_parent() {
    let err = compile_and_run_expect_failure(
        r#"<?php
abstract class EvalAotAbstractMethodBaseViaEvalSignature {
    abstract public function label(string $name): string;
}
eval('abstract class EvalAotAbstractMethodMiddleSignature extends EvalAotAbstractMethodBaseViaEvalSignature {}');
eval('class EvalAotAbstractMethodLeafSignature extends EvalAotAbstractMethodMiddleSignature {
    public function label(int $name): string { return "bad"; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects overriding final generated/AOT parent properties.
#[test]
fn test_eval_declared_class_rejects_final_aot_parent_property_override() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotFinalPropertyBase {
    final public int $value = 1;
}
eval('class EvalAotFinalPropertyChild extends EvalAotFinalPropertyBase {
    public int $value = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval validates generated/AOT parent property visibility and storage contracts.
#[test]
fn test_eval_declared_class_rejects_incompatible_aot_parent_property_contracts() {
    for source in [
        r#"<?php
class EvalAotPublicPropertyBase {
    public int $value = 1;
}
eval('class EvalAotProtectedPropertyChild extends EvalAotPublicPropertyBase {
    protected int $value = 2;
}');
"#,
        r#"<?php
class EvalAotStaticPropertyBase {
    public static int $value = 1;
}
eval('class EvalAotInstancePropertyChild extends EvalAotStaticPropertyBase {
    public int $value = 2;
}');
"#,
        r#"<?php
class EvalAotReadonlyPropertyBase {
    public readonly int $value;
}
eval('class EvalAotMutablePropertyChild extends EvalAotReadonlyPropertyBase {
    public int $value = 2;
}');
"#,
        r#"<?php
class EvalAotPrivateSetPropertyBase {
    public private(set) int $value = 1;
}
eval('class EvalAotPrivateSetPropertyChild extends EvalAotPrivateSetPropertyBase {
    public private(set) int $value = 2;
}');
"#,
    ] {
        let err = compile_and_run_expect_failure(source);
        assert!(
            err.contains("Fatal error: eval() runtime failed"),
            "stderr did not contain eval runtime fatal diagnostic: {err}"
        );
    }
}

/// Verifies eval rejects incompatible generated/AOT parent property types.
#[test]
fn test_eval_declared_class_rejects_incompatible_aot_parent_property_type() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotTypedPropertyBase {
    public string $value = "base";
}
eval('class EvalAotTypedPropertyChild extends EvalAotTypedPropertyBase {
    public int $value = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval concrete classes can implement generated/AOT abstract parent properties.
#[test]
fn test_eval_declared_class_implements_aot_abstract_parent_property() {
    let out = compile_and_run(
        r#"<?php
abstract class EvalAotAbstractPropertyBaseOk {
    abstract public int $id { get; set; }
}
eval('class EvalAotAbstractPropertyChildOk extends EvalAotAbstractPropertyBaseOk {
    public int $id = 7;
}
echo (new EvalAotAbstractPropertyChildOk())->id;');
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval concrete classes must implement generated/AOT abstract parent properties.
#[test]
fn test_eval_declared_class_rejects_missing_aot_abstract_parent_property() {
    let err = compile_and_run_expect_failure(
        r#"<?php
abstract class EvalAotAbstractPropertyBaseMissing {
    abstract public int $id { get; set; }
}
eval('class EvalAotAbstractPropertyChildMissing extends EvalAotAbstractPropertyBaseMissing {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval carries generated/AOT abstract property requirements through eval parents.
#[test]
fn test_eval_declared_class_rejects_missing_aot_abstract_parent_property_via_eval_parent() {
    let err = compile_and_run_expect_failure(
        r#"<?php
abstract class EvalAotAbstractPropertyBaseViaEval {
    abstract public int $id { get; set; }
}
eval('abstract class EvalAotAbstractPropertyMiddle extends EvalAotAbstractPropertyBaseViaEval {}');
eval('class EvalAotAbstractPropertyLeaf extends EvalAotAbstractPropertyMiddle {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval checks generated/AOT abstract property types through eval parents.
#[test]
fn test_eval_declared_class_rejects_incompatible_aot_abstract_parent_property_via_eval_parent() {
    let err = compile_and_run_expect_failure(
        r#"<?php
abstract class EvalAotAbstractPropertyBaseViaEvalType {
    abstract public string $name { get; set; }
}
eval('abstract class EvalAotAbstractPropertyMiddleType extends EvalAotAbstractPropertyBaseViaEvalType {}');
eval('class EvalAotAbstractPropertyLeafType extends EvalAotAbstractPropertyMiddleType {
    public int $name = 1;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval honors get-only versus get/set generated/AOT abstract property contracts.
#[test]
fn test_eval_declared_class_validates_aot_abstract_parent_property_set_requirement() {
    let out = compile_and_run(
        r#"<?php
abstract class EvalAotAbstractPropertyGetOnlyBase {
    abstract public int $id { get; }
}
eval('class EvalAotAbstractPropertyReadonlyChild extends EvalAotAbstractPropertyGetOnlyBase {
    public readonly int $id;
    public function __construct(int $id) { $this->id = $id; }
}
echo (new EvalAotAbstractPropertyReadonlyChild(9))->id;');
"#,
    );
    assert_eq!(out, "9");

    let err = compile_and_run_expect_failure(
        r#"<?php
abstract class EvalAotAbstractPropertyGetSetBase {
    abstract public int $id { get; set; }
}
eval('class EvalAotAbstractPropertyReadonlyBadChild extends EvalAotAbstractPropertyGetSetBase {
    public readonly int $id;
    public function __construct(int $id) { $this->id = $id; }
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval rejects global builtin attributes on unsupported OOP targets.
#[test]
fn test_eval_declared_builtin_attribute_target_validation() {
    let cases = [
        r#"eval('#[\AllowDynamicProperties] interface EvalInvalidAttrInterface {}');"#,
        r#"eval('class EvalInvalidAttrProperty { #[\Override] public int $value; }');"#,
        r#"eval('class EvalInvalidAttrMethod { #[\AllowDynamicProperties] public function run() {} }');"#,
    ];

    for source in cases {
        let err = compile_and_run_expect_failure(&format!("<?php\n{source}\n"));

        assert!(
            err.contains("Fatal error: eval() runtime failed"),
            "stderr did not contain eval runtime fatal diagnostic: {err}"
        );
    }
}

/// Verifies eval object-method callable arrays bind named arguments.
#[test]
fn test_eval_declared_object_method_callable_array_named_args() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalObjectCallableArrayBox {
    public function join($left, $right) {
        return $left . $right;
    }
}
$box = new EvalObjectCallableArrayBox();
$cb = [$box, "join"];
echo is_callable($cb) ? "Y:" : "N:";
echo call_user_func_array($cb, ["right" => "B", "left" => "A"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Y:AB");
}

/// Verifies eval-declared class constants work through the bridge.
#[test]
fn test_eval_declared_class_constants_and_scoped_fetches() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstBase {
    public const SEED = 2;
    protected const HIDDEN = 5;
    public static function read() {
        return self::SEED + static::SEED;
    }
    public static function hidden() {
        return self::HIDDEN;
    }
}
class EvalConstChild extends EvalConstBase {
    public const SEED = 7;
    public static function readParent() {
        return parent::SEED;
    }
}
echo EvalConstBase::SEED . ":";
echo EvalConstChild::SEED . ":";
echo EvalConstChild::read() . ":";
echo EvalConstChild::readParent() . ":";
echo EvalConstChild::hidden();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:7:9:2:5");
}

/// Verifies eval-declared class-like constants support PHP comma-separated declarations.
#[test]
fn test_eval_declared_comma_separated_class_like_constants() {
    let out = compile_and_run_capture(
        r#"<?php
echo eval('class EvalMultiConstClass {
    public const A = 1, B = 2;
}
interface EvalMultiConstIface {
    public const C = 3, D = 4;
}
trait EvalMultiConstTrait {
    public const E = 5, F = 6;
}
class EvalMultiConstTraitBox {
    use EvalMultiConstTrait;
}
enum EvalMultiConstEnum {
    public const G = 7, H = 8;
    case Ready;
}
echo EvalMultiConstClass::A . EvalMultiConstClass::B . ":";
echo EvalMultiConstIface::C . EvalMultiConstIface::D . ":";
echo EvalMultiConstTraitBox::E . EvalMultiConstTraitBox::F . ":";
return EvalMultiConstEnum::G + EvalMultiConstEnum::H;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "12:34:56:15");
}

/// Verifies eval rejects PHP's reserved `class` class-constant name.
#[test]
fn test_eval_declared_reserved_class_constant_name_fails() {
    for source in [
        r#"<?php
eval('class EvalBadConstName {
    const class = 1;
}');
"#,
        r#"<?php
eval('interface EvalBadIfaceConstName {
    const class = 1;
}');
"#,
        r#"<?php
eval('trait EvalBadTraitConstName {
    const class = 1;
}');
"#,
        r#"<?php
eval('enum EvalBadEnumConstName {
    const class = 1;
}');
"#,
    ] {
        let err = compile_and_run_expect_failure(source);
        assert!(
            err.contains("Fatal error: eval() fragment uses an unsupported construct"),
            "stderr did not contain eval unsupported-construct diagnostic: {err}"
        );
    }
}

/// Asserts one eval fragment rejects a PHP-reserved class-like declaration name.
fn assert_eval_declared_reserved_class_like_name_fails(source: &str) {
    let err = compile_and_run_expect_failure(source);
    assert!(
        err.contains("Fatal error: eval() fragment uses an unsupported construct"),
        "stderr did not contain eval unsupported-construct diagnostic: {err}"
    );
}

/// Verifies eval rejects the reserved `match` class declaration name.
#[test]
fn test_eval_declared_reserved_match_class_name_fails() {
    assert_eval_declared_reserved_class_like_name_fails(
        r#"<?php
eval('class match {}');
"#,
    );
}

/// Verifies eval rejects the reserved `string` class declaration name.
#[test]
fn test_eval_declared_reserved_string_class_name_fails() {
    assert_eval_declared_reserved_class_like_name_fails(
        r#"<?php
eval('class string {}');
"#,
    );
}

/// Verifies eval rejects the reserved `interface` interface declaration name.
#[test]
fn test_eval_declared_reserved_interface_name_fails() {
    assert_eval_declared_reserved_class_like_name_fails(
        r#"<?php
eval('interface interface {}');
"#,
    );
}

/// Verifies eval rejects the reserved `readonly` trait declaration name.
#[test]
fn test_eval_declared_reserved_readonly_trait_name_fails() {
    assert_eval_declared_reserved_class_like_name_fails(
        r#"<?php
eval('trait readonly {}');
"#,
    );
}

/// Verifies eval rejects the reserved `bool` enum declaration name.
#[test]
fn test_eval_declared_reserved_bool_enum_name_fails() {
    assert_eval_declared_reserved_class_like_name_fails(
        r#"<?php
eval('enum bool { case Ready; }');
"#,
    );
}

/// Asserts one eval fragment rejects a PHP-reserved bare class-like reference name.
fn assert_eval_reserved_class_like_reference_name_fails(source: &str) {
    let err = compile_and_run_expect_failure(source);
    assert!(
        err.contains("Fatal error: eval() fragment uses an unsupported construct"),
        "stderr did not contain eval unsupported-construct diagnostic: {err}"
    );
}

/// Verifies eval rejects a reserved class name in an `extends` reference.
#[test]
fn test_eval_reserved_extends_class_reference_name_fails() {
    assert_eval_reserved_class_like_reference_name_fails(
        r#"<?php
eval('class EvalBadExtends extends match {}');
"#,
    );
}

/// Verifies eval rejects a reserved class name in an `implements` reference.
#[test]
fn test_eval_reserved_implements_class_reference_name_fails() {
    assert_eval_reserved_class_like_reference_name_fails(
        r#"<?php
eval('class EvalBadImplements implements match {}');
"#,
    );
}

/// Verifies eval rejects a reserved trait name in a `use` reference.
#[test]
fn test_eval_reserved_trait_use_reference_name_fails() {
    assert_eval_reserved_class_like_reference_name_fails(
        r#"<?php
eval('class EvalBadTraitUse { use match; }');
"#,
    );
}

/// Verifies eval rejects a reserved class name in a `new` expression.
#[test]
fn test_eval_reserved_new_class_reference_name_fails() {
    assert_eval_reserved_class_like_reference_name_fails(
        r#"<?php
eval('$box = new match();');
"#,
    );
}

/// Verifies eval rejects a reserved class name in an `instanceof` expression.
#[test]
fn test_eval_reserved_instanceof_class_reference_name_fails() {
    assert_eval_reserved_class_like_reference_name_fails(
        r#"<?php
eval('$ok = $box instanceof match;');
"#,
    );
}

/// Verifies eval-declared final class constants cannot be redeclared.
#[test]
fn test_eval_declared_final_class_constant_override_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalFinalConstBase {
    final public const SEED = 1;
}
class EvalFinalConstChild extends EvalFinalConstBase {
    public const SEED = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared classes cannot redeclare final generated/AOT class constants.
#[test]
fn test_eval_declared_class_rejects_final_aot_parent_constant_override() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotFinalConstBase {
    final public const SEED = 1;
}
eval('class EvalAotFinalConstChild extends EvalAotFinalConstBase {
    public const SEED = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared class constants preserve PHP visibility redeclaration rules.
#[test]
fn test_eval_declared_class_constant_visibility_contracts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstVisibilityBase {
    protected const SEED = 2;
}
class EvalConstVisibilityChild extends EvalConstVisibilityBase {
    public const SEED = 7;
}
interface EvalConstVisibilityIface {
    public const TOKEN = 3;
}
class EvalConstVisibilityImpl implements EvalConstVisibilityIface {
    public const TOKEN = 5;
}
echo EvalConstVisibilityChild::SEED . ":";
echo EvalConstVisibilityImpl::TOKEN;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:5");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalConstPublicBase {
    public const SEED = 1;
}
class EvalConstProtectedChild extends EvalConstPublicBase {
    protected const SEED = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalConstPublicContract {
    public const SEED = 1;
}
class EvalConstProtectedImpl implements EvalConstPublicContract {
    protected const SEED = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('interface EvalConstProtectedIface {
    protected const SEED = 1;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() fragment uses an unsupported construct"),
        "stderr did not contain eval unsupported-construct diagnostic: {err}"
    );
}

/// Verifies eval-declared class constants honor generated/AOT visibility contracts.
#[test]
fn test_eval_declared_class_rejects_reduced_aot_parent_constant_visibility() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotConstPublicBase {
    public const SEED = 1;
}
eval('class EvalAotConstProtectedChild extends EvalAotConstPublicBase {
    protected const SEED = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared classes cannot redeclare final generated/AOT interface constants.
#[test]
fn test_eval_declared_class_rejects_final_aot_interface_constant_override() {
    let err = compile_and_run_expect_failure(
        r#"<?php
interface EvalAotFinalConstContract {
    final public const TOKEN = 1;
}
eval('class EvalAotFinalConstImpl implements EvalAotFinalConstContract {
    public const TOKEN = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared interfaces cannot redeclare final generated/AOT interface constants.
#[test]
fn test_eval_declared_interface_rejects_final_aot_parent_constant_override() {
    let err = compile_and_run_expect_failure(
        r#"<?php
interface EvalAotFinalParentContract {
    final public const TOKEN = 1;
}
eval('interface EvalAotFinalChildContract extends EvalAotFinalParentContract {
    public const TOKEN = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared final private class constants are rejected.
#[test]
fn test_eval_declared_final_private_class_constant_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalFinalPrivateConst {
    final private const SEED = 1;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval class-name literals work for class-like receivers.
#[test]
fn test_eval_declared_class_name_literals() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalClassNameBase {
    public static function selfName() { return self::class; }
    public static function staticName() { return static::class; }
}
class EvalClassNameChild extends EvalClassNameBase {}
interface EvalClassNameIface {}
trait EvalClassNameTrait {}
echo EvalClassNameChild::class . ":";
echo EvalClassNameIface::class . ":";
echo EvalClassNameTrait::class . ":";
echo EvalClassNameChild::selfName() . ":";
echo EvalClassNameChild::staticName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalClassNameChild:EvalClassNameIface:EvalClassNameTrait:EvalClassNameBase:EvalClassNameChild"
    );
}

/// Verifies eval-declared class attributes expose names and supported literal args.
#[test]
fn test_eval_declared_class_attribute_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalAttrDep {}
#[Route("/home", -1, 1.5, true, null, EvalAttrDep::class, ["nested", 2])]
#[Tag("first"), Tag("second")]
class EvalAttrMeta {}
$names = class_attribute_names("EvalAttrMeta");
echo count($names) . ":" . $names[0] . ":" . $names[1] . ":" . $names[2] . ":";
$args = class_attribute_args("EvalAttrMeta", "route");
echo count($args) . ":" . $args[0] . ":" . $args[1] . ":";
echo $args[2] . ":" . ($args[3] ? "T" : "F") . ":" . (is_null($args[4]) ? "N" : "bad") . ":";
echo $args[5] . ":";
echo count($args[6]) . ":" . $args[6][0] . ":" . $args[6][1] . ":";
$tag = class_attribute_args("evalattrmeta", "Tag");
echo $tag[0] . ":";
$attrs = class_get_attributes("EvalAttrMeta");
echo count($attrs) . ":" . $attrs[0]->getName() . ":";
$attrArgs = $attrs[0]->getArguments();
echo count($attrArgs) . ":" . $attrArgs[0] . ":" . $attrArgs[1] . ":";
echo $attrArgs[2] . ":" . ($attrArgs[3] ? "T" : "F") . ":" . (is_null($attrArgs[4]) ? "N" : "bad") . ":";
echo $attrArgs[5] . ":";
echo count($attrArgs[6]) . ":" . $attrArgs[6][0] . ":" . $attrArgs[6][1] . ":";
$tagArgs = $attrs[1]->getArguments();
echo $attrs[1]->getName() . ":" . $tagArgs[0] . ":";
echo is_null($attrs[0]->newInstance()) ? "N" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "3:Route:Tag:Tag:7:/home:-1:1.5:T:N:EvalAttrDep:2:nested:2:first:3:Route:7:/home:-1:1.5:T:N:EvalAttrDep:2:nested:2:Tag:first:N"
    );
}

/// Verifies eval attribute array arguments preserve string keys.
#[test]
fn test_eval_declared_class_attribute_string_keyed_array_args() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalArrayAttribute {
    public $items;
    public function __construct($items) {
        $this->items = $items;
    }
}
#[EvalArrayAttribute(["plain", "name" => "Ada", "nested" => ["inner" => "value"]])]
class EvalArrayAttributeTarget {}
$args = class_attribute_args("EvalArrayAttributeTarget", "EvalArrayAttribute");
$items = $args[0];
echo count($items) . ":" . $items[0] . ":" . $items["name"] . ":" . $items["nested"]["inner"] . ":";
$attr = class_get_attributes("EvalArrayAttributeTarget")[0];
$attrItems = $attr->getArguments()[0];
echo count($attrItems) . ":" . $attrItems[0] . ":" . $attrItems["name"] . ":" . $attrItems["nested"]["inner"] . ":";
$instanceItems = $attr->newInstance()->items;
echo count($instanceItems) . ":" . $instanceItems[0] . ":" . $instanceItems["name"] . ":" . $instanceItems["nested"]["inner"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "3:plain:Ada:value:3:plain:Ada:value:3:plain:Ada:value"
    );
}

/// Verifies eval attribute array arguments preserve PHP-normalized scalar keys.
#[test]
fn test_eval_declared_class_attribute_scalar_keyed_array_args() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalScalarKeyAttribute {
    public $items;
    public function __construct($items) {
        $this->items = $items;
    }
}
#[EvalScalarKeyAttribute([2 => "two", true => "bool", null => "null", 1.8 => "float", "name" => "Ada"])]
class EvalScalarKeyAttributeTarget {}
$args = class_attribute_args("EvalScalarKeyAttributeTarget", "EvalScalarKeyAttribute");
$items = $args[0];
echo count($items) . ":" . $items[2] . ":" . $items[1] . ":" . $items[""] . ":" . $items["name"] . ":";
$attr = class_get_attributes("EvalScalarKeyAttributeTarget")[0];
$attrItems = $attr->getArguments()[0];
echo count($attrItems) . ":" . $attrItems[2] . ":" . $attrItems[1] . ":" . $attrItems[""] . ":" . $attrItems["name"] . ":";
$instanceItems = $attr->newInstance()->items;
echo count($instanceItems) . ":" . $instanceItems[2] . ":" . $instanceItems[1] . ":" . $instanceItems[""] . ":" . $instanceItems["name"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "4:two:float:null:Ada:4:two:float:null:Ada:4:two:float:null:Ada"
    );
}

/// Verifies eval can read generated/AOT float attribute arguments.
#[test]
fn test_eval_reflection_class_exposes_aot_float_attribute_args() {
    let out = compile_and_run_capture(
        r#"<?php
#[EvalAotFloatAttr(1.5, value: -2.25)]
class EvalAotFloatAttrTarget {}
echo eval('$args = class_attribute_args("EvalAotFloatAttrTarget", "EvalAotFloatAttr");
echo count($args) . ":" . $args[0] . ":" . $args["value"] . ":";
$attrs = class_get_attributes("EvalAotFloatAttrTarget");
$attrArgs = $attrs[0]->getArguments();
echo count($attrArgs) . ":" . $attrArgs[0] . ":" . $attrArgs["value"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:1.5:-2.25:2:1.5:-2.25");
}

/// Verifies eval ReflectionAttribute::newInstance builds eval-declared attribute objects.
#[test]
fn test_eval_reflection_attribute_new_instance_for_eval_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalRoute {
    public $path;
    public $code;
    public $enabled;
    public function __construct($path, $code, $enabled) {
        $this->path = $path;
        $this->code = $code;
        $this->enabled = $enabled;
    }
    public function summary() {
        return $this->path . ":" . $this->code . ":" . ($this->enabled ? "T" : "F");
    }
}
#[EvalRoute("/home", -7, true)]
class EvalRouteTarget {}
$attrs = class_get_attributes("EvalRouteTarget");
$instance = $attrs[0]->newInstance();
echo get_class($instance) . ":" . $instance->summary();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalRoute:/home:-7:T");
}

/// Verifies eval ReflectionClass/Method/Property expose eval-declared attributes.
#[test]
fn test_eval_reflection_member_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMarker {
    public $name;
    public function __construct($name) {
        $this->name = $name;
    }
    public function label() {
        return $this->name;
    }
}
#[EvalMarker("class")]
class EvalReflectTarget {
    #[EvalMarker("method")]
    public function handle() {}
    #[EvalMarker("property")]
    public $id;
}
$classAttrs = (new ReflectionClass("EvalReflectTarget"))->getAttributes();
echo count($classAttrs) . ":" . (new ReflectionClass("EvalReflectTarget"))->getName() . ":";
echo $classAttrs[0]->getName() . ":" . $classAttrs[0]->newInstance()->label() . ":";
$methodAttrs = (new ReflectionMethod("EvalReflectTarget", "handle"))->getAttributes();
echo count($methodAttrs) . ":" . (new ReflectionMethod("EvalReflectTarget", "handle"))->getName() . ":";
echo $methodAttrs[0]->getName() . ":";
echo $methodAttrs[0]->getArguments()[0] . ":" . $methodAttrs[0]->newInstance()->label() . ":";
$propertyAttrs = (new ReflectionProperty("EvalReflectTarget", "id"))->getAttributes();
echo count($propertyAttrs) . ":" . (new ReflectionProperty("EvalReflectTarget", "id"))->getName() . ":";
echo $propertyAttrs[0]->getName() . ":";
echo $propertyAttrs[0]->getArguments()[0] . ":" . $propertyAttrs[0]->newInstance()->label();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalReflectTarget:EvalMarker:class:1:handle:EvalMarker:method:method:1:id:EvalMarker:property:property"
    );
}

/// Verifies eval ReflectionClass/Method/Function expose source-location metadata.
#[test]
fn test_eval_reflection_source_locations() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectSourceE2E {
    public function run() {
        return 1;
    }
}
function eval_reflect_source_e2e() {
    return 1;
}
$class = new ReflectionClass("EvalReflectSourceE2E");
$method = new ReflectionMethod("EvalReflectSourceE2E", "run");
$function = new ReflectionFunction("eval_reflect_source_e2e");
echo $class->getFileName() === false ? "f" : "F"; echo ":";
echo $class->getStartLine(); echo ":"; echo $class->getEndLine(); echo ":";
echo $method->getStartLine(); echo ":"; echo $method->getEndLine(); echo ":";
echo $function->getStartLine(); echo ":"; echo $function->getEndLine();');
"#,
    );
    assert_eq!(out, "F:1:5:2:4:6:8");
}

/// Verifies eval ReflectionClass exposes generated/AOT source-location metadata.
#[test]
fn test_eval_reflection_aot_class_source_locations() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectClassSourceE2E {
    public function run() { return 1; }
}
interface EvalAotReflectInterfaceSourceE2E {}
trait EvalAotReflectTraitSourceE2E {}
enum EvalAotReflectEnumSourceE2E { case One; }
echo eval('$class = new ReflectionClass("EvalAotReflectClassSourceE2E");
echo $class->getFileName() === false ? "f" : "F"; echo ":";
echo $class->getStartLine(); echo ":"; echo $class->getEndLine(); echo ":";
$interface = new ReflectionClass("EvalAotReflectInterfaceSourceE2E");
echo $interface->getFileName() === false ? "f" : "F"; echo ":";
echo $interface->getStartLine(); echo ":"; echo $interface->getEndLine(); echo ":";
$trait = new ReflectionClass("EvalAotReflectTraitSourceE2E");
echo $trait->getFileName() === false ? "f" : "F"; echo ":";
echo $trait->getStartLine(); echo ":"; echo $trait->getEndLine(); echo ":";
$enum = new ReflectionClass("EvalAotReflectEnumSourceE2E");
echo $enum->getFileName() === false ? "f" : "F"; echo ":";
echo $enum->getStartLine(); echo ":"; echo $enum->getEndLine();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "F:2:2:F:5:5:F:6:6:F:7:7");
}

/// Verifies eval ReflectionMethod exposes generated/AOT source-location metadata.
#[test]
fn test_eval_reflection_aot_method_source_locations() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectSourceE2E {
    public function run() { return 1; }
}
echo eval('$method = new ReflectionMethod("EvalAotReflectSourceE2E", "run");
echo $method->getFileName() === false ? "f" : "F"; echo ":";
echo $method->getStartLine(); echo ":"; echo $method->getEndLine();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "F:3:3");
}

/// Verifies eval ReflectionAttribute exposes owner target and repetition metadata.
#[test]
fn test_eval_reflection_attribute_target_and_repetition() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalTargetMarker {
    public function __construct($name = null) {}
}
#[EvalTargetMarker("class-a"), EvalTargetMarker("class-b")]
class EvalReflectAttributeTarget {
    #[EvalTargetMarker("method")]
    public function run(#[EvalTargetMarker("param")] $id) {}
    #[EvalTargetMarker("property")]
    public $id;
    #[EvalTargetMarker("const")]
    public const ANSWER = 42;
}
enum EvalReflectAttributeEnum {
    #[EvalTargetMarker("case")]
    case Ready;
}
$classAttrs = (new ReflectionClass("EvalReflectAttributeTarget"))->getAttributes();
echo $classAttrs[0]->getTarget() . "/" . ($classAttrs[0]->isRepeated() ? "R" : "r") . ":";
echo $classAttrs[1]->getTarget() . "/" . ($classAttrs[1]->isRepeated() ? "R" : "r") . ":";
$methodAttr = (new ReflectionMethod("EvalReflectAttributeTarget", "run"))->getAttributes()[0];
echo $methodAttr->getTarget() . "/" . ($methodAttr->isRepeated() ? "R" : "r") . ":";
$propertyAttr = (new ReflectionProperty("EvalReflectAttributeTarget", "id"))->getAttributes()[0];
echo $propertyAttr->getTarget() . "/" . ($propertyAttr->isRepeated() ? "R" : "r") . ":";
$paramAttr = (new ReflectionMethod("EvalReflectAttributeTarget", "run"))->getParameters()[0]->getAttributes()[0];
echo $paramAttr->getTarget() . "/" . ($paramAttr->isRepeated() ? "R" : "r") . ":";
$constAttr = (new ReflectionClassConstant("EvalReflectAttributeTarget", "ANSWER"))->getAttributes()[0];
echo $constAttr->getTarget() . "/" . ($constAttr->isRepeated() ? "R" : "r") . ":";
$caseAttr = (new ReflectionEnumUnitCase("EvalReflectAttributeEnum", "Ready"))->getAttributes()[0];
echo $caseAttr->getTarget() . "/" . ($caseAttr->isRepeated() ? "R" : "r") . ":";
echo method_exists($classAttrs[0], "getTarget") ? "Y" : "n";
echo method_exists($classAttrs[0], "isRepeated") ? "Y" : "n";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1/R:1/R:4/r:8/r:32/r:16/r:16/r:YY");
}

/// Verifies eval ReflectionClass exposes namespace-derived class-name parts.
#[test]
fn test_eval_reflection_class_name_parts() {
    let out = compile_and_run_capture(
        r#"<?php
eval('namespace Eval\Ns;
class Thing {}
$ref = new \ReflectionClass(Thing::class);
echo $ref->getName() . ":";
echo $ref->getShortName() . ":";
echo $ref->getNamespaceName() . ":";
echo $ref->inNamespace() ? "Y" : "N";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Eval\\Ns\\Thing:Thing:Eval\\Ns:Y");
}

/// Verifies eval ReflectionClass exposes implemented interface and used trait names.
#[test]
fn test_eval_reflection_class_relation_names() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalRelationIface {}
trait EvalRelationTrait {
    public function primary() {}
}
trait EvalRelationOtherTrait {
    public function other() {}
}
class EvalRelationTarget implements EvalRelationIface {
    use EvalRelationTrait, EvalRelationOtherTrait {
        EvalRelationTrait::primary as relationAlias;
        EvalRelationOtherTrait::other as private hiddenOther;
        EvalRelationOtherTrait::other as protected;
    }
}
class EvalRelationInherited extends EvalRelationTarget {}
interface EvalRelationParent {}
interface EvalRelationChild extends EvalRelationParent {}
$ref = new ReflectionClass("EvalRelationTarget");
$interfaces = $ref->getInterfaceNames();
$traits = $ref->getTraitNames();
echo count($interfaces) . ":" . $interfaces[0] . ":";
echo count($traits) . ":" . $traits[0] . ":" . $traits[1] . ":";
$parentInterfaces = (new ReflectionClass("EvalRelationChild"))->getInterfaceNames();
echo count($parentInterfaces) . ":" . $parentInterfaces[0] . ":";
$interfaceObjects = $ref->getInterfaces();
echo count($interfaceObjects) . ":" . $interfaceObjects["EvalRelationIface"]->getName() . ":";
$traitObjects = $ref->getTraits();
echo count($traitObjects) . ":" . $traitObjects["EvalRelationTrait"]->getName() . ":" . $traitObjects["EvalRelationOtherTrait"]->getName() . ":";
$parentInterfaceObjects = (new ReflectionClass("EvalRelationChild"))->getInterfaces();
echo count($parentInterfaceObjects) . ":" . $parentInterfaceObjects["EvalRelationParent"]->getName() . ":";
$aliases = $ref->getTraitAliases();
echo count($aliases) . ":" . $aliases["relationAlias"] . ":" . $aliases["hiddenOther"] . ":";
$inheritedAliases = (new ReflectionClass("EvalRelationInherited"))->getTraitAliases();
echo count($inheritedAliases);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalRelationIface:2:EvalRelationTrait:EvalRelationOtherTrait:1:EvalRelationParent:1:EvalRelationIface:2:EvalRelationTrait:EvalRelationOtherTrait:1:EvalRelationParent:2:EvalRelationTrait::primary:EvalRelationOtherTrait::other:0"
    );
}

/// Verifies eval ReflectionClass exposes generated/AOT implemented interface names.
#[test]
fn test_eval_reflection_class_get_interface_names_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotReflectIfaceBase {}
interface EvalAotReflectIfaceChild extends EvalAotReflectIfaceBase {}
class EvalAotReflectIfaceTarget implements EvalAotReflectIfaceChild {}
eval('$interfaces = (new ReflectionClass("EvalAotReflectIfaceTarget"))->getInterfaceNames();
sort($interfaces);
echo count($interfaces) . ":";
echo implode(",", $interfaces) . ":";
$interfaceObjects = (new ReflectionClass("EvalAotReflectIfaceTarget"))->getInterfaces();
ksort($interfaceObjects);
echo count($interfaceObjects) . ":" . implode(",", array_keys($interfaceObjects)) . ":";
echo $interfaceObjects["EvalAotReflectIfaceBase"]->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "2:EvalAotReflectIfaceBase,EvalAotReflectIfaceChild:2:EvalAotReflectIfaceBase,EvalAotReflectIfaceChild:EvalAotReflectIfaceBase"
    );
}

/// Verifies eval ReflectionClass exposes generated/AOT direct trait-use metadata.
#[test]
fn test_eval_reflection_class_get_trait_names_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
trait EvalAotReflectRelationInnerTrait {}
trait EvalAotReflectRelationOuterTrait {
    use EvalAotReflectRelationInnerTrait;
}
class EvalAotReflectRelationTarget {
    use EvalAotReflectRelationOuterTrait;
}
eval('$ref = new ReflectionClass("EvalAotReflectRelationTarget");
$traits = $ref->getTraitNames();
echo count($traits) . ":" . $traits[0] . ":";
$traitObjects = $ref->getTraits();
echo count($traitObjects) . ":" . $traitObjects["EvalAotReflectRelationOuterTrait"]->getName() . ":";
$nestedTraits = (new ReflectionClass("EvalAotReflectRelationOuterTrait"))->getTraitNames();
echo count($nestedTraits) . ":" . $nestedTraits[0];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalAotReflectRelationOuterTrait:1:EvalAotReflectRelationOuterTrait:1:EvalAotReflectRelationInnerTrait"
    );
}

/// Verifies eval ReflectionClass exposes generated/AOT direct trait aliases.
#[test]
fn test_eval_reflection_class_get_trait_aliases_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
trait EvalAotReflectAliasTrait {
    public function original() {}
}
class EvalAotReflectAliasTarget {
    use EvalAotReflectAliasTrait {
        original as aliasOriginal;
    }
}
eval('$aliases = (new ReflectionClass("EvalAotReflectAliasTarget"))->getTraitAliases();
echo count($aliases) . ":" . $aliases["aliasOriginal"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1:EvalAotReflectAliasTrait::original");
}

/// Verifies eval ReflectionClass exposes generated/AOT enum trait metadata.
#[test]
fn test_eval_reflection_class_get_trait_metadata_for_aot_enum() {
    if !codegen_fixture_uses_ir_backend() {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
trait EvalAotReflectEnumTrait {
    public function original() {}
}
enum EvalAotReflectTraitEnum {
    use EvalAotReflectEnumTrait {
        original as aliasOriginal;
    }
    case Ready;
}
eval('$ref = new ReflectionClass("EvalAotReflectTraitEnum");
$traits = $ref->getTraitNames();
$aliases = $ref->getTraitAliases();
echo count($traits) . ":" . implode(",", $traits) . ":";
echo count($aliases) . ":" . $aliases["aliasOriginal"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalAotReflectEnumTrait:1:EvalAotReflectEnumTrait::original"
    );
}

/// Verifies eval can fetch a generated/AOT enum case only referenced inside eval source.
#[test]
fn test_eval_fetches_aot_enum_case_object_from_eval_only_reference() {
    if !codegen_fixture_uses_ir_backend() {
        return;
    }
    let out = compile_and_run_capture(
        r#"<?php
trait EvalAotCaseTrait {
    public function marker() {}
}
enum EvalAotCaseEnum {
    use EvalAotCaseTrait;
    case Ready;
}
eval('$case = EvalAotCaseEnum::Ready;
echo get_class($case) . ":";
$uses = class_uses($case);
ksort($uses);
echo count($uses) . ":" . $uses["EvalAotCaseTrait"] . ":";
$stringUses = class_uses("EvalAotCaseEnum");
ksort($stringUses);
echo count($stringUses) . ":" . $stringUses["EvalAotCaseTrait"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotCaseEnum:1:EvalAotCaseTrait:1:EvalAotCaseTrait"
    );
}

/// Verifies generated enum trait metadata emits one runtime reflection row per direct trait.
#[test]
fn test_eval_reflection_class_trait_metadata_for_aot_enum_is_not_duplicated() {
    if !codegen_fixture_uses_ir_backend() {
        return;
    }
    let dir = make_cli_test_dir("elephc_eval_aot_enum_trait_metadata");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
trait EvalAotReflectEnumTrait {
    public function original() {}
}
enum EvalAotReflectTraitEnum {
    use EvalAotReflectEnumTrait {
        original as aliasOriginal;
    }
    case Ready;
}
eval('$ref = new ReflectionClass("EvalAotReflectTraitEnum");
$traits = $ref->getTraitNames();
$aliases = $ref->getTraitAliases();
echo count($traits) . ":" . implode(",", $traits) . ":";
echo count($aliases) . ":" . $aliases["aliasOriginal"];');
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let start = user_asm
        .find("_eval_reflection_class_traits:\n")
        .expect("missing trait metadata table");
    let count_start = user_asm
        .find("_eval_reflection_class_trait_count:\n")
        .expect("missing trait metadata count");
    let count_tail = &user_asm[count_start..start];
    let tail = &user_asm[start..];
    let end = tail
        .find(".globl _eval_reflection_class_trait_alias_count")
        .expect("missing trait alias metadata table");
    let trait_table = &tail[..end];

    assert!(
        count_tail.contains("    .quad 1\n"),
        "unexpected trait table count:\n{count_tail}\n{trait_table}"
    );

    assert_eq!(
        trait_table
            .matches(".ascii \"EvalAotReflectTraitEnum\"")
            .count(),
        1,
        "{trait_table}"
    );
}

/// Verifies eval ReflectionClass::implementsInterface reports class, enum, and
/// interface metadata through the bridge.
#[test]
fn test_eval_reflection_class_implements_interface_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalImplBase {}
interface EvalImplChild extends EvalImplBase {}
class EvalImplTarget implements EvalImplChild {}
enum EvalImplEnum implements EvalImplBase { case Ready; }
trait EvalImplTrait {}
echo (new ReflectionClass("EvalImplTarget"))->implementsInterface("EvalImplChild") ? "C" : "c";
echo (new ReflectionClass("EvalImplTarget"))->implementsInterface("evalimplbase") ? "B" : "b";
echo (new ReflectionClass("EvalImplEnum"))->implementsInterface("EvalImplBase") ? "E" : "e";
echo (new ReflectionClass("EvalImplChild"))->implementsInterface("EvalImplChild") ? "I" : "i";
echo (new ReflectionClass("EvalImplChild"))->implementsInterface("EvalImplBase") ? "P" : "p";
echo (new ReflectionClass("EvalImplTrait"))->implementsInterface("EvalImplBase") ? "T" : "t";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "CBEIPt");
}

/// Verifies eval ReflectionClass::implementsInterface uses generated/AOT relations.
#[test]
fn test_eval_reflection_class_implements_interface_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotReflectImplBase {}
interface EvalAotReflectImplChild extends EvalAotReflectImplBase {}
class EvalAotReflectImplTarget implements EvalAotReflectImplChild {}
eval('$ref = new ReflectionClass("EvalAotReflectImplTarget");
echo $ref->implementsInterface("EvalAotReflectImplChild") ? "C" : "c";
echo $ref->implementsInterface("evalaotreflectimplbase") ? "B" : "b";
echo $ref->implementsInterface("Iterator") ? "I" : "i";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "CBi");
}

/// Verifies eval-declared children expose interfaces inherited from AOT parents.
#[test]
fn test_eval_declared_class_reflects_aot_parent_interfaces() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotParentMarkerBase {}
interface EvalAotParentMarkerChild extends EvalAotParentMarkerBase {}
class EvalAotParentMarkerRoot implements EvalAotParentMarkerChild {}
eval('class EvalAotParentMarkerLeaf extends EvalAotParentMarkerRoot {}
$implements = class_implements("EvalAotParentMarkerLeaf");
ksort($implements);
echo implode(",", array_keys($implements)) . ":";
$ref = new ReflectionClass("EvalAotParentMarkerLeaf");
$names = $ref->getInterfaceNames();
sort($names);
echo implode(",", $names) . ":";
$objects = $ref->getInterfaces();
ksort($objects);
echo implode(",", array_keys($objects)) . ":";
echo $ref->implementsInterface("EvalAotParentMarkerChild") ? "C" : "c";
echo $ref->implementsInterface("evalaotparentmarkerbase") ? "B" : "b";
echo $ref->isSubclassOf("EvalAotParentMarkerChild") ? "S" : "s";
echo is_subclass_of("EvalAotParentMarkerLeaf", "EvalAotParentMarkerChild") ? "U" : "u";
$box = new EvalAotParentMarkerLeaf();
echo $box instanceof EvalAotParentMarkerChild ? "I" : "i";');
$box = new EvalAotParentMarkerLeaf();
echo ":";
echo $box instanceof EvalAotParentMarkerChild ? "N" : "n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotParentMarkerBase,EvalAotParentMarkerChild:EvalAotParentMarkerBase,EvalAotParentMarkerChild:EvalAotParentMarkerBase,EvalAotParentMarkerChild:CBSUI:N"
    );
}

/// Verifies eval-declared interfaces expose inherited AOT parent interfaces.
#[test]
fn test_eval_declared_interface_reflects_aot_parent_interfaces() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotInterfaceMarkerBase {}
interface EvalAotInterfaceMarkerChild extends EvalAotInterfaceMarkerBase {}
eval('interface EvalAotInterfaceMarkerLeaf extends EvalAotInterfaceMarkerChild {}
class EvalAotInterfaceMarkerBox implements EvalAotInterfaceMarkerLeaf {}
$ifaceParents = class_implements("EvalAotInterfaceMarkerLeaf");
ksort($ifaceParents);
echo implode(",", array_keys($ifaceParents)) . ":";
$classImplements = class_implements("EvalAotInterfaceMarkerBox");
ksort($classImplements);
echo implode(",", array_keys($classImplements)) . ":";
$names = (new ReflectionClass("EvalAotInterfaceMarkerBox"))->getInterfaceNames();
sort($names);
echo implode(",", $names) . ":";
echo (new ReflectionClass("EvalAotInterfaceMarkerLeaf"))->implementsInterface("EvalAotInterfaceMarkerBase") ? "I" : "i";
echo (new ReflectionClass("EvalAotInterfaceMarkerBox"))->implementsInterface("evalaotinterfacemarkerbase") ? "C" : "c";
echo (new ReflectionClass("EvalAotInterfaceMarkerLeaf"))->isSubclassOf("EvalAotInterfaceMarkerBase") ? "S" : "s";
echo is_subclass_of("EvalAotInterfaceMarkerLeaf", "EvalAotInterfaceMarkerBase") ? "U" : "u";
$box = new EvalAotInterfaceMarkerBox();
echo $box instanceof EvalAotInterfaceMarkerBase ? "N" : "n";
echo is_a($box, "EvalAotInterfaceMarkerBase") ? "A" : "a";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotInterfaceMarkerBase,EvalAotInterfaceMarkerChild:EvalAotInterfaceMarkerBase,EvalAotInterfaceMarkerChild,EvalAotInterfaceMarkerLeaf:EvalAotInterfaceMarkerBase,EvalAotInterfaceMarkerChild,EvalAotInterfaceMarkerLeaf:ICSUNA"
    );
}

/// Verifies eval `ReflectionClass::implementsInterface()` throws ReflectionException
/// for missing or non-interface argument names.
#[test]
fn test_eval_reflection_class_implements_interface_rejects_non_interfaces() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalImplRejectIface {}
interface EvalImplRejectOther {}
class EvalImplRejectTarget implements EvalImplRejectIface {}
class EvalImplRejectClass {}
trait EvalImplRejectTrait {}
enum EvalImplRejectEnum { case Ready; }
$ref = new ReflectionClass("EvalImplRejectTarget");
echo $ref->implementsInterface("EvalImplRejectOther") ? "T" : "F";
try {
    $ref->implementsInterface("EvalImplRejectClass");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectTrait");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectEnum");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("EvalImplRejectMissing");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "F:ReflectionException:EvalImplRejectClass is not an interface:ReflectionException:EvalImplRejectTrait is not an interface:ReflectionException:EvalImplRejectEnum is not an interface:ReflectionException:Interface \"EvalImplRejectMissing\" does not exist"
    );
}

/// Verifies eval ReflectionClass::isSubclassOf reports parent and interface
/// metadata through the linked eval bridge.
#[test]
fn test_eval_reflection_class_is_subclass_of_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalSubclassIface {}
interface EvalSubclassChildIface extends EvalSubclassIface {}
class EvalSubclassBase {}
class EvalSubclassParent extends EvalSubclassBase {}
class EvalSubclassChild extends EvalSubclassParent implements EvalSubclassChildIface {}
trait EvalSubclassTrait {}
enum EvalSubclassEnum implements EvalSubclassIface { case Ready; }
$ref = new ReflectionClass("EvalSubclassChild");
echo $ref->isSubclassOf("EvalSubclassParent") ? "P" : "p";
echo $ref->isSubclassOf("evalsubclassbase") ? "B" : "b";
echo $ref->isSubclassOf("EvalSubclassIface") ? "I" : "i";
echo $ref->isSubclassOf("EvalSubclassChild") ? "S" : "s";
echo (new ReflectionClass("EvalSubclassChildIface"))->isSubclassOf("EvalSubclassIface") ? "J" : "j";
echo (new ReflectionClass("EvalSubclassIface"))->isSubclassOf("EvalSubclassIface") ? "X" : "x";
echo $ref->isSubclassOf("EvalSubclassTrait") ? "T" : "t";
echo $ref->isSubclassOf("EvalSubclassEnum") ? "Q" : "q";
echo (new ReflectionClass("EvalSubclassEnum"))->isSubclassOf("EvalSubclassIface") ? "E" : "e";
try {
    $ref->isSubclassOf("EvalSubclassMissing");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":missing";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PBIsJxtqE:missing");
}

/// Verifies eval ReflectionClass::isSubclassOf can query generated AOT class
/// relations when the reflected class was declared outside the eval fragment.
#[test]
fn test_eval_reflection_class_is_subclass_of_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotSubclassParent {}
class EvalAotSubclassChild extends EvalAotSubclassParent {}
interface EvalAotSubclassIface {}
class EvalAotSubclassImpl implements EvalAotSubclassIface {}
eval('$child = new ReflectionClass("EvalAotSubclassChild");
echo $child->isSubclassOf("EvalAotSubclassParent") ? "P" : "p";
echo $child->isSubclassOf("EvalAotSubclassChild") ? "S" : "s";
$impl = new ReflectionClass("EvalAotSubclassImpl");
echo $impl->isSubclassOf("EvalAotSubclassIface") ? "I" : "i";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PsI");
}

/// Verifies eval ReflectionClass::isInstance reports eval-declared object
/// relations through the linked eval bridge.
#[test]
fn test_eval_reflection_class_is_instance_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalInstanceIface {}
class EvalInstanceBase {}
class EvalInstanceChild extends EvalInstanceBase implements EvalInstanceIface {}
trait EvalInstanceTrait {}
enum EvalInstanceEnum implements EvalInstanceIface { case Ready; }
$base = new ReflectionClass("EvalInstanceBase");
$child = new ReflectionClass("EvalInstanceChild");
$iface = new ReflectionClass("EvalInstanceIface");
$trait = new ReflectionClass("EvalInstanceTrait");
$enum = new ReflectionClass("EvalInstanceEnum");
$childObj = new EvalInstanceChild();
$objectRef = new ReflectionClass($childObj);
echo $objectRef->getName(); echo ":";
echo $objectRef->getParentClass()->getName(); echo ":";
echo $objectRef->isInstance($childObj) ? "O" : "o"; echo ":";
echo $base->isInstance($childObj) ? "B" : "b";
echo $child->isInstance(new EvalInstanceBase()) ? "C" : "c";
echo $iface->isInstance($childObj) ? "I" : "i";
echo $trait->isInstance($childObj) ? "T" : "t";
echo $enum->isInstance(EvalInstanceEnum::Ready) ? "E" : "e";
echo $iface->isInstance(EvalInstanceEnum::Ready) ? "N" : "n";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalInstanceChild:EvalInstanceBase:O:BcItEN");
}

/// Verifies eval ReflectionClass::isInstance can query generated AOT object
/// relations when the reflected class was declared outside the eval fragment.
#[test]
fn test_eval_reflection_class_is_instance_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotInstanceParent {}
class EvalAotInstanceChild extends EvalAotInstanceParent {}
interface EvalAotInstanceIface {}
class EvalAotInstanceImpl implements EvalAotInstanceIface {}
eval('$parent = new ReflectionClass("EvalAotInstanceParent");
echo $parent->isInstance(new EvalAotInstanceChild()) ? "P" : "p";
$child = new ReflectionClass("EvalAotInstanceChild");
echo $child->isInstance(new EvalAotInstanceParent()) ? "C" : "c";
$iface = new ReflectionClass("EvalAotInstanceIface");
$objectRef = new ReflectionClass(new EvalAotInstanceChild());
echo $iface->isInstance(new EvalAotInstanceImpl()) ? "I" : "i"; echo ":";
echo $objectRef->getName(); echo ":";
echo $objectRef->getParentClass()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PcI:EvalAotInstanceChild:EvalAotInstanceParent");
}

/// Verifies eval ReflectionClass::getParentClass crosses the generated runtime bridge.
#[test]
fn test_eval_reflection_class_get_parent_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalBridgeParent {}
class EvalBridgeChild extends EvalBridgeParent {}
$parent = (new ReflectionClass("EvalBridgeChild"))->getParentClass();
echo $parent->getName() . ":";
$root = (new ReflectionClass("EvalBridgeParent"))->getParentClass();
if ($root === false) {
    echo "false";
} else {
    echo "bad";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalBridgeParent:false");
}

/// Verifies eval ReflectionClass::getParentClass materializes generated/AOT parents.
#[test]
fn test_eval_reflection_class_get_parent_class_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectParentBase {}
class EvalAotReflectParentChild extends EvalAotReflectParentBase {}
eval('$parent = (new ReflectionClass("EvalAotReflectParentChild"))->getParentClass();
if ($parent === false) {
    echo "missing";
} else {
    echo $parent->getName();
}
echo ":";
$root = (new ReflectionClass("EvalAotReflectParentBase"))->getParentClass();
echo $root === false ? "false" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "EvalAotReflectParentBase:false");
}

/// Verifies eval ReflectionClass::getConstructor crosses the generated runtime bridge.
#[test]
fn test_eval_reflection_class_get_constructor() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalBridgeCtorBase {
    public function __construct($required, $optional = 2) {}
}
class EvalBridgeCtorChild extends EvalBridgeCtorBase {}
class EvalBridgeCtorPlain {}
interface EvalBridgeCtorInterface {
    public function __construct($required);
}
trait EvalBridgeCtorTrait {
    public function __construct($required, $optional = null, ...$rest) {}
}
$base = (new ReflectionClass("EvalBridgeCtorBase"))->getConstructor();
echo $base->getName() . "/" . $base->getNumberOfParameters();
echo "/" . $base->getNumberOfRequiredParameters() . ":";
$child = (new ReflectionClass("EvalBridgeCtorChild"))->getConstructor();
echo $child->getName() . "/" . $child->getNumberOfParameters();
echo "/" . $child->getNumberOfRequiredParameters() . ":";
$plain = (new ReflectionClass("EvalBridgeCtorPlain"))->getConstructor();
echo $plain === null ? "null" : "bad";
echo ":";
$interface = (new ReflectionClass("EvalBridgeCtorInterface"))->getConstructor();
echo $interface->getName() . "/" . $interface->getNumberOfParameters();
echo "/" . $interface->getNumberOfRequiredParameters() . ":";
$trait = (new ReflectionClass("EvalBridgeCtorTrait"))->getConstructor();
echo $trait->getName() . "/" . $trait->getNumberOfParameters();
echo "/" . $trait->getNumberOfRequiredParameters();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "__construct/2/1:__construct/2/1:null:__construct/1/1:__construct/3/1"
    );
}

/// Verifies eval ReflectionClass reports class-like final and abstract flags.
#[test]
fn test_eval_reflection_class_modifier_flags() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalAbstractReflect {}
final class EvalFinalReflect {}
interface EvalIfaceReflect {}
trait EvalTraitReflect {}
enum EvalEnumReflect { case Ready; }
echo (new ReflectionClass("EvalAbstractReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalAbstractReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalAbstractReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalAbstractReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalAbstractReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalFinalReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalFinalReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalFinalReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalFinalReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalFinalReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalEnumReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalEnumReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalEnumReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalEnumReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalEnumReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalIfaceReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalIfaceReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalIfaceReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalIfaceReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalIfaceReflect"))->isEnum() ? "E" : "e"; echo ":";
echo (new ReflectionClass("EvalTraitReflect"))->isAbstract() ? "A" : "a";
echo (new ReflectionClass("EvalTraitReflect"))->isFinal() ? "F" : "f";
echo (new ReflectionClass("EvalTraitReflect"))->isInterface() ? "I" : "i";
echo (new ReflectionClass("EvalTraitReflect"))->isTrait() ? "T" : "t";
echo (new ReflectionClass("EvalTraitReflect"))->isEnum() ? "E" : "e";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Afite:aFite:aFitE:afIte:afiTe");
}

/// Verifies eval ReflectionClass reports PHP modifier bitmasks through the bridge.
#[test]
fn test_eval_reflection_class_modifier_bitmask() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalModifierAbstract {}
final class EvalModifierFinal {}
readonly class EvalModifierReadonly {}
final readonly class EvalModifierFinalReadonly {}
enum EvalModifierEnum { case Ready; }
interface EvalModifierIface {}
trait EvalModifierTrait {}
echo (new ReflectionClass("EvalModifierAbstract"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierFinal"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierReadonly"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierFinalReadonly"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierEnum"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierIface"))->getModifiers() . ":";
echo (new ReflectionClass("EvalModifierTrait"))->getModifiers();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "64:32:65536:65568:32:0:0");
}

/// Verifies eval ReflectionClass reports readonly class status through the bridge.
#[test]
fn test_eval_reflection_class_readonly_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReadonlyPlain {}
readonly class EvalReadonlyReflect {}
final readonly class EvalReadonlyFinalReflect {}
enum EvalReadonlyEnumReflect { case Ready; }
interface EvalReadonlyIface {}
trait EvalReadonlyTrait {}
echo (new ReflectionClass("EvalReadonlyPlain"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyFinalReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyEnumReflect"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyIface"))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass("EvalReadonlyTrait"))->isReadOnly() ? "R" : "r";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "rRRrrr");
}

/// Verifies eval ReflectionClass reports instantiability through the bridge.
#[test]
fn test_eval_reflection_class_instantiable_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalInstAbstract {}
class EvalInstPublic {}
final class EvalInstFinal {}
class EvalInstPrivate { private function __construct() {} }
class EvalInstProtected { protected function __construct() {} }
interface EvalInstIface {}
trait EvalInstTrait {}
enum EvalInstEnum { case Ready; }
echo (new ReflectionClass("EvalInstAbstract"))->isInstantiable() ? "A" : "a";
echo (new ReflectionClass("EvalInstPublic"))->isInstantiable() ? "B" : "b";
echo (new ReflectionClass("EvalInstFinal"))->isInstantiable() ? "C" : "c";
echo (new ReflectionClass("EvalInstPrivate"))->isInstantiable() ? "P" : "p";
echo (new ReflectionClass("EvalInstProtected"))->isInstantiable() ? "R" : "r";
echo (new ReflectionClass("EvalInstIface"))->isInstantiable() ? "I" : "i";
echo (new ReflectionClass("EvalInstTrait"))->isInstantiable() ? "T" : "t";
echo (new ReflectionClass("EvalInstEnum"))->isInstantiable() ? "E" : "e";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "aBCprite");
}

/// Verifies eval ReflectionClass modifier and lifecycle predicates use generated/AOT class flags.
#[test]
fn test_eval_reflection_class_aot_modifier_flags() {
    let out = compile_and_run(
        r#"<?php
abstract class EvalAotModifierAbstract {}
final class EvalAotModifierFinal {}
readonly class EvalAotModifierReadonly {}
enum EvalAotModifierEnum { case Ready; }

echo eval('
function eval_aot_modifier_line($name) {
    $ref = new ReflectionClass($name);
    echo $ref->isAbstract() ? "A" : "a";
    echo $ref->isFinal() ? "F" : "f";
    echo $ref->isReadOnly() ? "R" : "r";
    echo $ref->isEnum() ? "E" : "e";
    echo "/" . $ref->getModifiers() . "/";
    echo $ref->isInstantiable() ? "I" : "i";
    echo $ref->isCloneable() ? "C" : "c";
    echo ":";
}
eval_aot_modifier_line("EvalAotModifierAbstract");
eval_aot_modifier_line("EvalAotModifierFinal");
eval_aot_modifier_line("EvalAotModifierReadonly");
eval_aot_modifier_line("EvalAotModifierEnum");
');
"#,
    );
    assert_eq!(out, "Afre/64/ic:aFre/32/IC:afRe/65536/IC:aFrE/32/ic:");
}

/// Verifies eval ReflectionClass lifecycle predicates use generated/AOT lifecycle visibility.
#[test]
fn test_eval_reflection_class_aot_lifecycle_visibility_predicates() {
    let out = compile_and_run(
        r#"<?php
class EvalAotInstNoCtor {}
class EvalAotInstPublicCtor { public function __construct() {} }
class EvalAotInstPrivateCtor { private function __construct() {} }
class EvalAotInstProtectedCtor { protected function __construct() {} }
class EvalAotCloneNoHook {}
class EvalAotClonePublicHook { public function __clone() {} }
class EvalAotClonePrivateHook { private function __clone() {} }
class EvalAotCloneProtectedHook { protected function __clone() {} }

echo eval('
echo (new ReflectionClass("EvalAotInstNoCtor"))->isInstantiable() ? "N" : "n";
echo (new ReflectionClass("EvalAotInstPublicCtor"))->isInstantiable() ? "P" : "p";
echo (new ReflectionClass("EvalAotInstPrivateCtor"))->isInstantiable() ? "R" : "r";
echo (new ReflectionClass("EvalAotInstProtectedCtor"))->isInstantiable() ? "T" : "t";
echo ":";
echo (new ReflectionClass("EvalAotCloneNoHook"))->isCloneable() ? "N" : "n";
echo (new ReflectionClass("EvalAotClonePublicHook"))->isCloneable() ? "P" : "p";
echo (new ReflectionClass("EvalAotClonePrivateHook"))->isCloneable() ? "R" : "r";
echo (new ReflectionClass("EvalAotCloneProtectedHook"))->isCloneable() ? "T" : "t";
');
"#,
    );
    assert_eq!(out, "NPrt:NPrt");
}

/// Verifies eval ReflectionClass reports named eval class-like symbols as non-anonymous through
/// the generated reflection-owner bridge.
#[test]
fn test_eval_reflection_class_anonymous_predicate() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalAnonReflect {}
interface EvalAnonIface {}
trait EvalAnonTrait {}
enum EvalAnonEnum { case Ready; }
echo (new ReflectionClass("EvalAnonReflect"))->isAnonymous() ? "C" : "c";
echo (new ReflectionClass("EvalAnonIface"))->isAnonymous() ? "I" : "i";
echo (new ReflectionClass("EvalAnonTrait"))->isAnonymous() ? "T" : "t";
echo (new ReflectionClass("EvalAnonEnum"))->isAnonymous() ? "E" : "e";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "cite");
}

/// Verifies eval anonymous class expressions instantiate and reflect as anonymous through the bridge.
#[test]
fn test_eval_anonymous_class_expression_runtime_and_reflection() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalRuntimeAnonLabel {
    function label();
}
class EvalRuntimeAnonBase {
    protected string $prefix;
    public function __construct($prefix) { $this->prefix = $prefix; }
}
function eval_runtime_anon_make($prefix) {
    return new class($prefix) extends EvalRuntimeAnonBase implements EvalRuntimeAnonLabel {
        public function label() { return $this->prefix . ":anon"; }
    };
}
$first = eval_runtime_anon_make("A");
$second = eval_runtime_anon_make("B");
echo $first->label(); echo ":";
echo $second->label(); echo ":";
echo get_class($first) === get_class($second) ? "same" : "different"; echo ":";
$ref = new ReflectionClass(get_class($first));
echo $ref->isAnonymous() ? "anonymous" : "named"; echo ":";
echo $ref->implementsInterface("EvalRuntimeAnonLabel") ? "iface" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "A:anon:B:anon:same:anonymous:iface");
}

/// Verifies eval readonly anonymous class expressions initialize and reject writes.
#[test]
fn test_eval_readonly_anonymous_class_expression_runtime() {
    let out = compile_and_run(
        r#"<?php
eval('$box = new readonly class("frozen") {
    public function __construct(public string $label) {}
};
echo $box->label . ":";
try {
    $box->label = "bad";
    echo "bad";
} catch (Error $e) {
    echo get_class($e);
}');
"#,
    );
    assert_eq!(out, "frozen:Error");
}

/// Verifies eval ReflectionClass reports method, property, and constant membership through the bridge.
#[test]
fn test_eval_reflection_class_member_existence() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalMemberParent {
    const PARENT_CONST = 1;
    private function hiddenParent() {}
    protected static function parentStatic() {}
    private $hiddenProp;
    protected static $parentStaticProp;
}
interface EvalMemberClassIface {
    const CLASS_LIMIT = 10;
}
class EvalMemberChild extends EvalMemberParent implements EvalMemberClassIface {
    const CHILD_CONST = 2;
    public function ChildMethod() {}
    public $childProp;
}
interface EvalMemberIfaceParent {
    const PARENT_LIMIT = 10;
    public function parentRequirement();
}
interface EvalMemberIface extends EvalMemberIfaceParent {
    const CHILD_LIMIT = 20;
    public function childRequirement();
    public string $hook { get; }
}
trait EvalMemberTrait {
    const TRAIT_CONST = 30;
    private function traitHidden() {}
    public $traitProp;
}
enum EvalMemberPureEnum {
    case Ready;
    const LEVEL = 40;
    public function label() { return "ok"; }
}
enum EvalMemberBackedEnum: string {
    case Ready = "ready";
}
$child = new ReflectionClass("EvalMemberChild");
echo $child->hasMethod("childmethod") ? "M" : "m";
echo $child->hasMethod("HIDDENPARENT") ? "P" : "p";
echo $child->hasMethod("parentStatic") ? "S" : "s";
echo $child->hasMethod("missing") ? "X" : "x";
echo ":";
echo $child->hasProperty("childProp") ? "C" : "c";
echo $child->hasProperty("hiddenProp") ? "H" : "h";
echo $child->hasProperty("parentStaticProp") ? "T" : "t";
echo $child->hasProperty("childprop") ? "W" : "w";
echo $child->hasConstant("CHILD_CONST") ? "D" : "d";
echo $child->hasConstant("PARENT_CONST") ? "P" : "p";
echo $child->hasConstant("CLASS_LIMIT") ? "A" : "a";
echo $child->hasConstant("child_const") ? "Z" : "z";
echo ":";
$iface = new ReflectionClass("EvalMemberIface");
echo $iface->hasMethod("parentrequirement") ? "I" : "i";
echo $iface->hasMethod("childRequirement") ? "J" : "j";
echo $iface->hasProperty("hook") ? "K" : "k";
echo $iface->hasConstant("PARENT_LIMIT") ? "L" : "l";
echo $iface->hasConstant("CHILD_LIMIT") ? "C" : "c";
echo ":";
$trait = new ReflectionClass("EvalMemberTrait");
echo $trait->hasMethod("traithidden") ? "R" : "r";
echo $trait->hasProperty("traitProp") ? "U" : "u";
echo $trait->hasConstant("TRAIT_CONST") ? "K" : "k";
echo ":";
$pure = new ReflectionClass("EvalMemberPureEnum");
echo $pure->hasMethod("cases") ? "E" : "e";
echo $pure->hasMethod("label") ? "L" : "l";
echo $pure->hasProperty("name") ? "N" : "n";
echo $pure->hasProperty("value") ? "V" : "v";
echo $pure->hasConstant("Ready") ? "G" : "g";
echo $pure->hasConstant("LEVEL") ? "F" : "f";
echo $pure->hasConstant("ready") ? "R" : "r";
echo ":";
$backed = new ReflectionClass("EvalMemberBackedEnum");
echo $backed->hasMethod("tryfrom") ? "B" : "b";
echo $backed->hasProperty("value") ? "Y" : "y";
echo $backed->hasConstant("Ready") ? "Q" : "q";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "MPSx:ChTwDPAz:IJKLC:RUK:ELNvGFr:BYQ");
}

/// Verifies eval ReflectionClass returns constant values and enum cases through the bridge.
#[test]
fn test_eval_reflection_class_constant_values() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectConstBase {
    public const BASE = 1;
}
interface EvalReflectConstIface {
    public const LIMIT = 2;
}
trait EvalReflectConstTrait {
    public const TRAIT_VALUE = 3;
}
class EvalReflectConstChild extends EvalReflectConstBase implements EvalReflectConstIface {
    private const SECRET = 9;
    public const OWN = "own";
    public const SUM = 5;
}
enum EvalReflectConstEnum {
    case Ready;
    public const LEVEL = 40;
}
$ref = new ReflectionClass("EvalReflectConstChild");
$all = $ref->getConstants();
$public = $ref->getConstants(ReflectionClassConstant::IS_PUBLIC);
$private = $ref->getConstants(filter: ReflectionClassConstant::IS_PRIVATE);
$none = $ref->getConstants(0);
$null = $ref->getConstants(null);
echo $ref->getConstant("OWN") . ":";
echo $ref->getConstant("BASE") . ":";
echo $ref->getConstant("LIMIT") . ":";
echo $ref->getConstant("SECRET") . ":";
echo $ref->getConstant("SUM") . ":";
echo $ref->getConstant("own") ? "bad" : "missing";
echo ":" . count($all) . ":" . $all["OWN"] . ":" . $all["BASE"] . ":" . $all["LIMIT"];
echo ":" . count($public) . ":" . $public["OWN"] . ":" . $public["BASE"];
echo ":" . count($private) . ":" . $private["SECRET"];
echo ":" . count($none) . ":" . count($null);
$trait = new ReflectionClass("EvalReflectConstTrait");
$traitAll = $trait->getConstants();
echo ":" . $trait->getConstant("TRAIT_VALUE") . ":" . count($traitAll) . ":" . $traitAll["TRAIT_VALUE"];
$enum = new ReflectionClass("EvalReflectConstEnum");
$case = $enum->getConstant("Ready");
$enumAll = $enum->getConstants();
echo ":" . $case->name;
echo ":" . $enum->getConstant("LEVEL") . ":" . $enumAll["LEVEL"] . ":" . count($enumAll);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "own:1:2:9:5:missing:5:own:1:2:4:own:1:1:9:0:5:3:1:3:Ready:40:40:2"
    );
}

/// Verifies eval ReflectionClass returns class-constant reflector objects through the bridge.
#[test]
fn test_eval_reflection_class_constant_reflector_objects() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectConstMarker {
    public $label;
    public function __construct($label) {
        $this->label = $label;
    }
    public function label() {
        return $this->label;
    }
}
class EvalReflectConstObjectTarget {
    #[EvalReflectConstMarker("const")]
    final public const ANSWER = 42;
}
enum EvalReflectConstObjectEnum {
    #[EvalReflectConstMarker("case")]
    case Ready;
    final public const LEVEL = 7;
}
$ref = new ReflectionClass("EvalReflectConstObjectTarget");
$single = $ref->getReflectionConstant("ANSWER");
$all = $ref->getReflectionConstants();
$public = $ref->getReflectionConstants(ReflectionClassConstant::IS_PUBLIC);
$final = $ref->getReflectionConstants(filter: ReflectionClassConstant::IS_FINAL);
echo $single->getName() . ":";
echo ($single->isFinal() ? "F" : "f") . ":";
echo count($all) . ":" . $all[0]->getName() . ":";
echo $single->getAttributes()[0]->newInstance()->label() . ":";
echo $ref->getReflectionConstant("answer") ? "bad" : "missing";
echo ":" . count($public) . ":" . $public[0]->getName();
echo ":" . count($final) . ":" . $final[0]->getName();
$enum = new ReflectionClass("EvalReflectConstObjectEnum");
$enumAll = $enum->getReflectionConstants();
$enumFinal = $enum->getReflectionConstants(ReflectionClassConstant::IS_FINAL);
$case = $enum->getReflectionConstant("Ready");
$level = $enum->getReflectionConstant("LEVEL");
echo ":" . count($enumAll) . ":" . $enumAll[0]->getName() . ":" . $enumAll[1]->getName();
echo ":" . $case->getAttributes()[0]->newInstance()->label() . ":";
echo count($level->getAttributes()) . ":";
echo $level->isFinal() ? "F" : "f";
echo ":" . count($enumFinal) . ":" . $enumFinal[0]->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ANSWER:F:1:ANSWER:const:missing:1:ANSWER:1:ANSWER:2:Ready:LEVEL:case:0:F:1:LEVEL"
    );
}

/// Verifies eval ReflectionMethod and ReflectionProperty expose member predicates through the bridge.
#[test]
fn test_eval_reflection_member_predicates() {
    let out = compile_and_run_capture(
        r#"<?php
eval('abstract class EvalReflectMemberBase {
    protected static function baseStatic() {}
    abstract protected function mustImplement();
    final public function locked() {}
}
readonly class EvalReflectReadonlyClass {
    public int $classReadonly;
}
abstract class EvalReflectAbstractProperty {
    abstract public int $mustRead { get; }
}
class EvalReflectMemberChild extends EvalReflectMemberBase {
    public function mustImplement() {}
    private static $token;
    final public static $staticSeal;
    protected $visible;
    public readonly int $locked;
    final public int $sealed;
}
$baseStatic = new ReflectionMethod("EvalReflectMemberChild", "baseStatic");
echo $baseStatic->isStatic() ? "S" : "s";
echo $baseStatic->isProtected() ? "P" : "p";
echo $baseStatic->isPublic() ? "U" : "u";
echo $baseStatic->isPrivate() ? "R" : "r";
echo $baseStatic->isFinal() ? "F" : "f";
echo $baseStatic->isAbstract() ? "A" : "a";
echo ":";
$abstractMethod = new ReflectionMethod("EvalReflectMemberBase", "mustImplement");
echo $abstractMethod->isAbstract() ? "A" : "a";
echo $abstractMethod->isProtected() ? "P" : "p";
echo $abstractMethod->isStatic() ? "S" : "s";
echo ":";
$finalMethod = new ReflectionMethod("EvalReflectMemberChild", "locked");
echo $finalMethod->isFinal() ? "F" : "f";
echo $finalMethod->isPublic() ? "U" : "u";
echo $finalMethod->isStatic() ? "S" : "s";
echo ":";
$staticProp = new ReflectionProperty("EvalReflectMemberChild", "token");
echo $staticProp->isStatic() ? "S" : "s";
echo $staticProp->isPrivate() ? "R" : "r";
echo $staticProp->isProtected() ? "P" : "p";
echo $staticProp->isFinal() ? "F" : "f";
echo $staticProp->isAbstract() ? "A" : "a";
echo $staticProp->isReadOnly() ? "R" : "r";
echo $staticProp->isProtectedSet() ? "T" : "t";
echo $staticProp->isPrivateSet() ? "D" : "d";
echo $staticProp->getModifiers();
echo ":";
$visibleProp = new ReflectionProperty("EvalReflectMemberChild", "visible");
echo $visibleProp->isStatic() ? "S" : "s";
echo $visibleProp->isProtected() ? "P" : "p";
echo $visibleProp->isPublic() ? "U" : "u";
echo $visibleProp->isFinal() ? "F" : "f";
echo $visibleProp->isAbstract() ? "A" : "a";
echo $visibleProp->isReadOnly() ? "R" : "r";
echo $visibleProp->isProtectedSet() ? "T" : "t";
echo $visibleProp->isPrivateSet() ? "D" : "d";
echo $visibleProp->getModifiers();
echo ":";
$readonlyProp = new ReflectionProperty("EvalReflectMemberChild", "locked");
echo $readonlyProp->isReadOnly() ? "R" : "r";
echo $readonlyProp->isPublic() ? "U" : "u";
echo $readonlyProp->isProtectedSet() ? "T" : "t";
echo $readonlyProp->isPrivateSet() ? "D" : "d";
echo $readonlyProp->getModifiers();
echo ":";
$sealedProp = new ReflectionProperty("EvalReflectMemberChild", "sealed");
echo $sealedProp->isFinal() ? "F" : "f";
echo $sealedProp->isPublic() ? "U" : "u";
echo $sealedProp->getModifiers();
echo ":";
$staticFinalProp = new ReflectionProperty("EvalReflectMemberChild", "staticSeal");
echo $staticFinalProp->isFinal() ? "F" : "f";
echo $staticFinalProp->isStatic() ? "S" : "s";
echo $staticFinalProp->getModifiers();
echo ":";
$abstractProp = new ReflectionProperty("EvalReflectAbstractProperty", "mustRead");
echo $abstractProp->isAbstract() ? "A" : "a";
echo $abstractProp->isFinal() ? "F" : "f";
echo $abstractProp->getModifiers();
echo ":";
$classReadonlyProp = new ReflectionProperty("EvalReflectReadonlyClass", "classReadonly");
echo $classReadonlyProp->isReadOnly() ? "C" : "c";
echo $classReadonlyProp->isProtectedSet() ? "T" : "t";
echo $classReadonlyProp->isPrivateSet() ? "D" : "d";
echo $classReadonlyProp->getModifiers();
echo ":";
echo $visibleProp->isDynamic() ? "D" : "d";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "SPurfa:APs:FUs:SRpfartd20:sPufartd2:RUTd2177:FU33:FS49:Af577:CTd2177:d"
    );
}

/// Verifies eval ReflectionProperty reports generated asymmetric set-visibility predicates.
#[test]
fn test_eval_reflection_property_set_visibility_predicates_for_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalAotReflectSetVisibility {
    public private(set) int $privateSet = 1;
    public protected(set) int $protectedSet = 2;
}
eval('$private = new ReflectionProperty("EvalAotReflectSetVisibility", "privateSet");
echo $private->isPrivateSet() ? "P" : "p";
echo $private->isProtectedSet() ? "T" : "t";
echo $private->getModifiers(); echo ":";
$protected = new ReflectionProperty("EvalAotReflectSetVisibility", "protectedSet");
echo $protected->isPrivateSet() ? "P" : "p";
echo $protected->isProtectedSet() ? "T" : "t";
echo $protected->getModifiers();');
"#,
    );
    assert_eq!(out, "Pt4129:pT2049");
}

/// Verifies eval-declared asymmetric property visibility enforces writes and reflects metadata.
#[test]
fn test_eval_declared_asymmetric_property_visibility() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalDeclaredAsymBase {
    public private(set) int $privateValue = 1;
    public protected(set) string $protectedName = "base";
    public function ownerWrite($value, $name) {
        $this->privateValue = $value;
        $this->protectedName = $name;
    }
}
class EvalDeclaredAsymChild extends EvalDeclaredAsymBase {
    public function childWrite($name) {
        $this->protectedName = $name;
    }
}
$box = new EvalDeclaredAsymChild();
echo $box->privateValue . ":" . $box->protectedName . ":";
$box->ownerWrite(7, "owner");
echo $box->privateValue . ":" . $box->protectedName . ":";
$box->childWrite("child");
echo $box->protectedName . ":";
$private = new ReflectionProperty("EvalDeclaredAsymBase", "privateValue");
echo ($private->isPrivateSet() ? "P" : "p") . ($private->isProtectedSet() ? "T" : "t");
echo $private->getModifiers() . ":";
$protected = new ReflectionProperty("EvalDeclaredAsymBase", "protectedName");
echo ($protected->isPrivateSet() ? "P" : "p") . ($protected->isProtectedSet() ? "T" : "t");
echo $protected->getModifiers();');
"#,
    );
    assert_eq!(out, "1:base:7:owner:child:Pt4129:pT2049");

    let errors = compile_and_run_capture(
        r#"<?php
eval('class EvalDeclaredAsymErrorBox {
    public private(set) int $privateValue = 1;
    public protected(set) int $protectedValue = 2;
}
$box = new EvalDeclaredAsymErrorBox();
try {
    $box->privateValue = 7;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    unset($box->protectedValue);
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        errors.success,
        "program failed: stdout={:?} stderr={}",
        errors.stdout, errors.stderr
    );
    assert_eq!(
        errors.stdout,
        "Error:Cannot modify private(set) property EvalDeclaredAsymErrorBox::$privateValue from global scope|\
Error:Cannot unset protected(set) property EvalDeclaredAsymErrorBox::$protectedValue from global scope"
    );
}

/// Verifies eval-declared inherited properties preserve PHP redeclaration invariants.
#[test]
fn test_eval_declared_inherited_property_redeclaration_contracts() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalPropertyRedeclareBase {
    protected int|string $value;
}
class EvalPropertyRedeclareChild extends EvalPropertyRedeclareBase {
    public string|int $value;
}
class EvalPropertyRelativeBase {
    public self $selfValue;
    public EvalPropertyRelativeBase $parentValue;
}
class EvalPropertyRelativeChild extends EvalPropertyRelativeBase {
    public self $selfValue;
    public parent $parentValue;
}
class EvalPropertyReadonlyAddBase {
    public int $count = 0;
}
class EvalPropertyReadonlyAddChild extends EvalPropertyReadonlyAddBase {
    public readonly int $count;
    public function __construct() { $this->count = 7; }
}
class EvalPropertyReadonlyWidenBase {
    protected int $count = 0;
    public function count() { return $this->count; }
}
class EvalPropertyReadonlyWidenChild extends EvalPropertyReadonlyWidenBase {
    public readonly int $count;
    public function __construct() { $this->count = 9; }
}
$box = new EvalPropertyRedeclareChild();
$box->value = "ok";
$readonly = new EvalPropertyReadonlyAddChild();
$widened = new EvalPropertyReadonlyWidenChild();
echo $box->value . ":" . $readonly->count . ":" . $widened->count . ":" . $widened->count();');
"#,
    );
    assert_eq!(out, "ok:7:9:9");

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalPropertyTypeBase {
    public int $value;
}
class EvalPropertyStringChild extends EvalPropertyTypeBase {
    public string $value;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalPropertyPublicBase {
    public int $value;
}
class EvalPropertyProtectedChild extends EvalPropertyPublicBase {
    protected int $value;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalPropertyUntypedBase {
    public $value;
}
class EvalPropertyTypedChild extends EvalPropertyUntypedBase {
    public int $value;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalPropertyStaticBase {
    public static int $value;
}
class EvalPropertyInstanceChild extends EvalPropertyStaticBase {
    public int $value;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );

    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalPropertyReadonlyBase {
    public readonly int $value;
}
class EvalPropertyMutableChild extends EvalPropertyReadonlyBase {
    public int $value;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared interface asymmetric property contracts are enforced and reflected.
#[test]
fn test_eval_declared_interface_asymmetric_property_contract() {
    let out = compile_and_run(
        r#"<?php
eval('interface EvalIfaceAsymContract {
    public protected(set) string $name { get; set; }
}
class EvalIfaceAsymBase implements EvalIfaceAsymContract {
    public protected(set) string $name = "base";
}
class EvalIfaceAsymChild extends EvalIfaceAsymBase {
    public function rename($name) { $this->name = $name; }
}
$box = new EvalIfaceAsymChild();
echo $box->name . ":";
$box->rename("child");
echo $box->name . ":";
$ref = new ReflectionProperty("EvalIfaceAsymContract", "name");
echo ($ref->isProtectedSet() ? "T" : "t") . ($ref->isPrivateSet() ? "P" : "p");
echo $ref->getModifiers();');
"#,
    );
    assert_eq!(out, "base:child:Tp2625");
}

/// Verifies eval can observe AOT constructor-promotion metadata through
/// `ReflectionProperty::isPromoted()`.
#[test]
fn test_eval_reflection_property_is_promoted_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotPromotedBase {
    public function __construct(public int $id, protected string $name = "Ada") {}
}
class EvalAotPromotedChild extends EvalAotPromotedBase {}
class EvalAotPromotedPlain {
    public int $id = 0;
    public static int $count = 0;
}
eval('$id = new ReflectionProperty("EvalAotPromotedBase", "id");
echo $id->isPromoted() ? "I" : "i";
$root = new ReflectionProperty("\EvalAotPromotedBase", "id");
echo $root->isPromoted() ? "I" : "i";
$name = new ReflectionProperty("EvalAotPromotedBase", "name");
echo $name->isPromoted() ? "N" : "n";
$child = new ReflectionProperty("EvalAotPromotedChild", "id");
echo $child->isPromoted() ? "C" : "c";
$plain = new ReflectionProperty("EvalAotPromotedPlain", "id");
echo $plain->isPromoted() ? "P" : "p";
$static = new ReflectionProperty("EvalAotPromotedPlain", "count");
echo $static->isPromoted() ? "S" : "s";
$class = new ReflectionClass("EvalAotPromotedBase");
echo $class->hasProperty("id") ? "H" : "h";
echo $class->hasProperty("missing") ? "M" : "m";
$listed = $class->getProperty("id");
echo $listed->isPromoted() ? "G" : "g";
echo $listed->getDeclaringClass()->getName();
$rootClass = new ReflectionClass("\EvalAotPromotedBase");
echo $rootClass->hasProperty("name") ? "N" : "n";
$properties = $class->getProperties();
echo ":" . count($properties);
$listedId = false;
$listedName = false;
foreach ($properties as $property) {
    if ($property->getName() === "id") {
        $listedId = $property->isPromoted();
    }
    if ($property->getName() === "name") {
        $listedName = $property->isPromoted();
    }
}
echo $listedId ? "I" : "i";
echo $listedName ? "N" : "n";
$publicProperties = $class->getProperties(ReflectionProperty::IS_PUBLIC);
$protectedProperties = $class->getProperties(filter: ReflectionProperty::IS_PROTECTED);
echo ":" . count($publicProperties) . $publicProperties[0]->getName();
echo ":" . count($protectedProperties) . $protectedProperties[0]->getName();
echo ":" . count($class->getProperties(0));
try {
    $class->getProperty("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "IINCpsHmGEvalAotPromotedBaseN:2IN:1id:1name:0:Property EvalAotPromotedBase::$missing does not exist"
    );
}

/// Verifies eval reports declaring classes for inherited generated/AOT properties.
#[test]
fn test_eval_reflection_property_declaring_class_for_inherited_aot_members() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectPropertyDeclaringBase {
    public int $base = 1;
    protected static string $baseStatic = "s";
}
class EvalAotReflectPropertyDeclaringChild extends EvalAotReflectPropertyDeclaringBase {
    public int $own = 2;
}
echo eval('$class = new ReflectionClass("EvalAotReflectPropertyDeclaringChild");
$base = $class->getProperty("base");
echo $base->getDeclaringClass()->getName() . ":";
$static = $class->getProperty("baseStatic");
echo $static->getDeclaringClass()->getName() . ":";
$own = $class->getProperty("own");
echo $own->getDeclaringClass()->getName() . ":";
$listed = null;
foreach ($class->getProperties() as $property) {
    if ($property->getName() === "base") {
        $listed = $property;
    }
}
echo $listed->getDeclaringClass()->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotReflectPropertyDeclaringBase:EvalAotReflectPropertyDeclaringBase:EvalAotReflectPropertyDeclaringChild:EvalAotReflectPropertyDeclaringBase"
    );
}

/// Verifies eval exposes declared generated/AOT property types through
/// `ReflectionProperty::hasType()` and `getType()`.
#[test]
fn test_eval_reflection_property_exposes_aot_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectPropertyTypeDep {}
class EvalAotReflectPropertyTypeBase {
    protected ?string $baseName = null;
}
class EvalAotReflectPropertyTypeTarget extends EvalAotReflectPropertyTypeBase {
    public int|string $id = 0;
    public ?EvalAotReflectPropertyTypeDep $dep = null;
    public static ?int $count = null;
    public $untyped = 1;
}
echo eval('$id = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "id");
echo $id->hasType() ? "H:" : "h:";
$type = $id->getType();
$parts = $type->getTypes();
echo $parts[0]->getName() . ($parts[0]->isBuiltin() ? "B" : "C");
echo "," . $parts[1]->getName() . ($parts[1]->isBuiltin() ? "B" : "C");
echo $type->allowsNull() ? ":N" : ":n";
$dep = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "dep");
$depType = $dep->getType();
echo ":" . ($dep->hasType() ? "D" : "d");
echo $depType->allowsNull() ? "?" : "!";
echo $depType->getName() . ($depType->isBuiltin() ? "B" : "C");
$static = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "count");
$staticType = $static->getType();
echo ":" . ($static->hasType() ? "S" : "s");
echo $staticType->allowsNull() ? "?" : "!";
echo $staticType->getName() . ($staticType->isBuiltin() ? "B" : "C");
$base = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "baseName");
$baseType = $base->getType();
echo ":" . ($base->hasType() ? "B" : "b");
echo $baseType->allowsNull() ? "?" : "!";
echo $baseType->getName() . ($baseType->isBuiltin() ? "B" : "C");
$untyped = new ReflectionProperty("EvalAotReflectPropertyTypeTarget", "untyped");
echo ":" . ($untyped->hasType() ? "U" : "u");
echo $untyped->getType() === null ? "N" : "n";
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "H:intB,stringB:n:D?EvalAotReflectPropertyTypeDepC:S?intB:B?stringB:uN"
    );
}

/// Verifies eval exposes supported generated/AOT property defaults through
/// `ReflectionProperty::hasDefaultValue()` and `getDefaultValue()`.
#[test]
fn test_eval_reflection_property_exposes_aot_default_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectPropertyDefaultBase {
    public $implicit;
    protected int $base = 3;
}
class EvalAotReflectPropertyDefaultTarget extends EvalAotReflectPropertyDefaultBase {
    public int $count = 7;
    public static string $label = "ok";
    public ?string $nullable = null;
    public bool $flag = true;
    public float $neg = -1.5;
    public int $typed;
}
echo eval('foreach (["count", "label", "nullable", "implicit", "typed", "base", "flag", "neg"] as $name) {
    $property = new ReflectionProperty("EvalAotReflectPropertyDefaultTarget", $name);
    echo $name . ":";
    echo $property->hasDefaultValue() ? "D:" : "d:";
    $value = $property->getDefaultValue();
    echo $value === null ? "null" : $value;
    echo "|";
}
$listed = null;
foreach ((new ReflectionClass("EvalAotReflectPropertyDefaultTarget"))->getProperties() as $property) {
    if ($property->getName() === "count") {
        $listed = $property;
    }
}
echo "listed:";
echo $listed->hasDefaultValue() ? "D:" : "d:";
echo $listed->getDefaultValue();
$defaults = (new ReflectionClass("EvalAotReflectPropertyDefaultTarget"))->getDefaultProperties();
echo "|defaults:";
echo $defaults["label"] . ":";
echo $defaults["count"] . ":";
echo $defaults["base"] . ":";
echo array_key_exists("implicit", $defaults) && $defaults["implicit"] === null ? "I:" : "i:";
echo array_key_exists("nullable", $defaults) && $defaults["nullable"] === null ? "N:" : "n:";
echo $defaults["flag"] ? "F:" : "f:";
echo $defaults["neg"] . ":";
echo array_key_exists("typed", $defaults) ? "T" : "t";
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "count:D:7|label:D:ok|nullable:D:null|implicit:D:null|typed:d:null|base:D:3|flag:D:1|neg:D:-1.5|listed:D:7|defaults:ok:7:3:I:N:F:-1.5:t"
    );
}

/// Verifies eval ReflectionProperty exposes generated/AOT non-empty array defaults.
#[test]
fn test_eval_reflection_property_exposes_aot_array_default_values() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectArrayPropertyDefaultTarget {
    public $items = ["left" => "L", 2 => "R", "tail"];
}
echo eval('$property = new ReflectionProperty("EvalAotReflectArrayPropertyDefaultTarget", "items");
$items = $property->getDefaultValue();
$defaults = (new ReflectionClass("EvalAotReflectArrayPropertyDefaultTarget"))->getDefaultProperties();
return $items["left"] . ":" . $items[2] . ":" . $items[3] . ":" . $defaults["items"]["left"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "L:R:tail:L");
}

/// Verifies eval exposes generated/AOT member attributes through Reflection.
#[test]
fn test_eval_reflection_member_exposes_aot_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotMemberAttr {
    public function __construct($first = null, $second = null, $third = null, $fourth = null) {}
}
class EvalAotReflectAttrBase {
    #[EvalAotMemberAttr("base", 7, true, null)]
    public function baseRun() {}
    #[EvalAotMemberAttr("baseProp")]
    protected int $baseId = 1;
}
class EvalAotReflectAttrTarget extends EvalAotReflectAttrBase {
    #[EvalAotMemberAttr("method")]
    public function run() {}
    #[EvalAotMemberAttr("property", -3)]
    public int $id = 2;
    #[EvalAotMemberAttr("constant", 11)]
    public const LIMIT = 5;
}
enum EvalAotReflectAttrEnum {
    #[EvalAotMemberAttr("case")]
    case Ready;
}
echo eval('$methodAttrs = (new ReflectionMethod("EvalAotReflectAttrTarget", "run"))->getAttributes();
echo "M" . count($methodAttrs) . ":";
echo $methodAttrs[0]->getName() . ":" . $methodAttrs[0]->getArguments()[0] . ":";
$propertyAttrs = (new ReflectionProperty("EvalAotReflectAttrTarget", "id"))->getAttributes();
echo "P" . count($propertyAttrs) . ":";
echo $propertyAttrs[0]->getName() . ":";
$propertyArgs = $propertyAttrs[0]->getArguments();
echo $propertyArgs[0] . ":" . $propertyArgs[1] . ":";
$baseMethodAttrs = (new ReflectionMethod("EvalAotReflectAttrTarget", "baseRun"))->getAttributes();
echo "BM" . count($baseMethodAttrs) . ":";
$args = $baseMethodAttrs[0]->getArguments();
echo $args[0] . ":" . $args[1] . ":" . ($args[2] ? "T" : "F") . ":" . ($args[3] === null ? "N" : "n") . ":";
$basePropertyAttrs = (new ReflectionProperty("EvalAotReflectAttrTarget", "baseId"))->getAttributes();
echo "BP" . count($basePropertyAttrs) . ":" . $basePropertyAttrs[0]->getArguments()[0] . ":";
$listedMethod = (new ReflectionClass("EvalAotReflectAttrTarget"))->getMethod("run");
echo count($listedMethod->getAttributes()) . ":";
$listedProperty = (new ReflectionClass("EvalAotReflectAttrTarget"))->getProperty("id");
echo count($listedProperty->getAttributes()) . ":";
$constantAttrs = (new ReflectionClassConstant("EvalAotReflectAttrTarget", "LIMIT"))->getAttributes();
echo "C" . count($constantAttrs) . ":";
echo $constantAttrs[0]->getName() . ":";
$constantArgs = $constantAttrs[0]->getArguments();
echo $constantArgs[0] . ":" . $constantArgs[1] . ":";
$listedConstant = (new ReflectionClass("EvalAotReflectAttrTarget"))->getReflectionConstant("LIMIT");
echo count($listedConstant->getAttributes()) . ":";
$caseAttrs = (new ReflectionClassConstant("EvalAotReflectAttrEnum", "Ready"))->getAttributes();
echo "E" . count($caseAttrs) . ":" . $caseAttrs[0]->getArguments()[0];
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "M1:EvalAotMemberAttr:method:P1:EvalAotMemberAttr:property:-3:BM1:base:7:T:N:BP1:baseProp:1:1:C1:EvalAotMemberAttr:constant:11:1:E1:case"
    );
}

/// Verifies eval class-attribute helpers expose generated/AOT class attributes.
#[test]
fn test_eval_reflection_class_exposes_aot_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotClassAttr {
    public string $name = "";
    public int $value = 0;
    public string $summary = "";

    public function __construct(string $name, int $value = 0, $items = []) {
        $this->name = $name;
        $this->value = $value;
        $this->summary = "ok";
    }

    public function label(): string {
        return $this->name . ":" . $this->value;
    }
}
#[EvalAotClassAttr("class", 1, ["nested", "kind" => "class", "meta" => ["inner" => "value"]]), EvalAotClassAttr("again", 2, ["later", "kind" => "again"])]
class EvalAotClassAttrTarget {}
echo eval('$names = class_attribute_names("EvalAotClassAttrTarget");
echo count($names) . ":" . $names[0] . ":" . $names[1] . ":";
$args = class_attribute_args("evalaotclassattrtarget", "EvalAotClassAttr");
echo count($args) . ":" . $args[0] . ":" . $args[1] . ":";
echo count($args[2]) . ":" . $args[2][0] . ":" . $args[2]["kind"] . ":" . $args[2]["meta"]["inner"] . ":";
$attrs = class_get_attributes("EvalAotClassAttrTarget");
echo count($attrs) . ":" . $attrs[0]->getName() . ":";
echo ($attrs[0]->isRepeated() ? "R" : "r") . ":";
echo $attrs[0]->getArguments()[0] . ":" . $attrs[0]->getArguments()[1] . ":";
echo count($attrs[0]->getArguments()[2]) . ":" . $attrs[0]->getArguments()[2][0] . ":" . $attrs[0]->getArguments()[2]["kind"] . ":";
$firstInstance = $attrs[0]->newInstance();
echo $attrs[0]->getTarget() . ":" . $firstInstance->label() . ":" . $firstInstance->summary . ":";
$refAttrs = (new ReflectionClass("EvalAotClassAttrTarget"))->getAttributes();
echo count($refAttrs) . ":" . $refAttrs[1]->getArguments()[0] . ":";
echo count($refAttrs[1]->getArguments()[2]) . ":" . $refAttrs[1]->getArguments()[2][0] . ":" . $refAttrs[1]->getArguments()[2]["kind"] . ":";
echo $refAttrs[1]->newInstance()->label();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "2:EvalAotClassAttr:EvalAotClassAttr:3:class:1:3:nested:class:value:2:EvalAotClassAttr:R:class:1:3:nested:class:1:class:1:ok:2:again:2:later:again:again:2"
    );
}

/// Verifies eval ReflectionAttribute::newInstance constructs generated/AOT attribute classes.
#[test]
fn test_eval_reflection_attribute_new_instance_constructs_aot_attribute() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotAttributeInstance {
    public string $name = "";
    public int $value = 0;

    public function __construct(string $name, int $value = 0) {
        $this->name = $name;
        $this->value = $value;
    }

    public function label(): string {
        return $this->name . ":" . $this->value;
    }
}
class EvalAotAttributeInstanceTarget {
    #[EvalAotAttributeInstance("method", 2)]
    public function run() {}

    #[EvalAotAttributeInstance("property", 3)]
    public int $id = 0;

    #[EvalAotAttributeInstance("constant", 4)]
    public const LIMIT = 5;
}
echo eval('$methodAttr = (new ReflectionMethod("EvalAotAttributeInstanceTarget", "run"))->getAttributes()[0];
echo $methodAttr->newInstance()->label() . ":";
$propertyAttr = (new ReflectionProperty("EvalAotAttributeInstanceTarget", "id"))->getAttributes()[0];
echo $propertyAttr->newInstance()->label() . ":";
$constantAttr = (new ReflectionClassConstant("EvalAotAttributeInstanceTarget", "LIMIT"))->getAttributes()[0];
echo $constantAttr->newInstance()->label();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "method:2:property:3:constant:4");
}

/// Verifies eval can probe generated/AOT method predicate metadata through
/// `ReflectionClass::hasMethod()`, `getMethod()`, and direct `ReflectionMethod`.
#[test]
fn test_eval_reflection_method_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectMethodBase {
    protected static function baseStatic() {}
    final public function locked() {}
}
class EvalAotReflectMethodChild extends EvalAotReflectMethodBase {
    public function run() {}
    private function hidden() {}
}
eval('$class = new ReflectionClass("EvalAotReflectMethodChild");
echo $class->hasMethod("RUN") ? "R" : "r";
echo $class->hasMethod("BASESTATIC") ? "B" : "b";
echo $class->hasMethod("missing") ? "M" : "m";
$run = $class->getMethod("RUN");
echo ":" . $run->getName();
echo $run->isPublic() ? "U" : "u";
echo $run->isStatic() ? "S" : "s";
echo $run->getDeclaringClass()->getName();
$base = $class->getMethod("baseStatic");
echo ":" . ($base->isStatic() ? "S" : "s");
echo $base->isProtected() ? "P" : "p";
$locked = new ReflectionMethod("EvalAotReflectMethodBase", "LOCKED");
echo ":" . $locked->getName();
echo $locked->isFinal() ? "F" : "f";
echo $locked->isPublic() ? "U" : "u";
echo $locked->getDeclaringClass()->getName();
$methods = $class->getMethods();
$seenRun = false;
$seenBase = false;
$seenLocked = false;
foreach ($methods as $method) {
    if (strtolower($method->getName()) === "run") {
        $seenRun = $method->isPublic();
    }
    if (strtolower($method->getName()) === "basestatic") {
        $seenBase = $method->isStatic();
    }
    if (strtolower($method->getName()) === "locked") {
        $seenLocked = $method->isFinal();
    }
}
echo ":" . count($methods);
echo $seenRun ? "R" : "r";
echo $seenBase ? "B" : "b";
echo $seenLocked ? "L" : "l";
$staticMethods = $class->getMethods(ReflectionMethod::IS_STATIC);
$privateMethods = $class->getMethods(filter: ReflectionMethod::IS_PRIVATE);
$seenStatic = false;
$seenHidden = false;
foreach ($staticMethods as $method) {
    if (strtolower($method->getName()) === "basestatic") {
        $seenStatic = $method->isProtected();
    }
}
foreach ($privateMethods as $method) {
    if (strtolower($method->getName()) === "hidden") {
        $seenHidden = $method->isPrivate();
    }
}
echo ":" . count($staticMethods) . ($seenStatic ? "S" : "s");
echo ":" . count($privateMethods) . ($seenHidden ? "H" : "h");
echo ":" . count($class->getMethods(0));
try {
    $class->getMethod("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "RBm:runUsEvalAotReflectMethodChild:SP:lockedFUEvalAotReflectMethodBase:4RBL:1S:1H:0:Method EvalAotReflectMethodChild::missing() does not exist"
    );
}

/// Verifies eval reports declaring classes for inherited generated/AOT methods and constructors.
#[test]
fn test_eval_reflection_method_declaring_class_for_inherited_aot_members() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectDeclaringBase {
    public function __construct(string $name = "base") {}

    public function inherited(): string {
        return "base";
    }

    protected static function baseStatic(): string {
        return "static";
    }
}
class EvalAotReflectDeclaringChild extends EvalAotReflectDeclaringBase {
    public function own(): string {
        return "child";
    }
}
echo eval('$class = new ReflectionClass("EvalAotReflectDeclaringChild");
$inherited = $class->getMethod("inherited");
echo $inherited->getDeclaringClass()->getName() . ":";
$static = $class->getMethod("baseStatic");
echo $static->getDeclaringClass()->getName() . ":";
$own = $class->getMethod("own");
echo $own->getDeclaringClass()->getName() . ":";
$ctor = $class->getConstructor();
echo $ctor->getDeclaringClass()->getName() . "/" . $ctor->getNumberOfParameters() . ":";
$listed = null;
foreach ($class->getMethods() as $method) {
    if ($method->getName() === "inherited") {
        $listed = $method;
    }
}
echo $listed->getDeclaringClass()->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotReflectDeclaringBase:EvalAotReflectDeclaringBase:EvalAotReflectDeclaringChild:EvalAotReflectDeclaringBase/1:EvalAotReflectDeclaringBase"
    );
}

/// Verifies eval ReflectionMethod::invoke can dispatch generated/AOT methods.
#[test]
fn test_eval_reflection_method_invoke_calls_aot_method() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectInvokeBase {
    public function who(): string {
        return static::class;
    }

    public static function make(string $left, string $right = "S"): string {
        return static::class . ":" . $left . $right;
    }
}
class EvalAotReflectInvokeChild extends EvalAotReflectInvokeBase {
    public function join(string $a, string $b = "B"): string {
        return $a . $b;
    }
}
echo eval('$object = new EvalAotReflectInvokeChild();
$who = (new ReflectionClass("EvalAotReflectInvokeChild"))->getMethod("who");
echo $who->invoke($object) . ":";
$static = new ReflectionMethod("EvalAotReflectInvokeBase", "make");
echo $static->invoke(null, right: "Y", left: "X") . ":";
echo $static->invoke($object, "A") . ":";
$join = new ReflectionMethod("EvalAotReflectInvokeChild", "join");
echo $join->invoke($object, "Q") . ":";
return $join->invokeArgs($object, ["b" => "2", "a" => "1"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotReflectInvokeChild:EvalAotReflectInvokeBase:XY:EvalAotReflectInvokeBase:AS:QB:12"
    );
}

/// Verifies eval ReflectionMethod::invokeArgs accepts runtime-built AOT argument arrays.
#[test]
fn test_eval_reflection_method_invoke_args_accepts_runtime_aot_arg_arrays() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectRuntimeInvokeArgsTarget {
    public function join(string $a, string $b = "B"): string {
        return $a . $b;
    }

    public static function make(string $left, string $right = "S"): string {
        return $left . $right;
    }
}
echo eval('$object = new EvalAotReflectRuntimeInvokeArgsTarget();
$join = new ReflectionMethod("EvalAotReflectRuntimeInvokeArgsTarget", "join");
$args = [];
$args["b"] = "2";
$args["a"] = "1";
echo $join->invokeArgs($object, $args) . ":";
$static = new ReflectionMethod("EvalAotReflectRuntimeInvokeArgsTarget", "make");
$staticArgs = [];
$staticArgs["right"] = "Y";
$staticArgs["left"] = "X";
return $static->invokeArgs(null, $staticArgs);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "12:XY");
}

/// Verifies eval ReflectionMethod::invoke bypasses generated/AOT method visibility.
#[test]
fn test_eval_reflection_method_invoke_bypasses_aot_method_visibility() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectVisibilityBox {
    private function secret(int $n): int {
        return $n + 2;
    }

    protected static function label(string $value): string {
        return "H" . $value;
    }
}

echo eval('$object = new EvalAotReflectVisibilityBox();
$secret = new ReflectionMethod(EvalAotReflectVisibilityBox::class, "secret");
$label = new ReflectionMethod(EvalAotReflectVisibilityBox::class, "label");
return $secret->invoke($object, 3) . ":" . $label->invoke(null, "x");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "5:Hx");
}

/// Verifies eval ReflectionMethod exposes registered generated/AOT parameter metadata.
#[test]
fn test_eval_reflection_method_exposes_aot_parameter_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectParamTarget {
    public function join(string $left, string $right = "B", ?int $count = null): string {
        return $left . $right . ($count ?? 0);
    }

    public static function sum(int $first, int $second = 2): int {
        return $first + $second;
    }
}
echo eval('$method = new ReflectionMethod("EvalAotReflectParamTarget", "join");
echo $method->getNumberOfParameters() . "/" . $method->getNumberOfRequiredParameters() . ":";
foreach ($method->getParameters() as $param) {
    echo $param->getName();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isDefaultValueAvailable() ? "=" : "-";
    if ($param->isDefaultValueAvailable()) {
        $default = $param->getDefaultValue();
        echo is_null($default) ? "null" : $default;
    }
    echo ";";
}
$static = new ReflectionMethod("EvalAotReflectParamTarget", "sum");
echo ":" . $static->getNumberOfParameters() . "/" . $static->getNumberOfRequiredParameters() . ":";
foreach ($static->getParameters() as $param) {
    echo $param->getName();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isDefaultValueAvailable() ? "=" : "-";
    if ($param->isDefaultValueAvailable()) {
        echo $param->getDefaultValue();
    }
    echo ";";
}
$listed = null;
foreach ((new ReflectionClass("EvalAotReflectParamTarget"))->getMethods() as $candidate) {
    if ($candidate->getName() === "join") {
        $listed = $candidate;
    }
}
echo ":" . $listed->getNumberOfParameters() . "/" . $listed->getParameters()[2]->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "3/1:leftr-;rightO=B;countO=null;:2/1:firstr-;secondO=2;:3/count"
    );
}

/// Verifies direct ReflectionParameter construction accepts generated/AOT method targets.
#[test]
fn test_eval_reflection_parameter_accepts_aot_method_targets() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotDirectParamTarget {
    public function join(string $left, string $right = "B"): string {
        return $left . $right;
    }

    public static function collect(int $count, string ...$items): string {
        return implode(":", $items);
    }
}
echo eval('$direct = new ReflectionParameter(["EvalAotDirectParamTarget", "join"], "right");
echo $direct->getName() . ":" . $direct->getPosition() . ":";
echo $direct->getDeclaringClass()->getName() . ":";
echo $direct->getDeclaringFunction()->getName() . ":";
echo ($direct->isOptional() ? "O" : "r") . ":" . $direct->getDefaultValue() . ":";
$objectParam = new ReflectionParameter([new EvalAotDirectParamTarget(), "join"], 0);
echo $objectParam->getName() . ":" . $objectParam->getType()->getName() . ":";
$static = new ReflectionParameter(["EvalAotDirectParamTarget", "collect"], "items");
echo $static->getName() . ":" . ($static->isVariadic() ? "V" : "v") . ":" . ($static->isOptional() ? "O" : "r");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "right:1:EvalAotDirectParamTarget:join:O:B:left:string:items:V:O"
    );
}

/// Verifies eval ReflectionMethod exposes generated/AOT empty-array defaults.
#[test]
fn test_eval_reflection_method_exposes_aot_empty_array_default() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectArrayDefaultTarget {
    public function countItems($items = []): int {
        return 0;
    }
}
echo eval('$method = new ReflectionMethod("EvalAotReflectArrayDefaultTarget", "countItems");
$param = $method->getParameters()[0];
echo $param->isOptional() ? "O" : "r";
echo $param->isDefaultValueAvailable() ? "=" : "-";
$default = $param->getDefaultValue();
echo is_array($default) ? count($default) : "bad";
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "O=0");
}

/// Verifies eval materializes generated/AOT empty-array defaults during method dispatch.
#[test]
fn test_eval_aot_method_call_uses_empty_array_default() {
    let out = compile_and_run(
        r#"<?php
class EvalAotArrayDefaultMethodTarget {
    public function countItems($items = []): int {
        return is_array($items) ? 0 : 9;
    }
}
echo eval('$obj = new EvalAotArrayDefaultMethodTarget();
return $obj->countItems();');
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies eval materializes generated/AOT non-empty array defaults during method dispatch.
#[test]
fn test_eval_aot_method_call_uses_array_default_values() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotArrayValueDefaultMethodTarget {
    public function describe(array $items = [4, 5, 6]): string {
        return $items[0] . ":" . $items[1] . ":" . $items[2];
    }

    public static function describeStatic(array $items = [7, 8, 9]): string {
        return $items[0] . ":" . $items[1] . ":" . $items[2];
    }
}
echo eval('$obj = new EvalAotArrayValueDefaultMethodTarget();
$method = new ReflectionMethod("EvalAotArrayValueDefaultMethodTarget", "describe");
$default = $method->getParameters()[0]->getDefaultValue();
return $obj->describe() . ":" . EvalAotArrayValueDefaultMethodTarget::describeStatic() . ":" . $default[0] . $default[1] . $default[2];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "4:5:6:7:8:9:456");
}

/// Verifies eval normalizes generated/AOT array-default keys with PHP rules.
#[test]
fn test_eval_aot_method_array_default_normalizes_keys() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotArrayKeyDefaultMethodTarget {
    public function describe(array $items = [
        true => "yes",
        false => "no",
        2.8 => "float",
        "3" => "numeric",
    ]): string {
        return $items[1] . ":" . $items[0] . ":" . $items[2] . ":" . $items[3];
    }

    public function reflected(array $items = [
        null => "nil",
        "03" => "string",
        -1.2 => "negative",
    ]): void {
    }
}

echo eval('$obj = new EvalAotArrayKeyDefaultMethodTarget();
$method = new ReflectionMethod("EvalAotArrayKeyDefaultMethodTarget", "describe");
$default = $method->getParameters()[0]->getDefaultValue();
$reflected = (new ReflectionMethod("EvalAotArrayKeyDefaultMethodTarget", "reflected"))->getParameters()[0]->getDefaultValue();
return $obj->describe() . "|" . $default[1] . ":" . $default[0] . ":" . $default[2] . ":" . $default[3] . "|" . $reflected[""] . ":" . $reflected["03"] . ":" . $reflected[-1];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "yes:no:float:numeric|yes:no:float:numeric|nil:string:negative"
    );
}

/// Verifies eval materializes generated/AOT object defaults during method dispatch.
#[test]
fn test_eval_aot_method_call_uses_object_default() {
    let out = compile_and_run(
        r#"<?php
class EvalAotObjectDefaultMethodDep {
    public string $label;
    public function __construct(string $left = "d", string $right = "e", string $third = "p", string $fourth = "") {
        $this->label = $left . $right . $third . $fourth;
    }
}

class EvalAotObjectDefaultMethodTarget {
    public function describe(EvalAotObjectDefaultMethodDep $dep = new EvalAotObjectDefaultMethodDep("m", "e", "t", "h")): string {
        return $dep->label;
    }

    public static function describeStatic(EvalAotObjectDefaultMethodDep $dep = new EvalAotObjectDefaultMethodDep("s", "t", "a", "t")): string {
        return $dep->label;
    }
}

echo eval('$obj = new EvalAotObjectDefaultMethodTarget();
$method = new ReflectionMethod("EvalAotObjectDefaultMethodTarget", "describe");
$default = $method->getParameters()[0]->getDefaultValue();
return $obj->describe() . ":" . EvalAotObjectDefaultMethodTarget::describeStatic() . ":" . $default->label;');
"#,
    );
    assert_eq!(out, "meth:stat:meth");
}

/// Verifies eval preserves named constructor args in generated/AOT object defaults.
#[test]
fn test_eval_aot_object_default_uses_named_constructor_args() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNamedObjectDefaultDep {
    public string $label;

    public function __construct(string $left = "l", string $right = "r") {
        $this->label = $left . $right;
    }
}

class EvalAotNamedObjectDefaultMethodTarget {
    public function describe(EvalAotNamedObjectDefaultDep $dep = new EvalAotNamedObjectDefaultDep(right: "R", left: "L")): string {
        return $dep->label;
    }
}

class EvalAotNamedObjectDefaultCtorTarget {
    public string $label = "";

    public function __construct(EvalAotNamedObjectDefaultDep $dep = new EvalAotNamedObjectDefaultDep(right: "B", left: "A")) {
        $this->label = $dep->label;
    }
}

echo eval('$obj = new EvalAotNamedObjectDefaultMethodTarget();
$method = new ReflectionMethod("EvalAotNamedObjectDefaultMethodTarget", "describe");
$default = $method->getParameters()[0]->getDefaultValue();
$box = new EvalAotNamedObjectDefaultCtorTarget();
return $obj->describe() . ":" . $default->label . ":" . $box->label;');
"#,
    );
    assert_eq!(out, "LR:LR:AB");
}

/// Verifies eval materializes nested generated/AOT object defaults during method dispatch.
#[test]
fn test_eval_aot_method_call_uses_nested_object_default() {
    let out = compile_and_run(
        r#"<?php
class EvalAotNestedObjectDefaultInner {
    public string $label;

    public function __construct(string $label = "inner") {
        $this->label = $label;
    }
}

class EvalAotNestedObjectDefaultOuter {
    public EvalAotNestedObjectDefaultInner $inner;

    public function __construct(EvalAotNestedObjectDefaultInner $inner = new EvalAotNestedObjectDefaultInner("outer")) {
        $this->inner = $inner;
    }
}

class EvalAotNestedObjectDefaultMethodTarget {
    public function describe(EvalAotNestedObjectDefaultOuter $outer = new EvalAotNestedObjectDefaultOuter(new EvalAotNestedObjectDefaultInner("method"))): string {
        return $outer->inner->label;
    }
}

echo eval('$obj = new EvalAotNestedObjectDefaultMethodTarget();
$method = new ReflectionMethod("EvalAotNestedObjectDefaultMethodTarget", "describe");
$default = $method->getParameters()[0]->getDefaultValue();
return $obj->describe() . ":" . $default->inner->label;');
"#,
    );
    assert_eq!(out, "method:method");
}

/// Verifies eval materializes generated/AOT object defaults with more than eight constructor args.
#[test]
fn test_eval_aot_object_default_uses_large_constructor_arg_list() {
    let out = compile_and_run(
        r#"<?php
class EvalAotLargeObjectDefaultDep {
    public string $label;

    public function __construct(string ...$parts) {
        $this->label = implode("", $parts);
    }
}

class EvalAotLargeObjectDefaultMethodTarget {
    public function describe(EvalAotLargeObjectDefaultDep $dep = new EvalAotLargeObjectDefaultDep("A", "B", "C", "D", "E", "F", "G", "H", "I")): string {
        return $dep->label;
    }
}

class EvalAotLargeObjectDefaultCtorTarget {
    public string $label = "";

    public function __construct(EvalAotLargeObjectDefaultDep $dep = new EvalAotLargeObjectDefaultDep("J", "K", "L", "M", "N", "O", "P", "Q", "R")) {
        $this->label = $dep->label;
    }
}

echo eval('$obj = new EvalAotLargeObjectDefaultMethodTarget();
$method = new ReflectionMethod("EvalAotLargeObjectDefaultMethodTarget", "describe");
$default = $method->getParameters()[0]->getDefaultValue();
$box = new EvalAotLargeObjectDefaultCtorTarget();
return $obj->describe() . ":" . $default->label . ":" . $box->label;');
"#,
    );
    assert_eq!(out, "ABCDEFGHI:ABCDEFGHI:JKLMNOPQR");
}

/// Verifies eval ReflectionMethod exposes generated/AOT by-ref and variadic parameter flags.
#[test]
fn test_eval_reflection_method_exposes_aot_parameter_flags() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectParamFlagsTarget {
    public function mutate(int &$value, string ...$parts): string {
        $value += count($parts);
        return implode("", $parts);
    }

    public static function collect(string ...$items): string {
        return implode(":", $items);
    }
}
echo eval('$method = new ReflectionMethod("EvalAotReflectParamFlagsTarget", "mutate");
$params = $method->getParameters();
echo $method->getNumberOfParameters() . "/" . $method->getNumberOfRequiredParameters() . ":";
foreach ($params as $param) {
    echo $param->getName();
    echo $param->isPassedByReference() ? "R" : "b";
    echo $param->isVariadic() ? "V" : "v";
    echo $param->isOptional() ? "O" : "r";
    echo ";";
}
$static = new ReflectionMethod("EvalAotReflectParamFlagsTarget", "collect");
$item = $static->getParameters()[0];
echo ":" . $item->getName();
echo $item->isPassedByReference() ? "R" : "b";
echo $item->isVariadic() ? "V" : "v";
echo $static->getNumberOfRequiredParameters();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2/1:valueRvr;partsbVO;:itemsbV0");
}

/// Verifies eval can dispatch generated/AOT variadic methods and constructors.
#[test]
fn test_eval_aot_variadic_method_and_constructor_bridge() {
    let out = compile_and_run(
        r#"<?php
class EvalAotVariadicBridgeTarget {
    public string $label = "";

    public function __construct($head, ...$items) {
        $this->label = $head . ":" . count($items) . ":" . $items[0] . ":" . $items[1];
    }

    public function collect($head, ...$items): string {
        return $head . ":" . count($items) . ":" . $items[0] . ":" . $items[1];
    }

    public static function collectStatic(...$items): string {
        return count($items) . ":" . $items[0] . ":" . $items[1];
    }
}
echo eval('$target = new EvalAotVariadicBridgeTarget("C", "D", "E");
return $target->collect("H", "A", "B") . "|" . EvalAotVariadicBridgeTarget::collectStatic("S", "T") . "|" . $target->label;');
"#,
    );
    assert_eq!(out, "H:2:A:B|2:S:T|C:2:D:E");
}

/// Verifies eval can dispatch generated/AOT methods and constructors beyond register-only arity.
#[test]
fn test_eval_aot_wide_method_and_constructor_bridge() {
    let out = compile_and_run(
        r#"<?php
class EvalAotWideMethodBridgeTarget {
    public string $label = "";

    public function __construct($a, $b, $c, $d, $e, $f, $g, $h, $i) {
        $this->label = $a . $b . $c . $d . $e . $f . $g . $h . $i;
    }

    public function join($a, $b, $c, $d, $e, $f, $g, $h, $i): string {
        return $a . $b . $c . $d . $e . $f . $g . $h . $i;
    }

    public static function joinStatic($a, $b, $c, $d, $e, $f, $g, $h, $i): string {
        return $a . $b . $c . $d . $e . $f . $g . $h . $i;
    }
}
echo eval('$target = new EvalAotWideMethodBridgeTarget("A", "B", "C", "D", "E", "F", "G", "H", "I");
return $target->join("1", "2", "3", "4", "5", "6", "7", "8", "9")
    . "|" . EvalAotWideMethodBridgeTarget::joinStatic("a", "b", "c", "d", "e", "f", "g", "h", "i")
    . "|" . $target->label;');
"#,
    );
    assert_eq!(out, "123456789|abcdefghi|ABCDEFGHI");
}

/// Verifies generated/AOT variadic bridge rejects named arguments captured by the tail.
#[test]
fn test_eval_aot_variadic_method_rejects_named_tail() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class EvalAotVariadicNamedTailTarget {
    public function collect(...$items): int {
        return count($items);
    }
}
eval('$target = new EvalAotVariadicNamedTailTarget();
$target->collect(named: "A");');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval ReflectionMethod exposes generated/AOT declared type metadata.
#[test]
fn test_eval_reflection_method_exposes_aot_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotReflectTypeLeft {}
interface EvalAotReflectTypeRight {}
class EvalAotReflectTypeBoth implements EvalAotReflectTypeLeft, EvalAotReflectTypeRight {}
class EvalAotReflectTypeDep {}
class EvalAotReflectTypeTarget {
    public function describe(int|string $id, ?EvalAotReflectTypeDep $dep): ?string {
        return null;
    }

    public static function factory(EvalAotReflectTypeDep $dep): EvalAotReflectTypeDep {
        return $dep;
    }

    public function both(EvalAotReflectTypeLeft&EvalAotReflectTypeRight $value): void {}
}
echo eval('$method = new ReflectionMethod("EvalAotReflectTypeTarget", "describe");
$params = $method->getParameters();
$union = $params[0]->getType();
echo "U" . count($union->getTypes());
foreach ($union->getTypes() as $type) {
    echo ":" . $type->getName() . ($type->isBuiltin() ? "B" : "C");
}
$dep = $params[1]->getType();
echo ":D" . ($dep->allowsNull() ? "?" : "!") . ":" . $dep->getName() . ($dep->isBuiltin() ? "B" : "C");
$return = $method->getReturnType();
echo ":R" . ($return->allowsNull() ? "?" : "!") . ":" . $return->getName() . ($return->isBuiltin() ? "B" : "C");
$static = (new ReflectionMethod("EvalAotReflectTypeTarget", "factory"))->getReturnType();
echo ":S" . ($static->allowsNull() ? "?" : "!") . ":" . $static->getName() . ($static->isBuiltin() ? "B" : "C");
$intersection = (new ReflectionMethod("EvalAotReflectTypeTarget", "both"))->getParameters()[0]->getType();
echo ":I" . count($intersection->getTypes());
foreach ($intersection->getTypes() as $type) {
    echo ":" . $type->getName() . ($type->isBuiltin() ? "B" : "C");
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "U2:intB:stringB:D?:EvalAotReflectTypeDepC:R?:stringB:S!:EvalAotReflectTypeDepC:I2:EvalAotReflectTypeLeftC:EvalAotReflectTypeRightC"
    );
}

/// Verifies eval ReflectionClass::getConstructor exposes generated/AOT constructor metadata.
#[test]
fn test_eval_reflection_class_get_constructor_for_aot_class() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectCtorParamTarget {
    public string $label = "";

    public function __construct(string $left, string $right = "B", ?int $count = null) {
        $this->label = $left . $right . ($count ?? 0);
    }
}
class EvalAotReflectCtorPlain {}
echo eval('$ctor = (new ReflectionClass("EvalAotReflectCtorParamTarget"))->getConstructor();
echo ($ctor instanceof ReflectionMethod) ? "M:" : "m:";
echo $ctor->getName() . "/" . $ctor->getDeclaringClass()->getName() . ":";
echo $ctor->getNumberOfParameters() . "/" . $ctor->getNumberOfRequiredParameters() . ":";
foreach ($ctor->getParameters() as $param) {
    echo $param->getName();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isDefaultValueAvailable() ? "=" : "-";
    if ($param->isDefaultValueAvailable()) {
        $default = $param->getDefaultValue();
        echo is_null($default) ? "null" : $default;
    }
    $type = $param->getType();
    echo ":";
    echo $type === null ? "none" : $type->getName() . ($type->allowsNull() ? "?" : "!");
    echo ";";
}
$listed = null;
foreach ((new ReflectionClass("EvalAotReflectCtorParamTarget"))->getMethods() as $candidate) {
    if ($candidate->getName() === "__construct") {
        $listed = $candidate;
    }
}
echo ":" . $listed->getNumberOfParameters() . "/" . $listed->getParameters()[0]->getName();
$plain = (new ReflectionClass("EvalAotReflectCtorPlain"))->getConstructor();
echo ":" . ($plain === null ? "null" : "bad");
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "M:__construct/EvalAotReflectCtorParamTarget:3/1:leftr-:string!;rightO=B:string!;countO=null:int?;:3/left:null"
    );
}

/// Verifies eval ReflectionMethod constructor/destructor predicates through the bridge.
#[test]
fn test_eval_reflection_method_reports_constructor_and_destructor() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectLifecycle {
    public function __construct() {}
    public function __destruct() {}
    public function run() {}
}
$ctor = new ReflectionMethod("EvalReflectLifecycle", "__CONSTRUCT");
echo $ctor->isConstructor() ? "C" : "c";
echo $ctor->isDestructor() ? "D" : "d";
echo ":";
$dtor = new ReflectionMethod("EvalReflectLifecycle", "__destruct");
echo $dtor->isConstructor() ? "C" : "c";
echo $dtor->isDestructor() ? "D" : "d";
echo ":";
$run = new ReflectionMethod("EvalReflectLifecycle", "run");
echo $run->isConstructor() ? "C" : "c";
echo $run->isDestructor() ? "D" : "d";
echo ":";
$listed = (new ReflectionClass("EvalReflectLifecycle"))->getConstructor();
echo $listed->isConstructor() ? "C" : "c";
echo $listed->isDestructor() ? "D" : "d";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Cd:cD:cd:Cd");
}

/// Verifies eval ReflectionMethod keeps declared name case after case-insensitive lookup.
#[test]
fn test_eval_reflection_method_preserves_declared_name_case() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectMethodCaseBase {
    public function MiXeDCase() { return "base"; }
}
class EvalReflectMethodCaseChild extends EvalReflectMethodCaseBase {
    public function childCase() { return "child"; }
}
$object = new EvalReflectMethodCaseChild();
$direct = new ReflectionMethod("EvalReflectMethodCaseChild", "mixedcase");
echo $direct->getName() . ":";
echo $direct->getShortName() . ":";
echo $direct->invoke($object) . ":";
$listed = (new ReflectionClass("EvalReflectMethodCaseChild"))->getMethod("CHILDCASE");
echo $listed->getName() . ":";
echo $listed->invoke($object);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "MiXeDCase:MiXeDCase:base:childCase:child");
}

/// Verifies eval ReflectionMethod accepts object targets through the bridge.
#[test]
fn test_eval_reflection_method_accepts_object_targets() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectMethodObjectBase {
    public function aotBase() { return "aot-base"; }
}
class EvalAotReflectMethodObjectChild extends EvalAotReflectMethodObjectBase {
    public function aotChild() { return "aot-child"; }
}
eval('class EvalReflectMethodObjectBase {
    public function MiXeDCase() { return "base"; }
}
class EvalReflectMethodObjectChild extends EvalReflectMethodObjectBase {
    public function childCase() { return "child"; }
}
$object = new EvalReflectMethodObjectChild();
$inherited = new ReflectionMethod($object, "mixedcase");
echo $inherited->getName() . ":";
echo $inherited->getDeclaringClass()->getName() . ":";
echo $inherited->invoke($object) . ":";
$own = new ReflectionMethod($object, "CHILDCASE");
echo $own->getName() . ":";
echo $own->getDeclaringClass()->getName() . ":";
echo $own->invoke($object) . "|";
$aot = new EvalAotReflectMethodObjectChild();
$aotInherited = new ReflectionMethod($aot, "aotbase");
echo $aotInherited->getName() . ":";
echo $aotInherited->getDeclaringClass()->getName() . ":";
$aotOwn = new ReflectionMethod($aot, "aotchild");
echo $aotOwn->getName() . ":";
echo $aotOwn->getDeclaringClass()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "MiXeDCase:EvalReflectMethodObjectBase:base:childCase:EvalReflectMethodObjectChild:child|aotbase:EvalAotReflectMethodObjectBase:aotchild:EvalAotReflectMethodObjectChild"
    );
}

/// Verifies eval ReflectionMethod::createFromMethodName resolves eval and AOT method strings.
#[test]
fn test_eval_reflection_method_create_from_method_name() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectCreateMethodTarget {
    public function aotRun() { return "aot"; }
}
eval('class EvalReflectCreateMethodTarget {
    public function MiXeDCase() { return "ok"; }
}
$ref = ReflectionMethod::createFromMethodName("EvalReflectCreateMethodTarget::mixedcase");
echo $ref->getDeclaringClass()->getName() . ":";
echo $ref->getName() . ":";
echo $ref->invoke(new EvalReflectCreateMethodTarget()) . "|";
$aot = ReflectionMethod::createFromMethodName("EvalAotReflectCreateMethodTarget::aotrun");
echo $aot->getDeclaringClass()->getName() . ":";
echo $aot->getName() . ":";
echo $aot->invoke(new EvalAotReflectCreateMethodTarget());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalReflectCreateMethodTarget:MiXeDCase:ok|EvalAotReflectCreateMethodTarget:aotrun:aot"
    );
}

/// Verifies eval ReflectionMethod accepts PHP's deprecated one-string constructor target.
#[test]
fn test_eval_reflection_method_accepts_single_method_string() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectCtorMethodTarget {
    public function aotRun() { return "aot"; }
}
$aot = new ReflectionMethod("EvalAotReflectCtorMethodTarget::aotrun");
echo $aot->getDeclaringClass()->getName() . ":";
echo $aot->getName() . ":";
echo $aot->invoke(new EvalAotReflectCtorMethodTarget()) . "|";
eval('class EvalReflectCtorMethodTarget {
    public function MiXeDCase() { return "ok"; }
}
$ref = new ReflectionMethod(objectOrMethod: "EvalReflectCtorMethodTarget::mixedcase");
echo $ref->getDeclaringClass()->getName() . ":";
echo $ref->getName() . ":";
echo $ref->invoke(new EvalReflectCtorMethodTarget());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalAotReflectCtorMethodTarget:aotrun:aot|EvalReflectCtorMethodTarget:MiXeDCase:ok"
    );
}

/// Verifies eval ReflectionMethod construction errors are catchable ReflectionException objects.
#[test]
fn test_eval_reflection_method_constructor_throws_reflection_exceptions() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectMissingMethodTarget {}
eval('
try {
    ReflectionMethod::createFromMethodName("EvalAotReflectMissingMethodTarget::missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
class EvalDynReflectMissingMethodTarget {}
try {
    new ReflectionMethod("EvalDynReflectMissingMethodTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionMethod("EvalDynReflectMissingMethodTarget::missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionMethod("EvalDynReflectMissingClass", "run");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    ReflectionMethod::createFromMethodName("not-a-method");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ReflectionException:Method EvalAotReflectMissingMethodTarget::missing() does not exist|Method EvalDynReflectMissingMethodTarget::missing() does not exist|Method EvalDynReflectMissingMethodTarget::missing() does not exist|Class \"EvalDynReflectMissingClass\" does not exist|ReflectionMethod::createFromMethodName(): Argument #1 ($method) must be a valid method name"
    );
}

/// Verifies eval ReflectionProperty construction errors are catchable ReflectionException objects.
#[test]
fn test_eval_reflection_property_constructor_throws_reflection_exceptions() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectMissingPropertyTarget {
    public $known = 1;
}
eval('
try {
    new ReflectionProperty("EvalAotReflectMissingPropertyTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
class EvalDynReflectMissingPropertyTarget {}
$object = new EvalDynReflectMissingPropertyTarget();
$object->dynamic = 1;
try {
    new ReflectionProperty("EvalDynReflectMissingPropertyTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionProperty("EvalDynReflectMissingPropertyClass", "value");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionProperty($object, "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
echo (new ReflectionProperty($object, "dynamic"))->getName() . ":";
echo (new ReflectionProperty("EvalAotReflectMissingPropertyTarget", "known"))->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ReflectionException:Property EvalAotReflectMissingPropertyTarget::$missing does not exist|Property EvalDynReflectMissingPropertyTarget::$missing does not exist|Class \"EvalDynReflectMissingPropertyClass\" does not exist|Property EvalDynReflectMissingPropertyTarget::$missing does not exist|dynamic:known"
    );
}

/// Verifies eval ReflectionClassConstant construction errors are catchable objects.
#[test]
fn test_eval_reflection_class_constant_constructor_throws_reflection_exceptions() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectMissingConstantTarget {
    public const OK = 1;
}
eval('
try {
    new ReflectionClassConstant("EvalAotReflectMissingConstantTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
class EvalDynReflectMissingConstantTarget {
    public const OK = 1;
}
try {
    new ReflectionClassConstant("EvalDynReflectMissingConstantTarget", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionClassConstant("EvalDynReflectMissingConstantClass", "VALUE");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
echo (new ReflectionClassConstant("EvalDynReflectMissingConstantTarget", "OK"))->getName() . ":";
echo (new ReflectionClassConstant("EvalAotReflectMissingConstantTarget", "OK"))->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ReflectionException:Constant EvalAotReflectMissingConstantTarget::missing does not exist|Constant EvalDynReflectMissingConstantTarget::missing does not exist|Class \"EvalDynReflectMissingConstantClass\" does not exist|OK:OK"
    );
}

/// Verifies eval ReflectionEnumUnitCase/BackedCase construction errors are catchable objects.
#[test]
fn test_eval_reflection_enum_case_constructor_throws_reflection_exceptions() {
    let out = compile_and_run_capture(
        r#"<?php
eval('enum EvalDynReflectMissingCaseUnit {
    case Ready;
    public const TOKEN = 1;
}
enum EvalDynReflectMissingCaseBacked: string {
    case Ready = "ready";
    public const TOKEN = 1;
}
class EvalDynReflectMissingCaseClass {
    public const TOKEN = 1;
}
try {
    new ReflectionEnumUnitCase("EvalDynReflectMissingCaseUnit", "Missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumUnitCase("EvalDynReflectMissingCaseClass", "TOKEN");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumUnitCase("EvalDynReflectMissingCaseUnit", "TOKEN");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumBackedCase("EvalDynReflectMissingCaseUnit", "Ready");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumBackedCase("EvalDynReflectMissingCaseBacked", "Missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnumBackedCase("EvalDynReflectMissingCaseClass", "Missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
echo (new ReflectionEnumUnitCase("EvalDynReflectMissingCaseBacked", "Ready"))->getName() . ":";
echo (new ReflectionEnumBackedCase("EvalDynReflectMissingCaseBacked", "Ready"))->getBackingValue();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ReflectionException:Constant EvalDynReflectMissingCaseUnit::Missing does not exist|Constant EvalDynReflectMissingCaseClass::TOKEN is not a case|Constant EvalDynReflectMissingCaseUnit::TOKEN is not a case|Enum case EvalDynReflectMissingCaseUnit::Ready is not a backed case|Constant EvalDynReflectMissingCaseBacked::Missing does not exist|Constant EvalDynReflectMissingCaseClass::Missing does not exist|Ready:ready"
    );
}

/// Verifies eval-declared final properties cannot be redeclared by subclasses.
#[test]
fn test_eval_declared_final_property_override_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalFinalPropertyBase {
    final public $value = 1;
}
class EvalFinalPropertyChild extends EvalFinalPropertyBase {
    public $value = 2;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval reflectors expose their declaring class through the bridge.
#[test]
fn test_eval_reflection_members_report_declaring_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalDeclaringBase {
    public $baseProp = 1;
    public function inherited() { return "base"; }
    public const BASE_CONST = 10;
}
class EvalDeclaringChild extends EvalDeclaringBase {
    public $childProp = 2;
    public function own() { return "child"; }
    public const CHILD_CONST = 20;
}
enum EvalDeclaringEnum: string {
    case Ready = "ready";
    public const LEVEL = 3;
}
echo (new ReflectionMethod("EvalDeclaringChild", "inherited"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getMethod("own")->getDeclaringClass()->getName() . ":";
echo (new ReflectionProperty("EvalDeclaringChild", "baseProp"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getProperty("childProp")->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringChild"))->getReflectionConstant("BASE_CONST")->getDeclaringClass()->getName() . ":";
echo (new ReflectionClassConstant("EvalDeclaringChild", "BASE_CONST"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass("EvalDeclaringEnum"))->getReflectionConstant("Ready")->getDeclaringClass()->getName() . ":";
echo (new ReflectionEnumBackedCase("EvalDeclaringEnum", "Ready"))->getDeclaringClass()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalDeclaringBase:EvalDeclaringChild:EvalDeclaringBase:EvalDeclaringChild:EvalDeclaringBase:EvalDeclaringBase:EvalDeclaringEnum:EvalDeclaringEnum"
    );
}

/// Verifies eval ReflectionClass getMethods/getProperties return member objects through the bridge.
#[test]
fn test_eval_reflection_class_lists_member_objects() {
    let out = compile_and_run_capture(
        r#"<?php
eval('#[Attribute]
class EvalListMarker {}
class EvalReflectListTarget {
    #[EvalListMarker]
    public function first() {}
    private static function helper() {}
    #[EvalListMarker]
    protected $visible;
    private static $token;
}
$ref = new ReflectionClass("EvalReflectListTarget");
$methods = $ref->getMethods();
$properties = $ref->getProperties();
$staticMethods = $ref->getMethods(ReflectionMethod::IS_STATIC);
$privateMethods = $ref->getMethods(filter: ReflectionMethod::IS_PRIVATE);
$noMethods = $ref->getMethods(0);
$nullMethods = $ref->getMethods(null);
$staticProperties = $ref->getProperties(ReflectionProperty::IS_STATIC);
$protectedProperties = $ref->getProperties(filter: ReflectionProperty::IS_PROTECTED);
$noProperties = $ref->getProperties(0);
echo count($methods) . ":" . count($properties) . ":";
echo ReflectionMethod::IS_STATIC . ":" . ReflectionMethod::IS_PRIVATE . ":";
$direct = new ReflectionMethod("EvalReflectListTarget", "helper");
echo "D" . $direct->getModifiers() . ":";
foreach ($methods as $method) {
    if ($method->getName() === "first") {
        echo "F" . count($method->getAttributes());
        echo "M" . $method->getModifiers();
    }
    if ($method->getName() === "helper") {
        echo $method->isStatic() ? "S" : "s";
        echo $method->isPrivate() ? "R" : "r";
        echo "M" . $method->getModifiers();
    }
}
echo ":";
foreach ($properties as $property) {
    if ($property->getName() === "visible") {
        echo "V" . count($property->getAttributes());
        echo $property->isProtected() ? "P" : "p";
        echo "M" . $property->getModifiers();
    }
    if ($property->getName() === "token") {
        echo $property->isStatic() ? "T" : "t";
        echo $property->isPrivate() ? "R" : "r";
        echo "M" . $property->getModifiers();
    }
}
echo ":";
echo count($staticMethods) . $staticMethods[0]->getName() . ":";
echo count($privateMethods) . $privateMethods[0]->getName() . ":";
echo count($noMethods) . ":" . count($nullMethods) . ":";
echo count($staticProperties) . $staticProperties[0]->getName() . ":";
echo count($protectedProperties) . $protectedProperties[0]->getName() . ":";
echo count($noProperties);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "2:2:16:4:D20:F1M1SRM20:V1PM2TRM20:1helper:1helper:0:2:1token:1visible:0"
    );
}

/// Verifies eval ReflectionClass getMethod/getProperty return single member objects.
#[test]
fn test_eval_reflection_class_get_method_and_property_lookup_members() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectLookupTarget {
    public function first() {}
    private static function helper() {}
    protected $visible;
    private static $token;
}
$ref = new ReflectionClass("EvalReflectLookupTarget");
$method = $ref->getMethod("FIRST");
echo $method->getName() . ":";
echo $method->isPublic() ? "U" : "u";
echo ":";
$helper = $ref->getMethod("helper");
echo $helper->isPrivate() ? "P" : "p";
echo $helper->isStatic() ? "S" : "s";
echo ":";
$property = $ref->getProperty("visible");
echo $property->getName() . ":";
echo $property->isProtected() ? "R" : "r";
echo ":";
try {
    $ref->getProperty("Visible");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo ":";
try {
    $ref->getMethod("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "first:U:PS:visible:R:Property EvalReflectLookupTarget::$Visible does not exist:Method EvalReflectLookupTarget::missing() does not exist"
    );
}

/// Verifies eval ReflectionMethod materializes ReflectionParameter objects through the bridge.
#[test]
fn test_eval_reflection_method_lists_parameters() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalReflectLeft {}
interface EvalReflectRight {}
class EvalReflectParamTarget {
    public function run(#[EvalParamTag("first")] int &$first, int|string $union, #[EvalParamTag("both")] EvalReflectLeft&EvalReflectRight $both, ?array $items = null, ?callable $callback = null, \App\Name|null $second = null, &...$rest) {}
}
$method = new ReflectionMethod("EvalReflectParamTarget", "run");
echo $method->getNumberOfParameters() . "/";
echo $method->getNumberOfRequiredParameters() . ":";
$params = $method->getParameters();
foreach ($params as $param) {
    echo $param->getName() . "@" . $param->getPosition();
    echo $param->isOptional() ? "O" : "r";
    echo $param->isVariadic() ? "V" : "v";
    echo $param->isPassedByReference() ? "R" : "b";
    echo $param->canBePassedByValue() ? "Y" : "N";
    echo $param->hasType() ? "T" : "t";
    echo $param->allowsNull() ? "N" : "n";
    echo $param->isArray() ? "A" : "a";
    echo $param->isCallable() ? "C" : "c";
    $type = $param->getType();
    if ($param->getName() == "union") {
        echo ":union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($param->getName() == "both") {
        echo ":intersection";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($type) {
        echo ":" . $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } else {
        echo ":null";
    }
    $attrs = $param->getAttributes();
    echo ":A" . count($attrs);
    if (count($attrs) > 0) {
        echo ":" . $attrs[0]->getName();
        echo ":" . $attrs[0]->getArguments()[0];
    }
    echo $param->isDefaultValueAvailable() ? ":D" : ":d";
    if ($param->isDefaultValueAvailable()) {
        echo "=";
        echo $param->getDefaultValue() === null ? "null" : $param->getDefaultValue();
    }
    echo "|";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "7/3:first@0rvRNTnac:int!B:A1:EvalParamTag:first:d|union@1rvbYTnac:union!:intB:stringB:A0:d|both@2rvbYTnac:intersection!:EvalReflectLeftC:EvalReflectRightC:A1:EvalParamTag:both:d|items@3OvbYTNAc:array?B:A0:D=null|callback@4OvbYTNaC:callable?B:A0:D=null|second@5OvbYTNac:App\\Name?C:A0:D=null|rest@6OVRNtNac:null:A0:d|"
    );
}

/// Verifies eval ReflectionType objects stringify retained parameter metadata.
#[test]
fn test_eval_reflection_type_to_string_formats_retained_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectTypeStringDep {}
interface EvalReflectTypeStringLeft {}
interface EvalReflectTypeStringRight {}
class EvalReflectTypeStringTarget {
    public function run(?EvalReflectTypeStringDep $dep, int|string|null $union, EvalReflectTypeStringLeft&EvalReflectTypeStringRight $both, mixed $mixed, ?array $items) {}
}
$params = (new ReflectionMethod("EvalReflectTypeStringTarget", "run"))->getParameters();
foreach ($params as $param) {
    $type = $param->getType();
    echo $param->getName() . ":";
    echo $type->__toString() . "|";
}
$unionType = (new ReflectionParameter(["EvalReflectTypeStringTarget", "run"], "union"))->getType();
echo "cast:" . (string)$unionType . "|";
echo "concat:" . $unionType . "|";
echo "echo:";
echo $unionType;
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "dep:?EvalReflectTypeStringDep|union:int|string|null|both:EvalReflectTypeStringLeft&EvalReflectTypeStringRight|mixed:mixed|items:?array|cast:int|string|null|concat:int|string|null|echo:int|string|null"
    );
}

/// Verifies eval ReflectionParameter stringifies retained parameter metadata.
#[test]
fn test_eval_reflection_parameter_to_string() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectParameterStringTarget {
    const LABEL = "L";
    public function run(string $name, int $count = 3, $label = self::LABEL, &...$items) {}
}
$params = (new ReflectionMethod("EvalReflectParameterStringTarget", "run"))->getParameters();
foreach ($params as $param) {
    echo $param->__toString();
    echo "|";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Parameter #0 [ <required> string $name ]|Parameter #1 [ <optional> int $count = 3 ]|Parameter #2 [ <optional> $label = self::LABEL ]|Parameter #3 [ <optional> &...$items ]|"
    );
}

/// Verifies eval ReflectionParameter::getClass() reports retained object type metadata.
#[test]
fn test_eval_reflection_parameter_get_class_reports_named_object_type() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectParamClassDep {}
interface EvalReflectParamClassLeft {}
interface EvalReflectParamClassRight {}
class EvalReflectParamClassTarget {
    public function run(EvalReflectParamClassDep $dep, ?EvalReflectParamClassDep $nullable, int $id, EvalReflectParamClassDep|int $unionObject, EvalReflectParamClassLeft&EvalReflectParamClassRight $intersection, $plain) {}
}
function eval_reflect_param_class_function(EvalReflectParamClassDep $dep) {}
$params = (new ReflectionMethod("EvalReflectParamClassTarget", "run"))->getParameters();
foreach ($params as $param) {
    $class = $param->getClass();
    echo $param->getName() . ":" . ($class ? $class->getName() : "null") . "|";
}
$direct = new ReflectionParameter(["EvalReflectParamClassTarget", "run"], "nullable");
echo "direct:" . $direct->getClass()->getName() . "|";
$functionParam = new ReflectionParameter("eval_reflect_param_class_function", "dep");
echo "function:" . $functionParam->getClass()->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "dep:EvalReflectParamClassDep|nullable:EvalReflectParamClassDep|id:null|unionObject:null|intersection:null|plain:null|direct:EvalReflectParamClassDep|function:EvalReflectParamClassDep"
    );
}

/// Verifies eval direct ReflectionParameter construction accepts runtime object method targets.
#[test]
fn test_eval_reflection_parameter_accepts_object_expression_target() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalDirectParamObjectTarget {
    public function run(int $id, ?string $name = null) {}
}
$param = new ReflectionParameter([new EvalDirectParamObjectTarget(), "run"], 1);
echo $param->getName() . ":";
echo $param->getPosition() . ":";
echo $param->getDeclaringClass()->getName() . ":";
echo $param->getDeclaringFunction()->getName() . ":";
echo ($param->isOptional() ? "O" : "R") . ":";
echo $param->getType()->getName() . ":";
echo $param->allowsNull() ? "N" : "n";
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "name:1:EvalDirectParamObjectTarget:run:O:string:N"
    );
}

/// Verifies eval ReflectionParameter construction errors are catchable objects.
#[test]
fn test_eval_reflection_parameter_constructor_throws_reflection_exceptions() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_dyn_reflect_param_error_function($known) {}
class EvalDynReflectParamErrorTarget {
    public function run($known) {}
}
try {
    new ReflectionParameter("eval_dyn_reflect_param_error_function", "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalDynReflectParamErrorTarget", "run"], "missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalDynReflectParamErrorTarget", "run"], 3);
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalDynReflectParamErrorTarget", "missing"], "known");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionParameter(["EvalDynReflectParamErrorTarget", "run"], -1);
    echo "bad";
} catch (ValueError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
echo (new ReflectionParameter(["EvalDynReflectParamErrorTarget", "run"], "known"))->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ReflectionException:The parameter specified by its name could not be found|The parameter specified by its name could not be found|The parameter specified by its offset could not be found|Method EvalDynReflectParamErrorTarget::missing() does not exist|ValueError:ReflectionParameter::__construct(): Argument #2 ($param) must be greater than or equal to 0|known"
    );
}

/// Verifies eval ReflectionParameter exposes PHP constant-default metadata.
#[test]
fn test_eval_reflection_parameter_reports_default_constant_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('define("EVAL_REFLECT_PARAM_DEFAULT_GLOBAL", "G");
class EvalReflectParamDefaultBase {
    const BASE = "B";
}
class EvalReflectParamDefaultTarget extends EvalReflectParamDefaultBase {
    const LABEL = "L";
    public function run($required, $global = EVAL_REFLECT_PARAM_DEFAULT_GLOBAL, $self = self::LABEL, $parent = parent::BASE, $literal = 7) {}
}
$params = (new ReflectionMethod("EvalReflectParamDefaultTarget", "run"))->getParameters();
foreach ($params as $param) {
    echo $param->getName() . ":";
    echo $param->isDefaultValueAvailable() ? "D:" : "d:";
    if ($param->isDefaultValueAvailable()) {
        if ($param->isDefaultValueConstant()) {
            echo "C:";
            echo $param->getDefaultValueConstantName();
            echo ":";
        } else {
            echo "c:null:";
        }
        echo $param->getDefaultValue();
    }
    echo "|";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "required:d:|global:D:C:EVAL_REFLECT_PARAM_DEFAULT_GLOBAL:G|self:D:C:self::LABEL:L|parent:D:C:parent::BASE:B|literal:D:c:null:7|"
    );
}

/// Verifies eval ReflectionParameter default magic constants use callable scopes through the bridge.
#[test]
fn test_eval_reflection_parameter_resolves_default_magic_constants() {
    let out = compile_and_run_capture(
        r#"<?php
eval('namespace EvalReflectParamMagicNs;
function eval_reflect_param_magic($fn = __FUNCTION__, $m = __METHOD__, $c = __CLASS__, $t = __TRAIT__, $n = __NAMESPACE__) {}
interface EvalReflectParamMagicIface {
    public function read($c = __CLASS__, $m = __METHOD__, $fn = __FUNCTION__, $t = __TRAIT__, $n = __NAMESPACE__);
}
trait EvalReflectParamMagicTrait {
    public function source($c = __CLASS__, $t = __TRAIT__, $m = __METHOD__, $fn = __FUNCTION__, $n = __NAMESPACE__) {}
}
class EvalReflectParamMagicBox {
    use EvalReflectParamMagicTrait { source as aliasSource; }
    public function own($c = __CLASS__, $t = __TRAIT__, $m = __METHOD__, $fn = __FUNCTION__, $n = __NAMESPACE__) {}
}
function eval_param_magic_dump($ref) {
    foreach ($ref->getParameters() as $param) {
        echo "[" . $param->getDefaultValue() . "]";
    }
    echo ":";
}
eval_param_magic_dump(new \ReflectionFunction(__NAMESPACE__ . "\\\\eval_reflect_param_magic"));
eval_param_magic_dump(new \ReflectionMethod(EvalReflectParamMagicBox::class, "own"));
eval_param_magic_dump(new \ReflectionMethod(EvalReflectParamMagicBox::class, "aliasSource"));
eval_param_magic_dump(new \ReflectionMethod(EvalReflectParamMagicIface::class, "read"));');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        concat!(
            "[EvalReflectParamMagicNs\\eval_reflect_param_magic]",
            "[EvalReflectParamMagicNs\\eval_reflect_param_magic]",
            "[][][EvalReflectParamMagicNs]:",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicBox]",
            "[]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicBox::own]",
            "[own]",
            "[EvalReflectParamMagicNs]:",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicBox]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicTrait]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicTrait::source]",
            "[source]",
            "[EvalReflectParamMagicNs]:",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicIface]",
            "[EvalReflectParamMagicNs\\EvalReflectParamMagicIface::read]",
            "[read]",
            "[]",
            "[EvalReflectParamMagicNs]:"
        )
    );
}

/// Verifies eval ReflectionMethod exposes eval-declared return type metadata.
#[test]
fn test_eval_reflection_method_reports_return_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalReflectReturnIface {
    public function read(): string;
}
class EvalReflectReturnTarget implements EvalReflectReturnIface {
    public function read(): string { return "ok"; }
    public function selfReturn(): static { return $this; }
    public function done(): void {}
}
$iface = new ReflectionMethod("EvalReflectReturnIface", "read");
$ifaceType = $iface->getReturnType();
echo ($iface->hasReturnType() ? "I" : "i") . ":";
echo $ifaceType->getName() . ":";
echo ($ifaceType->isBuiltin() ? "B" : "b") . ":";
$self = (new ReflectionMethod("EvalReflectReturnTarget", "selfReturn"))->getReturnType();
echo $self->getName() . ":";
echo ($self->isBuiltin() ? "B" : "b") . ":";
$void = (new ReflectionMethod("EvalReflectReturnTarget", "done"))->getReturnType();
echo $void->getName() . ":";
echo ($void->allowsNull() ? "N" : "n") . ":";
echo $void->isBuiltin() ? "B" : "b";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "I:string:B:static:b:void:n:B");
}

/// Verifies eval ReflectionProperty materializes property get/set type metadata through the bridge.
#[test]
fn test_eval_reflection_property_get_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectPropertyTypeDep {}
class EvalReflectPropertyTypeTarget {
    public int $id;
    public ?string $name;
    public EvalReflectPropertyTypeDep $dep;
    public $plain;
    public int|string $union;
}
$properties = (new ReflectionClass("EvalReflectPropertyTypeTarget"))->getProperties();
foreach ($properties as $property) {
    echo $property->getName() . ":";
    echo $property->hasType() ? "T:" : "t:";
    $type = $property->getType();
    if ($property->getName() == "union") {
        echo "union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($type) {
        echo $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } else {
        echo "null";
    }
    echo "|";
}
$direct = new ReflectionProperty("EvalReflectPropertyTypeTarget", "dep");
$directType = $direct->getType();
echo "direct:";
echo $direct->hasType() ? "T:" : "t:";
echo $directType->getName();
$directSettableType = $direct->getSettableType();
echo ":set:" . $directSettableType->getName();
$plain = new ReflectionProperty("EvalReflectPropertyTypeTarget", "plain");
echo ":plainSet:" . ($plain->getSettableType() === null ? "N" : "n");
$directUnion = new ReflectionProperty("EvalReflectPropertyTypeTarget", "union");
echo ":unionSet:" . count($directUnion->getSettableType()->getTypes());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "id:T:int!B|name:T:string?B|dep:T:EvalReflectPropertyTypeDep!C|plain:t:null|union:T:union!:intB:stringB|direct:T:EvalReflectPropertyTypeDep:set:EvalReflectPropertyTypeDep:plainSet:N:unionSet:2"
    );
}

/// Verifies eval ReflectionProperty uses explicit set-hook parameter metadata for settable type.
#[test]
fn test_eval_reflection_property_get_settable_type_uses_set_hook_parameter() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectSettableTypeTarget {
    public string $value {
        get => $this->value;
        set(int|string $raw) => (string) $raw;
    }
}
$property = new ReflectionProperty("EvalReflectSettableTypeTarget", "value");
$type = $property->getType();
$settable = $property->getSettableType();
echo $type->getName() . ":";
echo count($settable->getTypes());
foreach ($settable->getTypes() as $memberType) {
    echo ":" . $memberType->getName();
    echo $memberType->isBuiltin() ? "B" : "C";
}
$setHook = $property->getHook(PropertyHookType::Set);
$paramType = $setHook->getParameters()[0]->getType();
echo ":" . count($paramType->getTypes());
$box = new EvalReflectSettableTypeTarget();
$box->value = 7;
echo ":" . $box->value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "string:2:intB:stringB:2:7");
}

/// Verifies eval ReflectionProperty materializes property default metadata through the bridge.
#[test]
fn test_eval_reflection_property_get_default_value_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectPropertyDefaultTarget {
    public $implicit;
    public int $typed;
    public ?string $nullableTyped;
    public $explicitNull = null;
    public int $count = 7;
    public static string $label = "ok";
}
foreach (["implicit", "typed", "nullableTyped", "explicitNull", "count", "label"] as $name) {
    $property = new ReflectionProperty("EvalReflectPropertyDefaultTarget", $name);
    echo $property->getName() . ":";
    echo $property->isDefault() ? "Y:" : "N:";
    echo $property->hasDefaultValue() ? "D:" : "d:";
    $value = $property->getDefaultValue();
    echo $value === null ? "null" : $value;
    echo "|";
}
$listed = (new ReflectionClass("EvalReflectPropertyDefaultTarget"))->getProperty("implicit");
echo "listed:";
echo $listed->isDefault() ? "Y:" : "N:";
echo $listed->hasDefaultValue() ? "D:" : "d:";
echo $listed->getDefaultValue() === null ? "null" : "bad";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "implicit:Y:D:null|typed:Y:d:null|nullableTyped:Y:d:null|explicitNull:Y:D:null|count:Y:D:7|label:Y:D:ok|listed:Y:D:null"
    );
}

/// Verifies eval ReflectionProperty materializes dynamic object properties through the bridge.
#[test]
fn test_eval_reflection_property_supports_dynamic_properties() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectDynamicBridgeBase {}
class EvalReflectDynamicBridgeChild extends EvalReflectDynamicBridgeBase {}
$object = new EvalReflectDynamicBridgeBase();
$object->dynamic = "first";
$child = new EvalReflectDynamicBridgeChild();
$child->dynamic = "child";
$empty = new EvalReflectDynamicBridgeChild();
$property = new ReflectionProperty($object, "dynamic");
echo $property->getName(); echo ":";
echo $property->isDynamic() ? "D" : "d"; echo ":";
echo $property->isDefault() ? "Y" : "N"; echo ":";
echo $property->getModifiers(); echo ":";
echo $property->hasDefaultValue() ? "H" : "h"; echo ":";
echo is_null($property->getType()) ? "T" : "t"; echo ":";
echo $property->isInitialized($object) ? "I" : "i"; echo ":";
echo $property->getValue($object); echo ":";
echo $property->getValue($child); echo ":";
echo $property->isInitialized($empty) ? "E" : "e"; echo ":";
$property->setValue($empty, "filled");
echo $property->getValue($empty); echo ":";
$property->setRawValue($object, "raw");
echo $property->getRawValue($object); echo ":";
echo str_replace("\n", "\\n", $property->__toString());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "dynamic:D:N:1:h:T:I:first:child:e:filled:raw:Property [ <dynamic> public $dynamic ]\n"
    );
}

/// Verifies eval ReflectionProperty formats retained property metadata through `__toString()`.
#[test]
fn test_eval_reflection_property_to_string() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectPropertyStringTarget {
    public int $id = 7;
    protected static string $label = "ok";
    private $implicit;
    public $virtual {
        get => 1;
    }
}
foreach (["id", "label", "implicit", "virtual"] as $name) {
    echo (new ReflectionProperty("EvalReflectPropertyStringTarget", $name))->__toString();
    echo "|";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Property [ public int $id = 7 ]|Property [ protected static string $label = 'ok' ]|Property [ private $implicit = NULL ]|Property [ public $virtual ]|"
    );
}

/// Verifies eval ReflectionClass materializes property default metadata through the bridge.
#[test]
fn test_eval_reflection_class_get_default_properties_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
	eval('class EvalReflectClassDefaultBase {
    public int $base = 1;
    protected string $prot = "p";
    private int $shadow = 3;
    public $implicit;
    public int $typed;
    public static string $baseStatic = "bs";
}
class EvalReflectClassDefaultChild extends EvalReflectClassDefaultBase {
    public int $child = 5;
    private int $shadow = 9;
    public static int $childStatic = 7;
    public ?int $nullable = null;
}
$defaults = (new ReflectionClass("EvalReflectClassDefaultChild"))->getDefaultProperties();
echo $defaults["childStatic"] . ":";
echo $defaults["baseStatic"] . ":";
echo $defaults["child"] . ":";
echo $defaults["shadow"] . ":";
echo $defaults["base"] . ":";
echo $defaults["prot"] . ":";
echo array_key_exists("implicit", $defaults) && $defaults["implicit"] === null ? "I:" : "i:";
echo array_key_exists("nullable", $defaults) && $defaults["nullable"] === null ? "N:" : "n:";
echo array_key_exists("typed", $defaults) ? "T" : "t";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:bs:5:9:1:p:I:N:t");
}

/// Verifies eval ReflectionProperty value APIs use current runtime object values.
#[test]
fn test_eval_reflection_property_gets_and_sets_values() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectValueBase {
    private $secret = "base";
    public static $count = 1;
}
class EvalReflectValueChild extends EvalReflectValueBase {
    protected $name = "Ada";
}
class EvalReflectValueHook {
    public $raw = 2;
    public $doubled {
        get => $this->raw * 2;
        set { $this->raw = $value + 1; }
    }
    public $backed {
        get { return $this->backed * 2; }
        set { $this->backed = $value; }
    }
    public $virtual {
        get => $this->raw + 100;
    }
    public function __construct() {
        $this->backed = 2;
    }
}
$child = new EvalReflectValueChild();
$secret = new ReflectionProperty("EvalReflectValueBase", "secret");
echo $secret->getValue($child) . ":";
$secret->setValue($child, "changed");
echo $secret->getValue(object: $child) . ":";
$name = new ReflectionProperty("EvalReflectValueChild", "name");
echo $name->getValue($child) . ":";
$name->setValue(objectOrValue: $child, value: "Grace");
echo $name->getValue($child) . ":";
$count = new ReflectionProperty("EvalReflectValueBase", "count");
echo $count->getValue() . ":";
$count->setValue(5);
echo EvalReflectValueChild::$count . ":";
$count->setValue(null, 6);
echo $count->getValue($child) . ":";
$hook = new EvalReflectValueHook();
$doubled = new ReflectionProperty("EvalReflectValueHook", "doubled");
echo $doubled->getValue($hook) . ":";
$doubled->setValue($hook, 4);
echo $hook->raw . ":";
echo $doubled->getValue($hook) . ":";
$backed = new ReflectionProperty("EvalReflectValueHook", "backed");
echo $backed->getRawValue($hook) . ":";
echo $backed->getValue($hook) . ":";
$backed->setValue($hook, 4);
echo $backed->getRawValue(object: $hook) . ":";
echo $backed->getValue($hook) . ":";
$backed->setRawValue(object: $hook, value: 7);
echo $backed->getRawValue($hook) . ":";
echo $backed->getValue($hook) . ":";
echo $backed->isLazy($hook) ? "L:" : "l:";
$backed->skipLazyInitialization(object: $hook);
$backed->setRawValueWithoutLazyInitialization(object: $hook, value: 8);
echo $backed->getRawValue($hook) . ":";
echo $backed->getValue($hook) . ":";
echo $backed->getModifiers() . ":";
echo $backed->isVirtual() ? "V:" : "b:";
echo (new ReflectionProperty("EvalReflectValueHook", "virtual"))->isVirtual() ? "V:" : "b:";
echo (new ReflectionProperty("EvalReflectValueHook", "virtual"))->getModifiers();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "base:changed:Ada:Grace:1:5:6:4:5:10:2:4:4:8:7:14:l:8:16:1:b:V:513"
    );
}

/// Verifies eval ReflectionProperty raw APIs reject virtual property hooks.
#[test]
fn test_eval_reflection_property_virtual_raw_value_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class EvalReflectVirtualRawHook {
    public $raw = 2;
    public $virtual {
        get => $this->raw * 2;
    }
}
$object = new EvalReflectVirtualRawHook();
$property = new ReflectionProperty("EvalReflectVirtualRawHook", "virtual");
$property->getRawValue($object);');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval ReflectionProperty reports instance and static initialization state.
#[test]
fn test_eval_reflection_property_is_initialized() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectInitializedTarget {
    public int $typed;
    public ?int $nullable;
    public $plain;
    public static int $staticTyped;
    public static $staticPlain;
    public $virtual {
        get => 42;
    }
}
$object = new EvalReflectInitializedTarget();
$typed = new ReflectionProperty("EvalReflectInitializedTarget", "typed");
$nullable = new ReflectionProperty("EvalReflectInitializedTarget", "nullable");
$plain = new ReflectionProperty("EvalReflectInitializedTarget", "plain");
$staticTyped = new ReflectionProperty("EvalReflectInitializedTarget", "staticTyped");
$staticPlain = new ReflectionProperty("EvalReflectInitializedTarget", "staticPlain");
$virtual = new ReflectionProperty("EvalReflectInitializedTarget", "virtual");
echo $typed->isInitialized($object) ? "T:" : "t:";
echo $plain->isInitialized(object: $object) ? "P:" : "p:";
echo $staticTyped->isInitialized() ? "S:" : "s:";
echo $staticPlain->isInitialized() ? "N:" : "n:";
EvalReflectInitializedTarget::$staticTyped = 3;
echo $staticTyped->isInitialized() ? "S:" : "s:";
$object->typed = 5;
echo $typed->isInitialized($object) ? "T:" : "t:";
unset($object->typed);
echo $typed->isInitialized($object) ? "T:" : "t:";
$typed->setRawValue(object: $object, value: 9);
echo $typed->isInitialized($object) ? "T:" : "t:";
echo $nullable->isInitialized($object) ? "Y:" : "y:";
$nullable->setValue($object, null);
echo $nullable->isInitialized($object) ? "Y:" : "y:";
echo $virtual->isInitialized($object) ? "V" : "v";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "t:P:s:N:S:T:t:T:y:Y:V");
}

/// Verifies eval ReflectionProperty initialization checks bridge generated/AOT storage.
#[test]
fn test_eval_reflection_property_is_initialized_bridge_aot_storage() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectInitializedTarget {
    public string $name = "Ada";
    public int $typed;
    private int $secret;
    public static string $label = "ready";
    public static int $staticTyped;
    private static int $hidden;

    public function reveal() {
        return $this->secret . "," . self::$hidden;
    }
}
$object = new EvalAotReflectInitializedTarget();
echo eval('$name = new ReflectionProperty("EvalAotReflectInitializedTarget", "name");
$typed = new ReflectionProperty("EvalAotReflectInitializedTarget", "typed");
$secret = new ReflectionProperty("EvalAotReflectInitializedTarget", "secret");
$label = new ReflectionProperty("EvalAotReflectInitializedTarget", "label");
$staticTyped = new ReflectionProperty("EvalAotReflectInitializedTarget", "staticTyped");
$hidden = new ReflectionProperty("EvalAotReflectInitializedTarget", "hidden");
echo $name->isInitialized($object) ? "N:" : "n:";
echo $typed->isInitialized($object) ? "T:" : "t:";
echo $secret->isInitialized($object) ? "P:" : "p:";
echo $label->isInitialized() ? "L:" : "l:";
echo $staticTyped->isInitialized() ? "S:" : "s:";
echo $hidden->isInitialized() ? "H:" : "h:";
$typed->setValue($object, 5);
$secret->setValue($object, 7);
$staticTyped->setValue(11);
$hidden->setValue(13);
echo $typed->isInitialized($object) ? "T:" : "t:";
echo $secret->isInitialized($object) ? "P:" : "p:";
echo $staticTyped->isInitialized() ? "S:" : "s:";
echo $hidden->isInitialized() ? "H:" : "h:";
echo $object->reveal();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "N:t:p:L:s:h:T:P:S:H:7,13");
}

/// Verifies eval ReflectionProperty getValue rejects uninitialized generated/AOT typed storage.
#[test]
fn test_eval_reflection_property_get_value_rejects_uninitialized_aot_storage() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectUninitializedGetTarget {
    public string $name = "Ada";
    public int $typed;
}
$object = new EvalAotReflectUninitializedGetTarget();
echo eval('$name = new ReflectionProperty("EvalAotReflectUninitializedGetTarget", "name");
$typed = new ReflectionProperty("EvalAotReflectUninitializedGetTarget", "typed");
echo $name->getValue($object) . ":";
echo $typed->getValue($object);
');
"#,
    );
    assert!(
        !out.success,
        "program unexpectedly succeeded: stdout={:?}",
        out.stdout
    );
    assert_eq!(out.stdout, "Ada:");
    assert!(
        out.stderr.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {}",
        out.stderr
    );
}

/// Verifies eval ReflectionProperty raw APIs bridge generated/AOT instance storage.
#[test]
fn test_eval_reflection_property_raw_value_apis_bridge_aot_storage() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectRawPropertyTarget {
    public string $name = "Ada";
    private int $secret = 3;
    public int $typed;

    public function reveal() {
        return $this->secret . ":" . $this->typed;
    }
}
$object = new EvalAotReflectRawPropertyTarget();
echo eval('$name = new ReflectionProperty("EvalAotReflectRawPropertyTarget", "name");
$secret = new ReflectionProperty("EvalAotReflectRawPropertyTarget", "secret");
$typed = new ReflectionProperty("EvalAotReflectRawPropertyTarget", "typed");
echo $name->getRawValue($object) . ":";
$name->setRawValue($object, "Grace");
echo $object->name . ":";
echo $secret->getRawValue($object) . ":";
$secret->setRawValue($object, 9);
echo $typed->isLazy($object) ? "L:" : "l:";
$typed->skipLazyInitialization(object: $object);
$typed->setRawValueWithoutLazyInitialization(object: $object, value: 11);
echo $object->reveal();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada:Grace:3:l:9:11");
}

/// Verifies eval ReflectionProperty exposes property hook metadata and hook methods.
#[test]
fn test_eval_reflection_property_hook_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectHookedProperty {
    public int $raw = 2;
    public int $doubled {
        get { return $this->raw * 2; }
        set { $this->raw = $value; }
    }
    public int $readonlyHook {
        get => $this->raw + 1;
    }
    public int $plain = 5;
}
abstract class EvalReflectAbstractHookProperty {
    abstract public int $contract { get; set; }
}
interface EvalReflectInterfaceHookProperty {
    public int $iface { get; }
}
$hooked = new ReflectionProperty("EvalReflectHookedProperty", "doubled");
$plain = new ReflectionProperty("EvalReflectHookedProperty", "plain");
$readonly = new ReflectionProperty("EvalReflectHookedProperty", "readonlyHook");
$abstract = new ReflectionProperty("EvalReflectAbstractHookProperty", "contract");
$iface = new ReflectionProperty("EvalReflectInterfaceHookProperty", "iface");
$getCase = PropertyHookType::Get;
$setCase = PropertyHookType::Set;
echo $getCase->name . ":" . $getCase->value . ":";
$caseList = PropertyHookType::cases();
echo count($caseList) . ":" . $caseList[0]->name . ":" . $caseList[1]->value . ":";
echo PropertyHookType::from("set")->name . ":";
echo (PropertyHookType::tryFrom("missing") === null ? "T" : "t") . ":";
echo ($hooked->hasHooks() ? "H" : "h") . ":";
echo ($hooked->hasHook($getCase) ? "G" : "g") . ":";
echo ($hooked->hasHook(type: $setCase) ? "S" : "s") . ":";
$hooks = $hooked->getHooks();
echo count($hooks) . ":" . $hooks["get"]->getName() . ":" . $hooks["set"]->getName() . ":";
$get = $hooked->getHook($getCase);
$set = $hooked->getHook(type: $setCase);
echo $get->getDeclaringClass()->getName() . ":" . $get->getNumberOfParameters() . ":";
echo $set->getNumberOfParameters() . ":" . $set->getParameters()[0]->getName() . ":";
$box = new EvalReflectHookedProperty();
echo $get->invoke($box) . ":";
$set->invoke($box, 7);
echo $box->raw . ":";
echo ($readonly->hasHook($getCase) ? "R" : "r") . ":";
echo ($readonly->hasHook($setCase) ? "w" : "W") . ":";
echo ($readonly->getHook($setCase) === null ? "N" : "n") . ":";
echo ($plain->hasHooks() ? "bad" : "plain") . ":";
echo count($plain->getHooks()) . ":";
$abstractHooks = $abstract->getHooks();
echo count($abstractHooks) . ":";
echo ($abstract->hasHook($getCase) ? "AG" : "ag") . ":";
echo ($abstract->hasHook($setCase) ? "AS" : "as") . ":";
echo $abstractHooks["get"]->getName() . ":" . ($abstractHooks["get"]->isAbstract() ? "A" : "a") . ":";
echo $abstractHooks["set"]->getName() . ":" . ($abstractHooks["set"]->isAbstract() ? "A" : "a") . ":";
$ifaceHook = $iface->getHook($getCase);
echo count($iface->getHooks()) . ":";
echo ($iface->hasHook($getCase) ? "IG" : "ig") . ":";
echo ($iface->hasHook($setCase) ? "bad" : "is") . ":";
echo $ifaceHook->isAbstract() ? "IA" : "ia";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Get:get:2:Get:set:Set:T:H:G:S:2:$doubled::get:$doubled::set:EvalReflectHookedProperty:0:1:value:4:7:R:W:N:plain:0:2:AG:AS:$contract::get:A:$contract::set:A:1:IG:is:IA"
    );
}

/// Verifies eval ReflectionClass static-property APIs use current runtime values.
#[test]
fn test_eval_reflection_class_static_property_values() {
    let out = compile_and_run_capture(
        r#"<?php
	eval('class EvalReflectStaticBase {
    public static $base = "b";
    protected static $prot = "p";
    private static $shadow = "base-hidden";
    public $instance = "i";
}
class EvalReflectStaticChild extends EvalReflectStaticBase {
    public static $child = "c";
    private static $shadow = "child-hidden";
    public static int $count = 1;
}
EvalReflectStaticChild::$child = "mut";
$ref = new ReflectionClass("EvalReflectStaticChild");
$statics = $ref->getStaticProperties();
echo count($statics) . ":";
echo $statics["child"] . ":";
echo $statics["base"] . ":";
echo $statics["prot"] . ":";
echo $statics["shadow"] . ":";
echo $ref->getStaticPropertyValue("count") . ":";
$ref->setStaticPropertyValue("shadow", "changed");
echo $ref->getStaticPropertyValue("shadow") . ":";
$ref->setStaticPropertyValue(name: "count", value: 5);
echo EvalReflectStaticChild::$count . ":";
echo $ref->getStaticPropertyValue("instance", "fallback") . ":";
echo $ref->getStaticPropertyValue("missing", "fallback") . ":";
try {
    $ref->getStaticPropertyValue("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
try {
    $ref->setStaticPropertyValue("instance", "bad");
    echo "bad";
} catch (ReflectionException $e) {
    echo "S";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "5:mut:b:p:child-hidden:1:changed:5:fallback:fallback:E:S"
    );
}

/// Verifies eval ReflectionClass static-property APIs bridge generated/AOT values.
#[test]
fn test_eval_reflection_class_static_property_values_aot() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectStaticPropertyTarget {
    public static string $label = "start";
    public static int $count = 2;
    protected static string $secret = "prot";
    private static int $hidden = 4;
    public string $instance = "plain";
}
echo eval('$ref = new ReflectionClass("EvalAotReflectStaticPropertyTarget");
$statics = $ref->getStaticProperties();
echo count($statics) . ":";
echo $statics["label"] . ":";
echo $statics["count"] . ":";
echo $statics["secret"] . ":";
echo $statics["hidden"] . ":";
echo $ref->getStaticPropertyValue("label") . ":";
$ref->setStaticPropertyValue("label", "changed");
echo EvalAotReflectStaticPropertyTarget::$label . ":";
$ref->setStaticPropertyValue(name: "count", value: 9);
echo $ref->getStaticPropertyValue("count") . ":";
echo EvalAotReflectStaticPropertyTarget::$count . ":";
echo $ref->getStaticPropertyValue("secret") . ":";
$ref->setStaticPropertyValue("secret", "changed-prot");
echo $ref->getStaticPropertyValue("secret") . ":";
echo $ref->getStaticPropertyValue("hidden") . ":";
$ref->setStaticPropertyValue("hidden", 8);
echo $ref->getStaticPropertyValue("hidden") . ":";
echo $ref->getStaticPropertyValue("instance", "fallback") . ":";
try {
    $ref->setStaticPropertyValue("instance", "bad");
    echo "bad";
} catch (ReflectionException $e) {
    echo "S";
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "4:start:2:prot:4:start:changed:9:9:prot:changed-prot:4:8:fallback:S"
    );
}

/// Verifies eval ReflectionProperty value APIs bridge generated/AOT storage.
#[test]
fn test_eval_reflection_property_value_apis_bridge_aot_storage() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalAotReflectPropertyValueTarget {
    public string $name = "Ada";
    private int $secret = 3;
    public static string $label = "start";
    private static int $hidden = 4;

    public function reveal() {
        return $this->secret . "," . self::$hidden;
    }
}
$object = new EvalAotReflectPropertyValueTarget();
echo eval('$name = new ReflectionProperty("EvalAotReflectPropertyValueTarget", "name");
$secret = new ReflectionProperty("EvalAotReflectPropertyValueTarget", "secret");
$label = new ReflectionProperty("EvalAotReflectPropertyValueTarget", "label");
$hidden = new ReflectionProperty("EvalAotReflectPropertyValueTarget", "hidden");
echo $name->getValue($object) . ":";
$name->setValue($object, "Grace");
echo $object->name . ":";
echo $secret->getValue($object) . ":";
$secret->setValue($object, 9);
echo $object->reveal() . ":";
echo $label->getValue() . ":";
$label->setValue("changed");
echo EvalAotReflectPropertyValueTarget::$label . ":";
echo $hidden->getValue() . ":";
$hidden->setValue(8);
echo $object->reveal();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "Ada:Grace:3:9,4:start:changed:4:9,8");
}

/// Verifies eval ReflectionParameter exposes the declaring class for method parameters.
#[test]
fn test_eval_reflection_parameter_reports_declaring_class() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalDeclaringParamBase {
    public function inherited($base) {}
}
class EvalDeclaringParamChild extends EvalDeclaringParamBase {
    public function own($child) {}
}
$inherited = (new ReflectionMethod("EvalDeclaringParamChild", "inherited"))->getParameters()[0];
echo $inherited->getDeclaringClass()->getName() . ":";
echo $inherited->getDeclaringFunction()->getName() . ":";
echo $inherited->getDeclaringFunction()->getDeclaringClass()->getName() . ":";
$listed = (new ReflectionMethod("EvalDeclaringParamChild", "own"))->getParameters()[0];
echo $listed->getDeclaringClass()->getName() . ":";
echo $listed->getDeclaringFunction()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalDeclaringParamBase:inherited:EvalDeclaringParamBase:EvalDeclaringParamChild:own"
    );
}

/// Verifies eval ReflectionFunction materializes eval-declared function parameters.
#[test]
fn test_eval_reflection_function_reports_eval_function_parameters() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_free($left, $right) {
    return $left;
}
$ref = new ReflectionFunction("eval_reflect_free");
$params = $ref->getParameters();
echo $ref->getName() . ":";
echo $ref->getNumberOfParameters() . ":";
echo $ref->getNumberOfRequiredParameters() . ":";
echo count($params) . ":";
echo $params[0]->getName() . ":";
echo $params[1]->getPosition() . ":";
$declaring = $params[0]->getDeclaringFunction();
echo get_class($declaring) . ":";
echo $declaring->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "eval_reflect_free:2:2:2:left:1:ReflectionFunction:eval_reflect_free"
    );
}

/// Verifies eval ReflectionFunction preserves rich eval-declared function signatures.
#[test]
fn test_eval_reflection_function_reports_signature_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalFuncAttr {
    public $label;
    public function __construct($label) { $this->label = $label; }
    public function label() { return $this->label; }
}
#[EvalFuncAttr("free")]
function eval_reflect_rich(#[EvalFuncAttr("first")] string $name, int $count = 3, &...$items) {
    return $count;
}
$ref = new ReflectionFunction("eval_reflect_rich");
$attrs = $ref->getAttributes();
$params = $ref->getParameters();
echo count($attrs) . ":";
echo $attrs[0]->getName() . ":";
echo $attrs[0]->newInstance()->label() . ":";
echo $ref->getNumberOfParameters() . ":";
echo $ref->getNumberOfRequiredParameters() . ":";
echo ($params[0]->hasType() ? "T" : "t") . ":";
echo $params[0]->getType()->getName() . ":";
$paramAttrs = $params[0]->getAttributes();
echo count($paramAttrs) . ":";
echo $paramAttrs[0]->newInstance()->label() . ":";
echo ($params[1]->isOptional() ? "O" : "o") . ":";
echo $params[1]->getDefaultValue() . ":";
echo ($params[2]->isVariadic() ? "V" : "v") . ":";
echo $params[2]->isPassedByReference() ? "R" : "r";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:EvalFuncAttr:free:3:1:T:string:1:first:O:3:V:R"
    );
}

/// Verifies eval ReflectionFunction exposes eval-declared return type metadata.
#[test]
fn test_eval_reflection_function_reports_return_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_return_named(): ?int { return 1; }
function eval_reflect_return_union(): int|string { return 1; }
function eval_reflect_return_never(): never { throw new Exception("stop"); }
function eval_reflect_return_plain() {}
$namedRef = new ReflectionFunction("eval_reflect_return_named");
$named = $namedRef->getReturnType();
echo ($namedRef->hasReturnType() ? "T" : "t") . ":";
echo $named->getName() . ":";
echo ($named->allowsNull() ? "N" : "n") . ":";
echo ($named->isBuiltin() ? "B" : "b") . ":";
$union = (new ReflectionFunction("eval_reflect_return_union"))->getReturnType();
echo count($union->getTypes()) . ":";
foreach ($union->getTypes() as $type) {
    echo $type->getName();
    echo $type->isBuiltin() ? "B" : "b";
}
echo ":";
$never = (new ReflectionFunction("eval_reflect_return_never"))->getReturnType();
echo $never->getName() . ":";
echo ($never->allowsNull() ? "N" : "n") . ":";
echo ($never->isBuiltin() ? "B" : "b") . ":";
$plain = new ReflectionFunction("eval_reflect_return_plain");
echo ($plain->hasReturnType() ? "P" : "p") . ":";
echo $plain->getReturnType() === null ? "Q" : "q";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "T:int:N:B:2:intBstringB:never:n:B:p:Q");
}

/// Verifies eval ReflectionFunction and ReflectionMethod stringify retained signatures.
#[test]
fn test_eval_reflection_function_method_to_string() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_string_text(string $name, int $count = 3, &...$items): ?string {
    return $name;
}
class EvalReflectMethodStringTextTarget {
    final public static function run(?int $id, string $label = "ok"): ?string {
        return $label;
    }
}
$function = new ReflectionFunction("eval_reflect_string_text");
echo str_replace("\n", "|", $function->__toString());
echo "::";
$method = new ReflectionMethod("EvalReflectMethodStringTextTarget", "run");
echo str_replace("\n", "|", $method->__toString());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Function [ <user> function eval_reflect_string_text ] {|  - Parameters [3] {|    Parameter #0 [ <required> string $name ]|    Parameter #1 [ <optional> int $count = 3 ]|    Parameter #2 [ <optional> &...$items ]|  }|  - Return [ ?string ]|}|::Method [ <user> final static public method run ] {|  - Parameters [2] {|    Parameter #0 [ <required> ?int $id ]|    Parameter #1 [ <optional> string $label = 'ok' ]|  }|  - Return [ ?string ]|}|"
    );
}

/// Verifies eval ReflectionClass stringifies retained class metadata sections.
#[test]
fn test_eval_reflection_class_to_string() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectClassStringTarget {
    public const ANSWER = 42;
    public int $id = 7;
    public function read(string $name = "Ada"): ?string { return $name; }
}
echo str_replace("\n", "|", (new ReflectionClass("EvalReflectClassStringTarget"))->__toString());
echo "::";
echo str_replace("\n", "|", (new ReflectionObject(new EvalReflectClassStringTarget()))->__toString());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    let expected = "Class [ <user> class EvalReflectClassStringTarget ] {|  - Constants [1] {|    Constant [ public int ANSWER ] { 42 }|  }|  - Properties [1] {|    Property [ public int $id = 7 ]|  }|  - Methods [1] {|    Method [ <user> public method read ]|  }|}|";
    assert_eq!(out.stdout, format!("{expected}::{expected}"));
}

/// Verifies eval ReflectionObject lists dynamic public properties from its reflected instance.
#[test]
fn test_eval_reflection_object_lists_dynamic_properties() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectObjectDynamicTarget {
    public $declared = "declared";
}
$object = new EvalReflectObjectDynamicTarget();
$object->dynamic = "value";
$ref = new ReflectionObject($object);
$properties = $ref->getProperties();
foreach ($properties as $property) {
    echo $property->getName() . ":";
    echo ($property->isDynamic() ? "D" : "d") . "|";
}
echo ":";
$dynamic = $ref->getProperty("dynamic");
echo ($dynamic->isDynamic() ? "D" : "d") . ":";
echo $dynamic->getValue($object) . ":";
echo count($ref->getProperties(ReflectionProperty::IS_PUBLIC)) . ":";
echo count($ref->getProperties(ReflectionProperty::IS_STATIC)) . ":";
echo $ref->hasProperty("dynamic") ? "H" : "h";
echo $ref->hasProperty("declared") ? "D" : "d";
echo $ref->hasProperty("missing") ? "M" : "m";
echo (new ReflectionClass($object))->hasProperty("dynamic") ? "C" : "c";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "declared:d|dynamic:D|:D:value:2:0:HDmc");
}

/// Verifies eval ReflectionObject constructor type errors are catchable objects.
#[test]
fn test_eval_reflection_object_constructor_throws_type_errors() {
    let out = compile_and_run_capture(
        r#"<?php
eval('try {
    new ReflectionObject("EvalReflectObjectChild");
    echo "bad";
} catch (TypeError $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    new ReflectionObject([]);
    echo "bad";
} catch (TypeError $e) {
    echo $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "TypeError:ReflectionObject::__construct(): Argument #1 ($object) must be of type object, string given|ReflectionObject::__construct(): Argument #1 ($object) must be of type object, array given"
    );
}

/// Verifies eval Reflection origin metadata APIs are present on supported owners.
#[test]
fn test_eval_reflection_origin_metadata_defaults() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalReflectOriginTarget {
    public $id;
    public const ANSWER = 42;
    public function run() {}
}
function eval_reflect_origin_function() {}
enum EvalReflectOriginCase: string {
    case Ready = "ready";
}
$class = new ReflectionClass("EvalReflectOriginTarget");
$function = new ReflectionFunction("eval_reflect_origin_function");
$method = new ReflectionMethod("EvalReflectOriginTarget", "run");
$property = new ReflectionProperty("EvalReflectOriginTarget", "id");
$constant = new ReflectionClassConstant("EvalReflectOriginTarget", "ANSWER");
$unit = new ReflectionEnumUnitCase("EvalReflectOriginCase", "Ready");
$backed = new ReflectionEnumBackedCase("EvalReflectOriginCase", "Ready");
echo ($class->getDocComment() === false) ? "C" : "c"; echo ":";
echo ($function->getDocComment() === false) ? "F" : "f"; echo ":";
echo ($method->getDocComment() === false) ? "M" : "m"; echo ":";
echo ($property->getDocComment() === false) ? "P" : "p"; echo ":";
echo ($constant->getDocComment() === false) ? "K" : "k"; echo ":";
echo ($unit->getDocComment() === false) ? "U" : "u"; echo ":";
echo ($backed->getDocComment() === false) ? "B" : "b"; echo ":";
echo ($class->getExtensionName() === false) ? "E" : "e"; echo ":";
echo ($function->getExtensionName() === false) ? "N" : "n"; echo ":";
echo ($method->getExtensionName() === false) ? "O" : "o"; echo ":";
echo ($class->getExtension() === null) ? "X" : "x"; echo ":";
echo ($function->getExtension() === null) ? "Y" : "y"; echo ":";
echo ($method->getExtension() === null) ? "Z" : "z";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "C:F:M:P:K:U:B:E:N:O:X:Y:Z");
}

/// Verifies eval ReflectionFunction/Method expose name and origin predicate metadata.
#[test]
fn test_eval_reflection_function_and_method_name_origin_predicates() {
    let out = compile_and_run_capture(
        r#"<?php
eval('namespace EvalReflectNameNs;
function sample(...$items) {}
class Target {
    public function run(...$items) {}
    public static function stat() {}
}
$fn = new \ReflectionFunction("EvalReflectNameNs\\\\sample");
$method = new \ReflectionMethod(Target::class, "run");
echo $fn->getShortName() . ":";
echo $fn->getNamespaceName() . ":";
echo ($fn->inNamespace() ? "Y" : "N") . ":";
echo ($fn->isInternal() ? "I" : "i");
echo ($fn->isUserDefined() ? "U" : "u") . ":";
echo ($fn->isAnonymous() ? "A" : "a") . ":";
echo ($fn->isClosure() ? "C" : "c") . ":";
echo ($fn->isDeprecated() ? "D" : "d") . ":";
echo ($fn->isStatic() ? "S" : "s") . ":";
echo ($fn->returnsReference() ? "R" : "r") . ":";
echo ($fn->hasReturnType() ? "T" : "t") . ":";
echo ($fn->getReturnType() === null ? "N" : "n") . ":";
echo ($fn->isGenerator() ? "G" : "g") . ":";
echo ($fn->isVariadic() ? "V" : "v") . ":";
echo ($fn->hasTentativeReturnType() ? "H" : "h") . ":";
echo ($fn->getTentativeReturnType() === null ? "Q" : "q") . ":";
echo count($fn->getClosureUsedVariables()) . ":";
echo ($fn->getClosureThis() === null ? "T" : "t") . ":";
echo ($fn->getClosureScopeClass() === null ? "S" : "s") . ":";
echo ($fn->getClosureCalledClass() === null ? "L" : "l") . ":";
echo ($fn->isDisabled() ? "X" : "x") . "|";
echo $method->getShortName() . ":";
echo $method->getNamespaceName() . ":";
echo ($method->inNamespace() ? "Y" : "N") . ":";
echo ($method->isInternal() ? "I" : "i");
echo ($method->isUserDefined() ? "U" : "u") . ":";
echo ($method->isClosure() ? "C" : "c") . ":";
echo ($method->isDeprecated() ? "D" : "d") . ":";
echo ($method->isStatic() ? "S" : "s") . ":";
echo ($method->returnsReference() ? "R" : "r") . ":";
echo ($method->hasReturnType() ? "T" : "t") . ":";
echo ($method->getReturnType() === null ? "N" : "n") . ":";
echo ($method->isGenerator() ? "G" : "g") . ":";
echo ($method->isVariadic() ? "V" : "v") . ":";
echo ($method->hasTentativeReturnType() ? "H" : "h") . ":";
echo ($method->getTentativeReturnType() === null ? "Q" : "q") . ":";
echo count($method->getClosureUsedVariables()) . ":";
echo ($method->getClosureThis() === null ? "T" : "t") . ":";
echo ($method->getClosureScopeClass() === null ? "S" : "s") . ":";
echo ($method->getClosureCalledClass() === null ? "L" : "l") . ":";
$static = new \ReflectionMethod(Target::class, "stat");
echo ($static->isStatic() ? "S" : "s");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "sample:EvalReflectNameNs:Y:iU:a:c:d:s:r:t:N:g:V:h:Q:0:T:S:L:x|run::N:iU:c:d:s:r:t:N:g:V:h:Q:0:T:S:L:S"
    );
}

/// Verifies eval ReflectionFunction/Method derive deprecation predicates from `#[Deprecated]`.
#[test]
fn test_eval_reflection_function_and_method_deprecated_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('#[\Deprecated]
function eval_reflect_deprecated_fn() {}
function eval_reflect_plain_fn() {}
class EvalReflectDeprecatedMethodTarget {
    #[\Deprecated]
    public function old() {}
    public function fresh() {}
}
$deprecatedFn = new \ReflectionFunction("eval_reflect_deprecated_fn");
$plainFn = new \ReflectionFunction("eval_reflect_plain_fn");
$deprecatedMethod = new \ReflectionMethod(EvalReflectDeprecatedMethodTarget::class, "old");
$plainMethod = new \ReflectionMethod(EvalReflectDeprecatedMethodTarget::class, "fresh");
echo ($deprecatedFn->isDeprecated() ? "D" : "d") . ":";
echo ($plainFn->isDeprecated() ? "D" : "d") . ":";
echo ($deprecatedMethod->isDeprecated() ? "D" : "d") . ":";
echo ($plainMethod->isDeprecated() ? "D" : "d");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "D:d:D:d");
}

/// Verifies eval ReflectionFunction/Method expose static local variables through the bridge.
#[test]
fn test_eval_reflection_function_and_method_static_variables() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_static_fn() {
    static $count = 1;
    static $label = "fn";
    $count = $count + 1;
    return $count;
}
class EvalReflectStaticMethodBase {
    public function tick() {
        static $count = 3;
        static $label = "method";
        $count = $count + 1;
        return $count;
    }
}
class EvalReflectStaticMethodChild extends EvalReflectStaticMethodBase {}
$fn = new ReflectionFunction("eval_reflect_static_fn");
$beforeFn = $fn->getStaticVariables();
echo $beforeFn["count"] . ":" . $beforeFn["label"] . ":";
echo eval_reflect_static_fn() . ":";
$afterFn = $fn->getStaticVariables();
echo $afterFn["count"] . ":" . $afterFn["label"] . "|";
$object = new EvalReflectStaticMethodChild();
$method = new ReflectionMethod("EvalReflectStaticMethodChild", "tick");
$beforeMethod = $method->getStaticVariables();
echo $beforeMethod["count"] . ":" . $beforeMethod["label"] . ":";
echo $method->invoke($object) . ":";
$afterMethod = $method->getStaticVariables();
echo $afterMethod["count"] . ":" . $afterMethod["label"];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "1:fn:2:2:fn|3:method:4:4:method");
}

/// Verifies eval ReflectionMethod hasPrototype/getPrototype follow PHP inheritance rules.
#[test]
fn test_eval_reflection_method_reports_eval_prototypes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalProtoParentIface {
    public function parented();
}
interface EvalProtoChildIface extends EvalProtoParentIface {}
interface EvalProtoIface {
    public function iface();
}
class EvalProtoBase {
    public function run() {}
    public function inherited() {}
}
class EvalProtoChild extends EvalProtoBase implements EvalProtoIface, EvalProtoChildIface {
    public function run() {}
    public function iface() {}
    public function parented() {}
    public function own() {}
}
$override = new ReflectionMethod("EvalProtoChild", "run");
$overrideProto = $override->getPrototype();
echo ($override->hasPrototype() ? "Y" : "N") . ":";
echo $overrideProto->getDeclaringClass()->getName() . "::";
echo $overrideProto->getName() . ":";
$iface = new ReflectionMethod("EvalProtoChild", "iface");
$ifaceProto = $iface->getPrototype();
echo ($iface->hasPrototype() ? "Y" : "N") . ":";
echo $ifaceProto->getDeclaringClass()->getName() . "::";
echo $ifaceProto->getName() . ":";
$parentIface = new ReflectionMethod("EvalProtoChild", "parented");
$parentIfaceProto = $parentIface->getPrototype();
echo $parentIfaceProto->getDeclaringClass()->getName() . "::";
echo $parentIfaceProto->getName() . ":";
$own = new ReflectionMethod("EvalProtoChild", "own");
echo ($own->hasPrototype() ? "Y" : "N") . ":";
try {
    $own->getPrototype();
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
$inherited = new ReflectionMethod("EvalProtoChild", "inherited");
echo $inherited->hasPrototype() ? "Y" : "N";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Y:EvalProtoBase::run:Y:EvalProtoIface::iface:EvalProtoParentIface::parented:N:E:N"
    );
}

/// Verifies eval ReflectionMethod hasPrototype/getPrototype expose generated/AOT prototypes.
#[test]
fn test_eval_reflection_method_reports_aot_prototypes() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotProtoParentIface {
    public function parented();
}
interface EvalAotProtoChildIface extends EvalAotProtoParentIface {}
interface EvalAotProtoIface {
    public function iface();
    public static function staticIface();
}
class EvalAotProtoBase {
    public function run() {}
    public static function staticRun() {}
    public function inherited() {}
}
class EvalAotProtoChild extends EvalAotProtoBase implements EvalAotProtoIface, EvalAotProtoChildIface {
    public function run() {}
    public static function staticRun() {}
    public function iface() {}
    public static function staticIface() {}
    public function parented() {}
    public function own() {}
}
echo eval('try {
$override = new ReflectionMethod("EvalAotProtoChild", "run");
$overrideProto = $override->getPrototype();
echo ($override->hasPrototype() ? "Y" : "N") . ":";
echo $overrideProto->getDeclaringClass()->getName() . "::";
echo $overrideProto->getName() . ":";
$iface = new ReflectionMethod("EvalAotProtoChild", "iface");
$ifaceProto = $iface->getPrototype();
echo ($iface->hasPrototype() ? "Y" : "N") . ":";
echo $ifaceProto->getDeclaringClass()->getName() . "::";
echo $ifaceProto->getName() . ":";
$parentIface = new ReflectionMethod("EvalAotProtoChild", "parented");
$parentIfaceProto = $parentIface->getPrototype();
echo $parentIfaceProto->getDeclaringClass()->getName() . "::";
echo $parentIfaceProto->getName() . ":";
$staticOverride = new ReflectionMethod("EvalAotProtoChild", "staticRun");
$staticOverrideProto = $staticOverride->getPrototype();
echo ($staticOverride->hasPrototype() ? "Y" : "N") . ":";
echo $staticOverrideProto->getDeclaringClass()->getName() . "::";
echo $staticOverrideProto->getName() . ":";
$staticIface = new ReflectionMethod("EvalAotProtoChild", "staticIface");
$staticIfaceProto = $staticIface->getPrototype();
echo ($staticIface->hasPrototype() ? "Y" : "N") . ":";
echo $staticIfaceProto->getDeclaringClass()->getName() . "::";
echo $staticIfaceProto->getName() . ":";
$own = new ReflectionMethod("EvalAotProtoChild", "own");
echo ($own->hasPrototype() ? "Y" : "N") . ":";
try {
    $own->getPrototype();
} catch (ReflectionException $e) {
    echo "E";
}
echo ":";
$inherited = new ReflectionMethod("EvalAotProtoChild", "inherited");
echo $inherited->hasPrototype() ? "Y" : "N";
} catch (Throwable $e) {
    echo "ERR:" . get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Y:EvalAotProtoBase::run:Y:EvalAotProtoIface::iface:EvalAotProtoParentIface::parented:Y:EvalAotProtoBase::staticrun:Y:EvalAotProtoIface::staticiface:N:E:N"
    );
}

/// Verifies eval ReflectionMethod prototypes include inherited AOT interfaces.
#[test]
fn test_eval_reflection_method_reports_inherited_aot_interface_prototypes() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotProtoRootIface {
    public function inheritedIface(): string;
}
class EvalAotProtoParentWithIface implements EvalAotProtoRootIface {
    public function inheritedIface(): string { return "parent"; }
}
eval('interface EvalProtoLeafIface extends EvalAotProtoRootIface {}
class EvalProtoLeafImpl implements EvalProtoLeafIface {
    public function inheritedIface(): string { return "leaf"; }
}
class EvalProtoAotParentChild extends EvalAotProtoParentWithIface {
    public function inheritedIface(): string { return "child"; }
}
$leaf = new ReflectionMethod("EvalProtoLeafImpl", "inheritedIface");
$leafProto = $leaf->getPrototype();
echo ($leaf->hasPrototype() ? "L" : "l") . ":";
echo $leafProto->getDeclaringClass()->getName() . "::" . $leafProto->getName() . ":";
$child = new ReflectionMethod("EvalProtoAotParentChild", "inheritedIface");
$childProto = $child->getPrototype();
echo ($child->hasPrototype() ? "C" : "c") . ":";
echo $childProto->getDeclaringClass()->getName() . "::" . $childProto->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "L:EvalAotProtoRootIface::inheritediface:C:EvalAotProtoParentWithIface::inheritediface"
    );
}

/// Verifies eval ReflectionMethod prototypes preserve staticness for AOT targets.
#[test]
fn test_eval_reflection_method_reports_static_aot_prototypes() {
    let out = compile_and_run_capture(
        r#"<?php
interface EvalAotStaticProtoIface {
    public static function staticIface(): string;
}
class EvalAotStaticProtoParent {
    public static function staticRun(): string { return "parent"; }
}
eval('interface EvalStaticProtoLeafIface extends EvalAotStaticProtoIface {}
class EvalStaticProtoImpl extends EvalAotStaticProtoParent implements EvalStaticProtoLeafIface {
    public static function staticRun(): string { return "child"; }
    public static function staticIface(): string { return "iface"; }
}
$parent = new ReflectionMethod("EvalStaticProtoImpl", "staticRun");
$parentProto = $parent->getPrototype();
echo ($parent->hasPrototype() ? "P" : "p") . ":";
echo $parentProto->getDeclaringClass()->getName() . "::" . $parentProto->getName() . ":";
$iface = new ReflectionMethod("EvalStaticProtoImpl", "staticIface");
$ifaceProto = $iface->getPrototype();
echo ($iface->hasPrototype() ? "I" : "i") . ":";
echo $ifaceProto->getDeclaringClass()->getName() . "::" . $ifaceProto->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "P:EvalAotStaticProtoParent::staticrun:I:EvalAotStaticProtoIface::staticiface"
    );
}

/// Verifies eval-declared functions share method-style named/default/ref/variadic binding.
#[test]
fn test_eval_declared_function_rich_argument_binding() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_signature_call(string $name, &$value, int $count = 2, ...$rest) {
    $value = $value + $count;
    echo $name . ":";
    echo $count . ":";
    echo count($rest) . ":";
}
function eval_signature_array(string $name, int $count = 2, ...$rest) {
    echo $name . ":";
    echo $count . ":";
    echo count($rest) . ":";
    echo $rest["extra"];
}
$seed = 4;
eval_signature_call(name: "ok", value: $seed, extra: "z");
echo $seed . ":";
call_user_func_array("eval_signature_array", ["extra" => "z", "name" => "cb"]);');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "ok:2:1:6:cb:2:1:z");
}

/// Verifies eval ReflectionFunction::invoke and invokeArgs call eval-declared functions.
#[test]
fn test_eval_reflection_function_invoke_calls_eval_function() {
    let out = compile_and_run_capture(
        r#"<?php
eval('function eval_reflect_invoke($left = "A", $right = "B", ...$rest) {
    return $left . $right . count($rest) . $rest["extra"];
}
function eval_reflect_no_writeback(&$value) {
    $value = $value . "!";
    return $value;
}
$ref = new ReflectionFunction("eval_reflect_invoke");
echo $ref->invoke(right: "2", left: "1", extra: "X") . ":";
echo $ref->invokeArgs(["extra" => "Y", "left" => "3", "right" => "4"]) . ":";
$value = "Q";
$mutate = new ReflectionFunction("eval_reflect_no_writeback");
echo $mutate->invoke($value) . ":" . $value;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "121X:341Y:Q!:Q");
}

/// Verifies eval ReflectionFunction::invokeArgs accepts runtime-built AOT argument arrays.
#[test]
fn test_eval_reflection_function_invoke_args_accepts_runtime_aot_arg_arrays() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_aot_reflect_function_join(string $left, string $right = "B"): string {
    return $left . $right;
}
echo eval('$ref = new ReflectionFunction("eval_aot_reflect_function_join");
$args = [];
$args["right"] = "Y";
$args["left"] = "X";
echo $ref->invokeArgs($args) . ":";
return $ref->invoke(left: "Q");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "XY:QB");
}

/// Verifies eval ReflectionParameter exposes generated/AOT function defaults.
#[test]
fn test_eval_reflection_parameter_exposes_aot_function_defaults() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_aot_reflect_default_function(string $left, string $right = "B", $items = [1, 2]): string {
    return $left . $right . count($items);
}
echo eval('$ref = new ReflectionFunction("eval_aot_reflect_default_function");
$params = $ref->getParameters();
echo $params[0]->getName() . ":";
echo ($params[0]->isDefaultValueAvailable() ? "bad" : "required") . ":";
echo $params[1]->getName() . ":";
echo ($params[1]->isOptional() ? "O" : "r") . ":";
echo ($params[1]->isDefaultValueAvailable() ? "D" : "d") . ":";
echo $params[1]->getDefaultValue() . ":";
$direct = new ReflectionParameter("eval_aot_reflect_default_function", "items");
echo $direct->getName() . ":";
echo ($direct->isOptional() ? "O" : "r") . ":";
$default = $direct->getDefaultValue();
return count($default) . ":" . $default[0] . ":" . $default[1];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "left:required:right:O:D:B:items:O:2:1:2");
}

/// Verifies eval materializes generated/AOT constant-expression defaults.
#[test]
fn test_eval_aot_callable_constant_expression_defaults() {
    let out = compile_and_run_capture(
        r#"<?php
const EVAL_AOT_DEFAULT_GLOBAL_SUM = 9;
const EVAL_AOT_DEFAULT_GLOBAL_WORD = "G";

function eval_aot_default_expression_function(
    int $sum = 1 + 2,
    string $word = "A" . "B",
    bool $flag = !false,
    array $items = [2 * 3, "x" . "y", 1 + 1 => "two"],
    int $global = EVAL_AOT_DEFAULT_GLOBAL_SUM + 1,
    string $globalWord = EVAL_AOT_DEFAULT_GLOBAL_WORD . "H",
    int $pathFlag = PATHINFO_EXTENSION,
    ?string $maybe = null,
    float $scale = 1.5
): string {
    return $sum . ":" . $word . ":" . ($flag ? "T" : "F") . ":" .
        $items[0] . ":" . $items[1] . ":" . $items[2] . ":" .
        $global . ":" . $globalWord . ":" . $pathFlag . ":" .
        ($maybe === null ? "null" : "set") . ":" . ($scale > 1.0 ? "float" : "bad");
}

class EvalAotDefaultExpressionDep {
    public string $label;

    public function __construct(string $label = "base") {
        $this->label = $label;
    }
}

interface EvalAotDefaultExpressionIface {
    public const IFACE_TAG = "I";
}

class EvalAotDefaultExpressionBase {
    public const OFFSET = 4;
    public const TAG = "B";
}

class EvalAotDefaultExpressionTarget extends EvalAotDefaultExpressionBase implements EvalAotDefaultExpressionIface {
    public const LOCAL = 3;
    public const LABEL = "L";

    public string $label;

    public function __construct(
        string $prefix = EvalAotDefaultExpressionTarget::LABEL . EvalAotDefaultExpressionBase::TAG,
        int $count = EvalAotDefaultExpressionTarget::LOCAL + EvalAotDefaultExpressionBase::OFFSET
    ) {
        $this->label = $prefix . ":" . $count;
    }

    public function describe(
        EvalAotDefaultExpressionDep $dep = new EvalAotDefaultExpressionDep(EvalAotDefaultExpressionBase::TAG . "E"),
        array $items = [
            EvalAotDefaultExpressionTarget::LOCAL + EvalAotDefaultExpressionBase::OFFSET,
            EvalAotDefaultExpressionIface::IFACE_TAG
        ]
    ): string {
        return $dep->label . ":" . $items[0] . ":" . $items[1];
    }

    public function className(string $name = EvalAotDefaultExpressionTarget::class): string {
        return $name;
    }
}

echo eval('$target = new EvalAotDefaultExpressionTarget();
$ref = new ReflectionFunction("eval_aot_default_expression_function");
$items = $ref->getParameters()[3]->getDefaultValue();
return eval_aot_default_expression_function() . "|"
    . $target->describe() . "|"
    . $target->label . "|"
    . $target->className() . "|"
    . $items[0] . ":" . $items[2];');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "3:AB:T:6:xy:two:10:GH:4:null:float|BE:7:I|LB:7|EvalAotDefaultExpressionTarget|6:two"
    );
}

/// Verifies eval ReflectionParameter exposes generated/AOT function type flags.
#[test]
fn test_eval_reflection_parameter_exposes_aot_function_type_flags() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_aot_reflect_typed_function(int &$id, ?string $name, array ...$items): ?string {
    return $name;
}
echo eval('$ref = new ReflectionFunction("eval_aot_reflect_typed_function");
$params = $ref->getParameters();
foreach ($params as $param) {
    echo $param->getName() . ":";
    echo ($param->hasType() ? "T" : "t") . ":";
    $type = $param->getType();
    echo ($type ? $type->getName() : "none") . ":";
    echo ($type && $type->allowsNull() ? "N" : "n") . ":";
    echo ($param->isVariadic() ? "V" : "v") . ":";
    echo ($param->isPassedByReference() ? "R" : "r") . "|";
}
echo ":";
echo ($ref->hasReturnType() ? "R" : "r") . ":";
$return = $ref->getReturnType();
return $return->getName() . ":" . ($return->allowsNull() ? "N" : "n");');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "id:T:int:n:v:R|name:T:string:N:v:r|items:T:array:n:V:r|:R:string:N"
    );
}

/// Verifies eval ReflectionClass::isCloneable uses eval class metadata through the bridge.
#[test]
fn test_eval_reflection_class_cloneable_predicate() {
    let out = compile_and_run(
        r#"<?php
eval('abstract class EvalCloneAbstract {}
class EvalClonePlain {}
final class EvalCloneFinal {}
class EvalClonePrivate { private function __clone() {} }
class EvalCloneProtected { protected function __clone() {} }
class EvalClonePublic { public function __clone() {} }
interface EvalCloneIface {}
trait EvalCloneTrait {}
enum EvalCloneEnum { case Ready; }
echo (new ReflectionClass("EvalCloneAbstract"))->isCloneable() ? "A" : "a";
echo (new ReflectionClass("EvalClonePlain"))->isCloneable() ? "P" : "p";
echo (new ReflectionClass("EvalCloneFinal"))->isCloneable() ? "F" : "f";
echo (new ReflectionClass("EvalClonePrivate"))->isCloneable() ? "V" : "v";
echo (new ReflectionClass("EvalCloneProtected"))->isCloneable() ? "R" : "r";
echo (new ReflectionClass("EvalClonePublic"))->isCloneable() ? "U" : "u";
echo (new ReflectionClass("EvalCloneIface"))->isCloneable() ? "I" : "i";
echo (new ReflectionClass("EvalCloneTrait"))->isCloneable() ? "T" : "t";
echo (new ReflectionClass("EvalCloneEnum"))->isCloneable() ? "E" : "e";');
"#,
    );
    assert_eq!(out, "aPFvrUite");
}

/// Verifies eval `clone` shallow-copies eval-declared objects and runs `__clone()`.
#[test]
fn test_eval_clone_object_expression_runtime_and_hook() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalCloneRuntimeBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __clone() { $this->name = $this->name . ":clone"; }
}
$first = new EvalCloneRuntimeBox("A");
$second = clone $first;
echo $first->name; echo ":";
echo $second->name; echo ":";
$second->name = "B";
echo $first->name; echo ":";
echo $second->name;');
"#,
    );
    assert_eq!(out, "A:A:clone:A:B");
}

/// Verifies eval-declared `__destruct()` runs before final release of dynamic objects.
#[test]
fn test_eval_dynamic_object_runs_destructor_on_final_release() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalDestructRuntimeBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}
$box = new EvalDestructRuntimeBox("A");
unset($box);
new EvalDestructRuntimeBox("B");
echo "after";');
"#,
    );
    assert_eq!(out, "drop:A:drop:B:after");
}

/// Verifies eval-declared object destructors run when objects escape eval and native code releases them.
#[test]
fn test_eval_dynamic_object_runs_destructor_after_native_release() {
    let out = compile_and_run(
        r#"<?php
$box = eval('class EvalDestructEscapedBox {
    public string $name;
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}
return new EvalDestructEscapedBox("A");');
echo "before:";
unset($box);
echo "after";
"#,
    );
    assert_eq!(out, "before:drop:A:after");
}

/// Verifies eval-declared object destructors run when cycle collection releases them.
#[test]
fn test_eval_dynamic_object_runs_destructor_after_cycle_collection() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalCycleDropBox {
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}
$box = new EvalCycleDropBox("A");
$box->self = $box;
unset($box);
echo "after";');
"#,
    );
    assert_eq!(out, "drop:A:after");
}

/// Verifies eval-declared subclasses inherit generated/AOT destructors.
#[test]
fn test_eval_dynamic_subclass_runs_inherited_aot_destructor() {
    let out = compile_and_run(
        r#"<?php
class EvalDestructAotParent {
    public string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}

eval('class EvalDestructAotChild extends EvalDestructAotParent {}
$box = new EvalDestructAotChild("C");
echo "body:";
unset($box);
echo "after";');
"#,
    );
    assert_eq!(out, "body:drop:C:after");
}

/// Verifies eval `clone` shallow-copies ordinary emitted AOT objects.
#[test]
fn test_eval_clone_aot_object_expression() {
    let out = compile_and_run(
        r#"<?php
class EvalCloneAotBox {
    public string $name;
    public int $count;

    public function __construct(string $name, int $count) {
        $this->name = $name;
        $this->count = $count;
    }

    public function run(): void {
        eval('$copy = clone $this;
$copy->name = $copy->name . ":copy";
$copy->count = $copy->count + 10;
echo $this->name; echo ":";
echo $this->count; echo ":";
echo $copy->name; echo ":";
echo $copy->count; echo ":";
$plain = new stdClass();
$plain->name = "S";
$plainCopy = clone $plain;
$plainCopy->name = "S:copy";
echo $plain->name; echo ":";
echo $plainCopy->name;');
    }
}

(new EvalCloneAotBox("A", 2))->run();
"#,
    );
    assert_eq!(out, "A:2:A:copy:12:S:S:copy");
}

/// Verifies eval `clone` invokes public AOT `__clone()` hooks after storage copying.
#[test]
fn test_eval_clone_aot_object_runs_clone_hook() {
    let out = compile_and_run(
        r#"<?php
class EvalCloneAotHookBox {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    public function __clone(): void {
        $this->name = $this->name . ":hook";
    }

    public function run(): void {
        eval('$copy = clone $this;
echo $this->name; echo ":";
echo $copy->name;');
    }
}

(new EvalCloneAotHookBox("A"))->run();
"#,
    );
    assert_eq!(out, "A:A:hook");
}

/// Verifies eval `clone` invokes inherited AOT `__clone()` hooks for dynamic subclasses.
#[test]
fn test_eval_clone_dynamic_subclass_runs_inherited_aot_clone_hook() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalCloneAotInheritedHookParent {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    public function __clone(): void {
        $this->name = $this->name . ":aot";
    }
}
eval('class EvalCloneAotInheritedHookChild extends EvalCloneAotInheritedHookParent {}
$child = new EvalCloneAotInheritedHookChild("A");
$childCopy = clone $child;
echo $child->name . ":" . $childCopy->name;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "A:A:aot");
}

/// Verifies an eval `__clone()` override on an AOT-backed subclass owns clone-hook dispatch.
#[test]
fn test_eval_clone_dynamic_subclass_override_aot_clone_hook() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalCloneAotOverrideHookParent {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    public function __clone(): void {
        $this->name = $this->name . ":aot";
    }
}
eval('class EvalCloneAotOverrideHookChild extends EvalCloneAotOverrideHookParent {
    public function __clone(): void {
        $this->name = $this->name . ":eval";
    }
}
$child = new EvalCloneAotOverrideHookChild("B");
$childCopy = clone $child;
echo $child->name . ":" . $childCopy->name;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "B:B:eval");
}

/// Verifies eval `clone` invokes private AOT `__clone()` hooks from the declaring scope.
#[test]
fn test_eval_clone_aot_object_runs_private_clone_hook_in_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalCloneAotPrivateHookBox {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    private function __clone(): void {
        $this->name = $this->name . ":private";
    }

    public function run(): void {
        eval('$copy = clone $this;
echo $this->name; echo ":";
echo $copy->name;');
    }
}

(new EvalCloneAotPrivateHookBox("A"))->run();
"#,
    );
    assert_eq!(out, "A:A:private");
}

/// Verifies eval `clone` applies inherited private AOT clone visibility.
#[test]
fn test_eval_clone_dynamic_subclass_respects_private_aot_clone_hook() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalCloneAotInheritedPrivateParent {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    private function __clone(): void {
        $this->name = $this->name . ":private";
    }

    public function copyInParent(): void {
        eval('$copy = clone $this;
echo $copy->name;');
    }
}
eval('class EvalCloneAotInheritedPrivateChild extends EvalCloneAotInheritedPrivateParent {}
$child = new EvalCloneAotInheritedPrivateChild("A");
try {
    $copy = clone $child;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo ":";
$child->copyInParent();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Error:Call to private EvalCloneAotInheritedPrivateParent::__clone() from global scope:A:private"
    );
}

/// Verifies eval subclasses can invoke inherited protected AOT `__clone()` hooks in child scope.
#[test]
fn test_eval_clone_dynamic_subclass_runs_protected_aot_clone_hook_in_child_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalCloneAotInheritedProtectedParent {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    protected function __clone(): void {
        $this->name = $this->name . ":protected";
    }
}
eval('class EvalCloneAotInheritedProtectedChild extends EvalCloneAotInheritedProtectedParent {
    public function copySelf(): void {
        $copy = clone $this;
        echo $this->name . ":" . $copy->name;
    }
}
$child = new EvalCloneAotInheritedProtectedChild("B");
$child->copySelf();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "B:B:protected");
}

/// Verifies eval `clone` invokes protected AOT `__clone()` hooks from child scopes.
#[test]
fn test_eval_clone_aot_object_runs_protected_clone_hook_in_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalCloneAotProtectedHookBase {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    protected function __clone(): void {
        $this->name = $this->name . ":protected";
    }
}

class EvalCloneAotProtectedHookChild extends EvalCloneAotProtectedHookBase {
    public function run(): void {
        eval('$copy = clone $this;
echo $this->name; echo ":";
echo $copy->name;');
    }
}

(new EvalCloneAotProtectedHookChild("B"))->run();
"#,
    );
    assert_eq!(out, "B:B:protected");
}

/// Verifies eval `clone` rejects private AOT `__clone()` hooks outside allowed scopes.
#[test]
fn test_eval_clone_aot_object_rejects_private_clone_hook_outside_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalCloneAotPrivateOutsideBox {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    private function __clone(): void {
        $this->name = $this->name . ":private";
    }
}

$object = new EvalCloneAotPrivateOutsideBox("A");
eval('try {
    $copy = clone $object;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Error:Call to private EvalCloneAotPrivateOutsideBox::__clone() from global scope"
    );
}

/// Verifies eval method calls cannot directly invoke private AOT `__clone()` out of scope.
#[test]
fn test_eval_rejects_direct_private_aot_clone_method_call_outside_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalCloneAotPrivateDirectBox {
    public string $name = "A";

    private function __clone(): void {
        $this->name = "private";
    }
}

$object = new EvalCloneAotPrivateDirectBox();
eval('try {
    $object->__clone();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Error:Call to private method EvalCloneAotPrivateDirectBox::__clone() from global scope"
    );
}

/// Verifies eval ReflectionClass::isIterable reports eval and builtin class metadata.
#[test]
fn test_eval_reflection_class_iterable_predicate() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalIterablePlain {}
abstract class EvalIterableAbstract implements Iterator {}
interface EvalIterableIface extends Iterator {}
trait EvalIterableTrait {}
enum EvalIterableEnum { case Ready; }
class EvalIterableIterator implements Iterator {
    public function current(): mixed { return null; }
    public function key(): mixed { return null; }
    public function next(): void {}
    public function valid(): bool { return false; }
    public function rewind(): void {}
}
class EvalIterableAggregate implements IteratorAggregate {
    public function getIterator(): Traversable { return new ArrayIterator([]); }
}
echo (new ReflectionClass("EvalIterablePlain"))->isIterable() ? "P" : "p";
$iter = new ReflectionClass("EvalIterableIterator");
echo $iter->isIterable() ? "I" : "i";
echo $iter->isIterateable() ? "A" : "a";
echo (new ReflectionClass("EvalIterableAggregate"))->isIterable() ? "G" : "g";
echo (new ReflectionClass("EvalIterableAbstract"))->isIterable() ? "B" : "b";
echo (new ReflectionClass("EvalIterableIface"))->isIterable() ? "F" : "f";
echo (new ReflectionClass("Iterator"))->isIterable() ? "T" : "t";
echo (new ReflectionClass("ArrayIterator"))->isIterable() ? "R" : "r";
echo (new ReflectionClass("stdClass"))->isIterable() ? "S" : "s";
echo (new ReflectionClass("EvalIterableEnum"))->isIterable() ? "E" : "e";
echo (new ReflectionClass("EvalIterableTrait"))->isIterable() ? "H" : "h";');
"#,
    );
    assert_eq!(out, "pIAGbftRseh");
}

/// Verifies eval ReflectionClass::isIterable sees interfaces inherited from AOT parents.
#[test]
fn test_eval_reflection_class_iterable_inherits_aot_parent_interface() {
    let out = compile_and_run(
        r#"<?php
class EvalAotIterableParent implements IteratorAggregate {
    public function getIterator(): Traversable { return new ArrayIterator([]); }
}
eval('class EvalAotIterableChild extends EvalAotIterableParent {}
echo (new ReflectionClass("EvalAotIterableChild"))->isIterable() ? "R" : "r";
$box = new EvalAotIterableChild();
echo is_iterable($box) ? "I" : "i";');
"#,
    );
    assert_eq!(out, "RI");
}

/// Verifies eval ReflectionClass origin predicates distinguish eval symbols from built-ins.
#[test]
fn test_eval_reflection_class_internal_user_defined_predicates() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalOriginClass {}
interface EvalOriginIface {}
trait EvalOriginTrait {}
enum EvalOriginEnum { case Ready; }
function eval_reflect_origin($name) {
    $r = new ReflectionClass($name);
    echo $r->isInternal() ? "I" : "i";
    echo $r->isUserDefined() ? "U" : "u";
    echo ":";
}
eval_reflect_origin("EvalOriginClass");
eval_reflect_origin("EvalOriginIface");
eval_reflect_origin("EvalOriginTrait");
eval_reflect_origin("EvalOriginEnum");
eval_reflect_origin("stdClass");
eval_reflect_origin("ReflectionClass");
eval_reflect_origin("Iterator");');
"#,
    );
    assert_eq!(out, "iU:iU:iU:iU:Iu:Iu:Iu:");
}

/// Verifies eval ReflectionClass::newInstance constructs eval-declared classes.
#[test]
fn test_eval_reflection_class_new_instance_constructs_eval_class() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectNewTarget {
    public $label = "";
    public function __construct($left, $right) {
        $this->label = $left . $right;
    }
    public function label() {
        return $this->label;
    }
}
$ref = new ReflectionClass("EvalReflectNewTarget");
$first = $ref->newInstance("E", "F");
echo $first->label() . ":";
$second = $ref->newInstance(...["G", "H"]);
echo $second->label() . ":";
$third = $ref->newInstanceArgs(["right" => "J", "left" => "I"]);
echo $third->label() . ":";
$fourth = $ref->newInstanceArgs(["K", "L"]);
echo $fourth->label();');
"#,
    );
    assert_eq!(out, "EF:GH:IJ:KL");
}

/// Verifies eval ReflectionClass::newInstance rejects non-public eval constructors like PHP.
#[test]
fn test_eval_reflection_class_new_instance_rejects_non_public_eval_constructors() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectNewPrivateCtor {
    private function __construct() {}
}
class EvalReflectNewProtectedCtor {
    protected function __construct() {}
}
try {
    (new ReflectionClass("EvalReflectNewPrivateCtor"))->newInstance();
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    (new ReflectionClass("EvalReflectNewProtectedCtor"))->newInstance();
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "ReflectionException:Access to non-public constructor of class EvalReflectNewPrivateCtor|ReflectionException:Access to non-public constructor of class EvalReflectNewProtectedCtor"
    );
}

/// Verifies eval ReflectionClass::newInstance rejects non-public AOT constructors like PHP.
#[test]
fn test_eval_reflection_class_new_instance_rejects_protected_aot_constructor_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalReflectNewProtectedAotCtorBase {
    protected function __construct() {}
}

class EvalReflectNewProtectedAotCtorChild extends EvalReflectNewProtectedAotCtorBase {
    public static function run(): void {
        eval('try {
            $ref = new ReflectionClass("EvalReflectNewProtectedAotCtorBase");
            $ref->newInstance();
            echo "bad";
        } catch (ReflectionException $e) {
            echo get_class($e) . ":" . $e->getMessage();
        }');
    }
}

EvalReflectNewProtectedAotCtorChild::run();
"#,
    );
    assert_eq!(
        out,
        "ReflectionException:Access to non-public constructor of class EvalReflectNewProtectedAotCtorBase"
    );
}

/// Verifies eval ReflectionClass::newInstance constructs generated/AOT classes.
#[test]
fn test_eval_reflection_class_new_instance_constructs_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalReflectNewAotTarget {
    public string $label = "";

    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
}

echo eval('$ref = new ReflectionClass("EvalReflectNewAotTarget");
$first = $ref->newInstance("A");
echo $first->label . ":";
$second = $ref->newInstance(right: "Y", left: "X");
return $second->label;');
"#,
    );
    assert_eq!(out, "AB:XY");
}

/// Verifies eval ReflectionClass::newInstance rejects non-instantiable AOT class-likes.
#[test]
fn test_eval_reflection_class_new_instance_rejects_aot_non_instantiable_class_likes() {
    let cases = [
        (
            "abstract class EvalReflectNewAotAbstract {}",
            "EvalReflectNewAotAbstract",
            "Error:Cannot instantiate abstract class EvalReflectNewAotAbstract",
        ),
        (
            "interface EvalReflectNewAotIface {}",
            "EvalReflectNewAotIface",
            "Error:Cannot instantiate interface EvalReflectNewAotIface",
        ),
        (
            "trait EvalReflectNewAotTrait {}",
            "EvalReflectNewAotTrait",
            "Error:Cannot instantiate trait EvalReflectNewAotTrait",
        ),
        (
            "enum EvalReflectNewAotEnum { case Ready; }",
            "EvalReflectNewAotEnum",
            "Error:Cannot instantiate enum EvalReflectNewAotEnum",
        ),
    ];
    for (declaration, class_name, expected) in cases {
        let source = format!(
            r#"<?php
{declaration}
eval('try {{
    $ref = new ReflectionClass("{class_name}");
    $ref->newInstance();
    echo "bad";
}} catch (Error $e) {{
    echo get_class($e) . ":" . $e->getMessage();
}}');
"#
        );
        let out = compile_and_run(&source);
        assert_eq!(out, expected, "unexpected stdout for {class_name}");
    }
}

/// Verifies eval ReflectionClass instantiation rejects eval non-instantiable class-likes like PHP.
#[test]
fn test_eval_reflection_class_new_instance_rejects_eval_non_instantiable_class_likes() {
    let out = compile_and_run(
        r#"<?php
eval('abstract class EvalReflectNewAbstract {}
interface EvalReflectNewIface {}
trait EvalReflectNewTrait {}
enum EvalReflectNewEnum { case Ready; }
function eval_reflect_new_error($class, $without) {
    try {
        $ref = new ReflectionClass($class);
        if ($without) {
            $ref->newInstanceWithoutConstructor();
        } else {
            $ref->newInstance();
        }
        echo "bad";
    } catch (Error $e) {
        echo get_class($e) . ":" . $e->getMessage();
    }
}
eval_reflect_new_error("EvalReflectNewAbstract", false); echo "|";
eval_reflect_new_error("EvalReflectNewAbstract", true); echo "|";
eval_reflect_new_error("EvalReflectNewIface", false); echo "|";
eval_reflect_new_error("EvalReflectNewIface", true); echo "|";
eval_reflect_new_error("EvalReflectNewTrait", false); echo "|";
eval_reflect_new_error("EvalReflectNewTrait", true); echo "|";
eval_reflect_new_error("EvalReflectNewEnum", false); echo "|";
eval_reflect_new_error("EvalReflectNewEnum", true);');
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot instantiate abstract class EvalReflectNewAbstract|Error:Cannot instantiate abstract class EvalReflectNewAbstract|Error:Cannot instantiate interface EvalReflectNewIface|Error:Cannot instantiate interface EvalReflectNewIface|Error:Cannot instantiate trait EvalReflectNewTrait|Error:Cannot instantiate trait EvalReflectNewTrait|Error:Cannot instantiate enum EvalReflectNewEnum|Error:Cannot instantiate enum EvalReflectNewEnum"
    );
}

/// Verifies eval ReflectionMethod::invoke and invokeArgs call eval-declared methods.
#[test]
fn test_eval_reflection_method_invoke_calls_eval_method() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectInvokeBase {
    private function hidden($label = "H") {
        return "hidden:" . $label;
    }
    public function who() {
        return static::class;
    }
    public static function make($left, $right = "S") {
        return static::class . ":" . $left . $right;
    }
}
class EvalReflectInvokeChild extends EvalReflectInvokeBase {
    public function join($a, $b = "B") {
        return $a . $b;
    }
    public function mutate(&$value) {
        $value = $value . "!";
        return $value;
    }
}
$object = new EvalReflectInvokeChild();
$hidden = new ReflectionMethod("EvalReflectInvokeBase", "hidden");
echo $hidden->invoke($object, "X") . ":";
$who = (new ReflectionClass("EvalReflectInvokeChild"))->getMethod("who");
echo $who->invoke($object) . ":";
$static = new ReflectionMethod("EvalReflectInvokeBase", "make");
echo $static->invoke(null, right: "Y", left: "X") . ":";
echo $static->invoke($object, "A") . ":";
$join = null;
foreach ((new ReflectionClass("EvalReflectInvokeChild"))->getMethods() as $method) {
    if ($method->getName() === "join") {
        $join = $method;
    }
}
$value = "Q";
$mutate = new ReflectionMethod("EvalReflectInvokeChild", "mutate");
echo $join->invokeArgs($object, ["b" => "2", "a" => "1"]) . ":";
echo $mutate->invoke($object, $value) . ":" . $value;');
"#,
    );
    assert_eq!(
        out,
        "hidden:X:EvalReflectInvokeChild:EvalReflectInvokeBase:XY:EvalReflectInvokeBase:AS:12:Q!:Q"
    );
}

/// Verifies eval ReflectionMethod::invoke throws on incompatible receivers.
#[test]
fn test_eval_reflection_method_invoke_rejects_wrong_object() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectInvokeOwner {
    public function run() {
        return "owner";
    }
}
class EvalReflectInvokeOther {}
try {
    (new ReflectionMethod("EvalReflectInvokeOwner", "run"))->invoke(new EvalReflectInvokeOther());
    echo "bad";
} catch (ReflectionException $e) {
    echo "caught";
}');
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies eval ReflectionMethod/Property::setAccessible are PHP-compatible no-ops.
#[test]
fn test_eval_reflection_set_accessible_is_noop() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectAccessTarget {
    private $secret = "s";
    private function hidden() {
        return $this->secret;
    }
}
$object = new EvalReflectAccessTarget();
$method = new ReflectionMethod("EvalReflectAccessTarget", "hidden");
echo is_null($method->setAccessible(false)) ? "M" : "m"; echo ":";
echo $method->invoke($object); echo ":";
$property = new ReflectionProperty("EvalReflectAccessTarget", "secret");
echo is_null($property->setAccessible(accessible: true)) ? "P" : "p"; echo ":";
echo $property->getValue($object);');
"#,
    );
    assert_eq!(out, "M:s:P:s");
}

/// Verifies eval ReflectionClass::newInstanceWithoutConstructor allocates without constructors.
#[test]
fn test_eval_reflection_class_new_instance_without_constructor_allocates_eval_class() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalReflectNoCtorTarget {
    public $label = "default";
    private $secret = "hidden";
    public function __construct() {
        $this->label = "ctor";
    }
    public function label() {
        return $this->label;
    }
    public function secret() {
        return $this->secret;
    }
}
$ref = new ReflectionClass("EvalReflectNoCtorTarget");
$without = $ref->newInstanceWithoutConstructor();
echo $without->label() . ":";
echo $without->secret() . ":";
$with = $ref->newInstance();
echo $with->label();');
"#,
    );
    assert_eq!(out, "default:hidden:ctor");
}

/// Verifies eval ReflectionClass::newInstanceWithoutConstructor allocates generated/AOT classes.
#[test]
fn test_eval_reflection_class_new_instance_without_constructor_allocates_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalReflectNoCtorAotTarget {
    public int $value = 4;

    private function __construct() {
        $this->value = 9;
    }
}

echo eval('$ref = new ReflectionClass("EvalReflectNoCtorAotTarget");
$object = $ref->newInstanceWithoutConstructor();
echo $object->value . ":";
echo $ref->isInstantiable() ? "I" : "i";');
"#,
    );
    assert_eq!(out, "4:i");
}

/// Verifies eval ReflectionClass::newInstanceWithoutConstructor rejects non-allocatable AOT class-likes.
#[test]
fn test_eval_reflection_class_new_instance_without_constructor_rejects_aot_non_classes() {
    let cases = [
        (
            "abstract class EvalReflectNoCtorAotAbstract {}",
            "EvalReflectNoCtorAotAbstract",
            "Error:Cannot instantiate abstract class EvalReflectNoCtorAotAbstract",
        ),
        (
            "interface EvalReflectNoCtorAotIface {}",
            "EvalReflectNoCtorAotIface",
            "Error:Cannot instantiate interface EvalReflectNoCtorAotIface",
        ),
        (
            "trait EvalReflectNoCtorAotTrait {}",
            "EvalReflectNoCtorAotTrait",
            "Error:Cannot instantiate trait EvalReflectNoCtorAotTrait",
        ),
        (
            "enum EvalReflectNoCtorAotEnum { case Ready; }",
            "EvalReflectNoCtorAotEnum",
            "Error:Cannot instantiate enum EvalReflectNoCtorAotEnum",
        ),
    ];
    for (declaration, class_name, expected) in cases {
        let source = format!(
            r#"<?php
{declaration}
eval('try {{
    $ref = new ReflectionClass("{class_name}");
    $ref->newInstanceWithoutConstructor();
    echo "bad";
}} catch (Error $e) {{
    echo get_class($e) . ":" . $e->getMessage();
}}');
"#
        );
        let out = compile_and_run(&source);
        assert_eq!(out, expected, "unexpected stdout for {class_name}");
    }
}

/// Verifies eval ReflectionClassConstant/EnumCase expose eval-declared attributes.
#[test]
fn test_eval_reflection_constant_and_enum_case_attributes() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstMarker {
    public $name;
    public function __construct($name) {
        $this->name = $name;
    }
    public function label() {
        return $this->name;
    }
}
class EvalConstReflectTarget {
    #[EvalConstMarker("const")]
    public const ANSWER = 42;
}
enum EvalCaseReflectTarget: string {
    #[EvalConstMarker("case")]
    case Ready = "ready";
}
$constAttrs = (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getAttributes();
echo count($constAttrs) . ":" . (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->getName() . ":";
echo (new ReflectionClassConstant("EvalConstReflectTarget", "ANSWER"))->isEnumCase() ? "enum" : "plain"; echo ":";
echo $constAttrs[0]->getName() . ":" . $constAttrs[0]->getArguments()[0] . ":";
echo $constAttrs[0]->newInstance()->label() . ":";
$caseAttrs = (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo count($caseAttrs) . ":" . (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->getName() . ":";
echo (new ReflectionClassConstant("EvalCaseReflectTarget", "Ready"))->isEnumCase() ? "enum" : "plain"; echo ":";
echo $caseAttrs[0]->getName() . ":" . $caseAttrs[0]->getArguments()[0] . ":";
$unitAttrs = (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getName() . ":";
echo ((new ReflectionEnumUnitCase("EvalCaseReflectTarget", "Ready"))->getValue() === EvalCaseReflectTarget::Ready) ? "unit" : "bad"; echo ":";
echo $unitAttrs[0]->newInstance()->label() . ":";
$backedAttrs = (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getAttributes();
echo (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getName() . ":";
echo ((new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getValue() === EvalCaseReflectTarget::Ready) ? "backed" : "bad"; echo ":";
echo (new ReflectionEnumBackedCase("EvalCaseReflectTarget", "Ready"))->getBackingValue() . ":";
echo $backedAttrs[0]->newInstance()->label();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "1:ANSWER:plain:EvalConstMarker:const:const:1:Ready:enum:EvalConstMarker:case:Ready:unit:case:Ready:backed:ready:case"
    );
}

/// Verifies eval ReflectionClassConstant/EnumCase expose PHP's untyped metadata defaults.
#[test]
fn test_eval_reflection_constant_type_metadata_defaults() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstTypeTarget {
    public const ANSWER = 42;
}
enum EvalConstTypeEnum: string {
    case Ready = "ready";
}
$constant = new ReflectionClassConstant("EvalConstTypeTarget", "ANSWER");
echo $constant->isDeprecated() ? "D" : "d"; echo ":";
echo $constant->hasType() ? "T" : "t"; echo ":";
echo $constant->getType() === null ? "N" : "n"; echo ":";
$case = new ReflectionClassConstant("EvalConstTypeEnum", "Ready");
echo $case->isDeprecated() ? "D" : "d"; echo ":";
echo $case->hasType() ? "T" : "t"; echo ":";
echo $case->getType() === null ? "N" : "n"; echo ":";
$unit = new ReflectionEnumUnitCase("EvalConstTypeEnum", "Ready");
echo $unit->isDeprecated() ? "D" : "d"; echo ":";
echo $unit->hasType() ? "T" : "t"; echo ":";
echo $unit->getType() === null ? "N" : "n"; echo ":";
$backed = new ReflectionEnumBackedCase("EvalConstTypeEnum", "Ready");
echo $backed->isDeprecated() ? "D" : "d"; echo ":";
echo $backed->hasType() ? "T" : "t"; echo ":";
echo $backed->getType() === null ? "N" : "n";');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "d:t:N:d:t:N:d:t:N:d:t:N");
}

/// Verifies eval ReflectionClassConstant/EnumCase stringify retained metadata.
#[test]
fn test_eval_reflection_constant_to_string() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstStringTarget {
    public const ANSWER = 42;
    final protected const LIMIT = 7;
    private const FLAG = true;
    public const LABEL = "ok";
    public const NOTHING = null;
}
enum EvalConstStringEnum: string {
    case Ready = "ready";
}
foreach (["ANSWER", "LIMIT", "FLAG", "LABEL", "NOTHING"] as $name) {
    echo str_replace("\n", "\\n", (new ReflectionClassConstant("EvalConstStringTarget", $name))->__toString());
    echo "|";
}
echo str_replace("\n", "\\n", (new ReflectionClassConstant("EvalConstStringEnum", "Ready"))->__toString());
echo "|";
echo str_replace("\n", "\\n", (new ReflectionEnumUnitCase("EvalConstStringEnum", "Ready"))->__toString());
echo "|";
echo str_replace("\n", "\\n", (new ReflectionEnumBackedCase("EvalConstStringEnum", "Ready"))->__toString());');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "Constant [ public int ANSWER ] { 42 }\n|Constant [ final protected int LIMIT ] { 7 }\n|Constant [ private bool FLAG ] { 1 }\n|Constant [ public string LABEL ] { ok }\n|Constant [ public null NOTHING ] {  }\n|Constant [ public EvalConstStringEnum Ready ] { Object }\n|Constant [ public EvalConstStringEnum Ready ] { Object }\n|Constant [ public EvalConstStringEnum Ready ] { Object }\n"
    );
}

/// Verifies eval ReflectionClassConstant exposes visibility predicates and modifiers.
#[test]
fn test_eval_reflection_class_constant_visibility_and_modifiers() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalConstVisibilityTarget {
    private const SECRET = 1;
    protected const LIMIT = 2;
    final public const ANSWER = 3;
}
enum EvalConstVisibilityEnum {
    case Ready;
}
$secret = new ReflectionClassConstant("EvalConstVisibilityTarget", "SECRET");
echo "SECRET:";
echo $secret->isPrivate() ? "R" : "r";
echo $secret->isProtected() ? "P" : "p";
echo $secret->isPublic() ? "U" : "u";
echo $secret->isFinal() ? "F" : "f";
echo ":" . $secret->getModifiers() . "\n";
$limit = new ReflectionClassConstant("EvalConstVisibilityTarget", "LIMIT");
echo "LIMIT:";
echo $limit->isPrivate() ? "R" : "r";
echo $limit->isProtected() ? "P" : "p";
echo $limit->isPublic() ? "U" : "u";
echo $limit->isFinal() ? "F" : "f";
echo ":" . $limit->getModifiers() . "\n";
$answer = new ReflectionClassConstant("EvalConstVisibilityTarget", "ANSWER");
echo "ANSWER:";
echo $answer->isPrivate() ? "R" : "r";
echo $answer->isProtected() ? "P" : "p";
echo $answer->isPublic() ? "U" : "u";
echo $answer->isFinal() ? "F" : "f";
echo ":" . $answer->getModifiers() . "\n";
$case = new ReflectionClassConstant("EvalConstVisibilityEnum", "Ready");
echo "Ready:";
echo $case->isPrivate() ? "R" : "r";
echo $case->isProtected() ? "P" : "p";
echo $case->isPublic() ? "U" : "u";
echo $case->isFinal() ? "F" : "f";
echo ":" . $case->getModifiers() . "\n";
echo "VALUES:" . $secret->getValue() . ":" . $limit->getValue() . ":" . $answer->getValue() . ":";
echo $case->getValue() === EvalConstVisibilityEnum::Ready ? "E" : "e";
echo "\n";
foreach ((new ReflectionClass("EvalConstVisibilityTarget"))->getReflectionConstants() as $constant) {
    if ($constant->getName() === "ANSWER") {
        echo "LIST:" . $constant->getValue() . "\n";
    }
}');
echo ReflectionClassConstant::IS_PUBLIC . ":";
echo ReflectionClassConstant::IS_PROTECTED . ":";
echo ReflectionClassConstant::IS_PRIVATE . ":";
echo ReflectionClassConstant::IS_FINAL;
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "SECRET:Rpuf:4\nLIMIT:rPuf:2\nANSWER:rpUF:33\nReady:rpUf:1\nVALUES:1:2:3:E\nLIST:3\n1:2:4:32"
    );
}

/// Verifies eval and AOT enum-case reflectors expose inherited constant predicates.
#[test]
fn test_eval_reflection_enum_case_visibility_and_modifiers() {
    let out = compile_and_run_capture(
        r#"<?php
enum EvalAotEnumCaseVisibility: string {
    case Ready = "ready";
}
$unit = new ReflectionEnumUnitCase("EvalAotEnumCaseVisibility", "Ready");
$backed = new ReflectionEnumBackedCase("EvalAotEnumCaseVisibility", "Ready");
echo "AOT:";
foreach ([$unit, $backed] as $case) {
    echo $case->isEnumCase() ? "E" : "e";
    echo $case->isPrivate() ? "R" : "r";
    echo $case->isProtected() ? "P" : "p";
    echo $case->isPublic() ? "U" : "u";
    echo $case->isFinal() ? "F" : "f";
    echo $case->getModifiers() . ":";
}
eval('enum EvalEnumCaseVisibility: string {
    case Ready = "ready";
}
$unit = new ReflectionEnumUnitCase("EvalEnumCaseVisibility", "Ready");
$backed = new ReflectionEnumBackedCase("EvalEnumCaseVisibility", "Ready");
echo "\nEVAL:";
foreach ([$unit, $backed] as $case) {
    echo $case->isEnumCase() ? "E" : "e";
    echo $case->isPrivate() ? "R" : "r";
    echo $case->isProtected() ? "P" : "p";
    echo $case->isPublic() ? "U" : "u";
    echo $case->isFinal() ? "F" : "f";
    echo $case->getModifiers() . ":";
}
echo ReflectionEnumUnitCase::IS_PUBLIC . ":";
echo ReflectionEnumUnitCase::IS_PROTECTED . ":";
echo ReflectionEnumUnitCase::IS_PRIVATE . ":";
echo ReflectionEnumUnitCase::IS_FINAL . ":";
echo ReflectionEnumBackedCase::IS_PUBLIC . ":";
echo ReflectionEnumBackedCase::IS_PROTECTED . ":";
echo ReflectionEnumBackedCase::IS_PRIVATE . ":";
echo ReflectionEnumBackedCase::IS_FINAL;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "AOT:ErpUf1:ErpUf1:\nEVAL:ErpUf1:ErpUf1:1:2:4:32:1:2:4:32"
    );
}

/// Verifies ReflectionEnum methods work for enums declared inside eval.
#[test]
fn test_eval_reflection_enum_owner_methods() {
    let out = compile_and_run_capture(
        r#"<?php
eval('enum EvalBridgePure {
    case Ready;
    case Done;
}
enum EvalBridgeBacked: string {
    case Ready = "ready";
    case Done = "done";
}
$pure = new ReflectionEnum("EvalBridgePure");
echo $pure->getName() . ":";
echo ($pure->isBacked() ? "B" : "b") . ":";
echo ($pure->getBackingType() === null ? "N" : "n") . ":";
echo ($pure->hasCase("Ready") ? "R" : "r");
echo ($pure->hasCase("Missing") ? "M" : "m") . ":";
$case = $pure->getCase("Done");
echo $case->getName() . ":";
echo $case->getEnum()->getName() . ":";
$cases = $pure->getCases();
echo count($cases) . ":";
echo $cases[0]->getName() . ":";
echo $cases[1]->getEnum()->getName() . ":";
$backed = new ReflectionEnum("EvalBridgeBacked");
$type = $backed->getBackingType();
echo ($backed->isBacked() ? "B" : "b") . ":";
echo $type->getName() . ":";
echo ($type->isBuiltin() ? "I" : "i") . ":";
$backedCase = $backed->getCase("Ready");
echo $backedCase->getName() . ":";
echo $backedCase->getBackingValue() . ":";
echo ($backedCase->getEnum()->isBacked() ? "E" : "e") . ":";
$backedCases = $backed->getCases();
echo count($backedCases) . ":";
echo $backedCases[1]->getBackingValue() . ":";
echo $backedCases[0]->getEnum()->getBackingType()->getName();');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "EvalBridgePure:b:N:Rm:Done:EvalBridgePure:2:Ready:EvalBridgePure:B:string:I:Ready:ready:E:2:done:string"
    );
}

/// Verifies eval ReflectionEnum construction errors are catchable objects.
#[test]
fn test_eval_reflection_enum_constructor_throws_reflection_exceptions() {
    let out = compile_and_run_capture(
        r#"<?php
eval('class EvalDynReflectNotEnumClass {}
interface EvalDynReflectNotEnumIface {}
trait EvalDynReflectNotEnumTrait {}
enum EvalDynReflectActualEnum {
    case Ready;
}
try {
    new ReflectionEnum("EvalDynReflectNotEnumClass");
    echo "bad";
} catch (ReflectionException $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    new ReflectionEnum("EvalDynReflectNotEnumIface");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnum("EvalDynReflectNotEnumTrait");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
try {
    new ReflectionEnum("EvalDynReflectMissingEnum");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo "|";
echo (new ReflectionEnum("EvalDynReflectActualEnum"))->getName();
');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ReflectionException:Class \"EvalDynReflectNotEnumClass\" is not an enum|Class \"EvalDynReflectNotEnumIface\" is not an enum|Class \"EvalDynReflectNotEnumTrait\" is not an enum|Class \"EvalDynReflectMissingEnum\" does not exist|EvalDynReflectActualEnum"
    );
}

/// Verifies eval interface and trait constants work through the bridge.
#[test]
fn test_eval_declared_interface_and_trait_constants() {
    let out = compile_and_run_capture(
        r#"<?php
eval('interface EvalConstParentIface {
    public const BASE = 2;
}
interface EvalConstChildIface extends EvalConstParentIface {
    public const LOCAL = 3;
}
trait EvalConstReusableTrait {
    public const SEED = 6;
    public static function readTraitSeed() {
        return self::SEED;
    }
}
trait EvalConstDuplicateA {
    public const DUP = 9;
}
trait EvalConstDuplicateB {
    public const DUP = 9;
}
class EvalConstIfaceTraitBox implements EvalConstChildIface {
    use EvalConstReusableTrait;
}
class EvalConstDuplicateTraitBox {
    use EvalConstDuplicateA, EvalConstDuplicateB;
}
class EvalConstDuplicateClassBox {
    use EvalConstDuplicateA;
    public const DUP = 9;
}
echo EvalConstParentIface::BASE . ":";
echo EvalConstChildIface::BASE . ":";
echo EvalConstIfaceTraitBox::BASE . ":";
echo EvalConstIfaceTraitBox::LOCAL . ":";
echo EvalConstReusableTrait::SEED . ":";
echo EvalConstIfaceTraitBox::SEED . ":";
echo EvalConstIfaceTraitBox::readTraitSeed() . ":";
echo EvalConstDuplicateTraitBox::DUP . ":";
echo EvalConstDuplicateClassBox::DUP;');
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:2:2:3:6:6:6:9:9");
}

/// Verifies eval-declared trait constant conflicts follow PHP compatibility rules.
#[test]
fn test_eval_declared_trait_constant_conflict_rules() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('trait EvalTraitConstBad {
    public const SEED = 6;
}
class EvalTraitConstBadBox {
    use EvalTraitConstBad;
    public const SEED = 7;
}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval throws Error for private member access from outside the declaring class.
#[test]
fn test_eval_declared_private_member_access_throws_error() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalPrivateAccessBox {
    private int $secret = 4;
}
$box = new EvalPrivateAccessBox();
try {
    echo $box->secret;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot access private property EvalPrivateAccessBox::$secret"
    );
}

/// Verifies eval throws Error for inaccessible eval-declared method calls.
#[test]
fn test_eval_declared_inaccessible_method_access_throws_error() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalPrivateMethodAccessBox {
    private function hidden() { return 4; }
    protected static function seed() { return 5; }
}
$box = new EvalPrivateMethodAccessBox();
try {
    echo $box->hidden();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}
echo "|";
try {
    echo EvalPrivateMethodAccessBox::seed();
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Call to private method EvalPrivateMethodAccessBox::hidden() from global scope|Error:Call to protected method EvalPrivateMethodAccessBox::seed() from global scope"
    );
}

/// Verifies eval throws Error for protected class constant access from outside the declaring class.
#[test]
fn test_eval_declared_protected_class_constant_access_throws_error() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalProtectedConstAccessBox {
    protected const SECRET = 4;
}
try {
    echo EvalProtectedConstAccessBox::SECRET;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot access protected constant EvalProtectedConstAccessBox::SECRET"
    );
}

/// Verifies eval throws Error for private static member access from outside the declaring class.
#[test]
fn test_eval_declared_private_static_member_access_throws_error() {
    let out = compile_and_run(
        r#"<?php
eval('class EvalPrivateStaticAccessBox {
    private static int $secret = 4;
}
try {
    echo EvalPrivateStaticAccessBox::$secret;
    echo "bad";
} catch (Error $e) {
    echo get_class($e) . ":" . $e->getMessage();
}');
"#,
    );
    assert_eq!(
        out,
        "Error:Cannot access private property EvalPrivateStaticAccessBox::$secret"
    );
}

/// Verifies duplicate eval-declared functions fail through the runtime bridge.
#[test]
fn test_eval_duplicate_declared_function_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('function dyn_eval_dup() { return 1; }');
eval('function dyn_eval_dup() { return 2; }');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared empty classes are registered for later class probes.
#[test]
fn test_eval_declared_empty_class_is_visible_to_class_exists() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalClassExists {}');
echo eval('return class_exists("DynEvalClassExists") ? "Y" : "N";');
echo eval('return class_exists("dynevalclassexists") ? "Y" : "N";');
"#,
    );
    assert_eq!(out, "YY");
}

/// Verifies native `class_exists()` probes can see eval-declared classes after the barrier.
#[test]
fn test_eval_declared_empty_class_is_visible_to_native_class_exists_after_barrier() {
    let out = compile_and_run(
        r#"<?php
echo class_exists("DynEvalNativeClassExists") ? "bad" : "N";
eval('class DynEvalNativeClassExists {}');
echo class_exists("DynEvalNativeClassExists") ? "Y" : "N";
echo class_exists("dynevalnativeclassexists") ? "Y" : "N";
echo class_exists("\DynEvalNativeClassExists", false) ? "Y" : "N";
echo class_exists("MissingDynEvalNativeClassExists") ? "bad" : "N";
"#,
    );
    assert_eq!(out, "NYYYN");
}

/// Verifies post-eval native class probes keep AOT class results static.
#[test]
fn test_eval_barrier_keeps_native_class_exists_for_aot_classes() {
    let out = compile_and_run(
        r#"<?php
class EvalNativeClassExistsAot {}
eval('');
echo class_exists("evalnativeclassexistsaot") ? "Y" : "N";
"#,
    );
    assert_eq!(out, "Y");
}

/// Verifies duplicate eval-declared classes fail through the runtime bridge.
#[test]
fn test_eval_duplicate_declared_class_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class DynEvalClassDup {}');
eval('class dynevalclassdup {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval class declarations cannot redeclare an AOT class name.
#[test]
fn test_eval_declared_class_duplicate_aot_class_fails() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class DynEvalAotClassDup {}
eval('class dynevalaotclassdup {}');
"#,
    );
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval-declared classes support public properties, constructors, and methods.
#[test]
fn test_eval_declared_class_constructs_object_with_method() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalSupported {
    public int $x = 1;
    public function __construct($x) { $this->x = $x; }
    public function bump($n) { $this->x = $this->x + $n; return $this->x; }
}');
echo eval('$box = new DynEvalSupported(5);
echo get_class($box) . ":";
echo $box->bump(4) . ":";
echo is_a($box, "DynEvalSupported") ? "Y" : "N";
$call = [$box, "bump"];
echo call_user_func($call, 1) . ":";
echo call_user_func_array($call, [2]) . ":";
return $box->x;');
"#,
    );
    assert_eq!(out, "DynEvalSupported:9:Y10:12:12");
}

/// Verifies eval-declared methods support PHP static syntax for `$this` instance calls.
#[test]
fn test_eval_declared_class_static_syntax_calls_instance_methods() {
    let out = compile_and_run(
        r#"<?php
echo eval('class DynEvalStaticSyntaxBase {
    protected function label() { return "base"; }
}
class DynEvalStaticSyntaxChild extends DynEvalStaticSyntaxBase {
    protected function own() { return "child"; }
    public function run() {
        return parent::label() . ":" . self::own();
    }
}
$box = new DynEvalStaticSyntaxChild();
return $box->run();');
"#,
    );
    assert_eq!(out, "base:child");
}

/// Verifies native object construction can use eval-declared classes after the barrier.
#[test]
fn test_eval_declared_class_constructs_object_natively_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeSupported {
    public int $x = 1;
    public function __construct($x) { $this->x = $x; }
    public function bump($n) { $this->x = $this->x + $n; return $this->x; }
}');
$box = new DynEvalNativeSupported(5);
echo $box->bump(4) . ":";
$call = [$box, "bump"];
echo call_user_func($call, 1) . ":";
echo call_user_func_array($call, [2]) . ":";
echo $box->x;
"#,
    );
    assert_eq!(out, "9:10:12:12");
}

/// Verifies native dynamic `new $class` can instantiate eval-declared classes after the barrier.
#[test]
fn test_eval_declared_class_dynamic_new_natively_after_barrier() {
    let out = compile_and_run(
        r#"<?php
class DynEvalNativeDynamicNewAot {
    public int $label;
    public function __construct($label) { $this->label = $label; }
    public function read() { return $this->label; }
}
eval('class DynEvalNativeDynamicNew {
    public int $label;
    public function __construct($label) { $this->label = $label; }
    public function read() { return $this->label; }
}');
$class = "DynEvalNativeDynamicNew";
$box = new $class(5);
echo $box->read() . ":";
$aotClass = "DynEvalNativeDynamicNewAot";
$aotBox = new $aotClass(7);
echo $aotBox->read();
"#,
    );
    assert_eq!(out, "5:7");
}

/// Verifies native property writes can update eval-created objects after the barrier.
#[test]
fn test_eval_declared_class_native_property_write_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativePropertyWrite {
    public int $x = 1;
    public string $label = "old";
}');
$box = new DynEvalNativePropertyWrite();
$box->x = 8;
$box->label = "new";
echo $box->label . ":";
echo $box->x;
"#,
    );
    assert_eq!(out, "new:8");
}

/// Verifies native introspection sees eval-declared object metadata after the barrier.
#[test]
fn test_eval_declared_class_native_introspection_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeIntrospectBase {}
class DynEvalNativeIntrospectChild extends DynEvalNativeIntrospectBase {}');
$box = new DynEvalNativeIntrospectChild();
echo get_class($box) . ":";
echo get_parent_class($box) . ":";
echo is_a($box, "DynEvalNativeIntrospectChild") ? "C" : "c";
echo ":";
echo is_a($box, "DynEvalNativeIntrospectBase") ? "B" : "b";
echo ":";
echo is_subclass_of($box, "DynEvalNativeIntrospectChild") ? "S" : "s";
echo ":";
echo is_subclass_of($box, "DynEvalNativeIntrospectBase") ? "P" : "p";
"#,
    );
    assert_eq!(
        out,
        "DynEvalNativeIntrospectChild:DynEvalNativeIntrospectBase:C:B:s:P"
    );
}

/// Verifies native `instanceof` sees eval-declared class metadata after the barrier.
#[test]
fn test_eval_declared_class_native_instanceof_after_barrier() {
    let out = compile_and_run(
        r#"<?php
interface DynEvalNativeInstanceAotIface {}
class DynEvalNativeInstanceAotBase {}
eval('interface DynEvalNativeInstanceIface {}
class DynEvalNativeInstanceBase {}
class DynEvalNativeInstanceChild extends DynEvalNativeInstanceBase implements DynEvalNativeInstanceIface {}
class DynEvalNativeInstanceAotChild extends DynEvalNativeInstanceAotBase implements DynEvalNativeInstanceAotIface {}');
$box = new DynEvalNativeInstanceChild();
$aotBox = new DynEvalNativeInstanceAotChild();
echo $box instanceof DynEvalNativeInstanceChild ? "C" : "c";
echo ":";
echo $box instanceof DynEvalNativeInstanceBase ? "B" : "b";
echo ":";
echo $box instanceof DynEvalNativeInstanceIface ? "I" : "i";
echo ":";
echo $aotBox instanceof DynEvalNativeInstanceAotBase ? "A" : "a";
echo ":";
echo $aotBox instanceof DynEvalNativeInstanceAotIface ? "F" : "f";
echo ":";
$target = "DynEvalNativeInstanceChild";
echo $box instanceof $target ? "D" : "d";
echo ":";
$iface = "DynEvalNativeInstanceIface";
echo $box instanceof $iface ? "T" : "t";
echo ":";
echo 7 instanceof DynEvalNativeInstanceChild ? "bad" : "S";
"#,
    );
    assert_eq!(out, "C:B:I:A:F:D:T:S");
}

/// Verifies dynamic `instanceof` keeps invalid-target fatals after the eval barrier.
#[test]
fn test_eval_declared_class_native_dynamic_instanceof_invalid_target_after_barrier() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class DynEvalNativeInstanceInvalid {}');
$box = new DynEvalNativeInstanceInvalid();
$target = 7;
echo $box instanceof $target ? "bad" : "bad";
"#,
    );
    assert!(
        err.contains("Fatal error: Class name must be a valid object or a string"),
        "{err}"
    );
}

/// Verifies invalid dynamic targets still fatal when the tested value is scalar.
#[test]
fn test_eval_declared_class_native_dynamic_instanceof_scalar_lhs_invalid_target_after_barrier() {
    let err = compile_and_run_expect_failure(
        r#"<?php
eval('class DynEvalNativeInstanceScalarInvalid {}');
$value = 7;
$target = 7;
echo $value instanceof $target ? "bad" : "bad";
"#,
    );
    assert!(
        err.contains("Fatal error: Class name must be a valid object or a string"),
        "{err}"
    );
}

/// Verifies native callable probes see eval-declared dynamic callable targets.
#[test]
fn test_eval_declared_class_native_callable_probe_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeCallableProbe {
    public function bump($n) { return $n + 1; }
    public static function up($n) { return $n + 2; }
}
function dyn_eval_native_callable_fn($n) { return $n + 3; }');
$box = new DynEvalNativeCallableProbe();
$call = [$box, "bump"];
echo is_callable($call) ? "Y" : "N";
echo ":";
echo call_user_func($call, 4);
echo ":";
echo is_callable("dyn_eval_native_callable_fn") ? "F" : "f";
echo ":";
echo is_callable(["DynEvalNativeCallableProbe", "up"]) ? "S" : "s";
echo ":";
echo is_callable([$box, "missing"]) ? "M" : "m";
"#,
    );
    assert_eq!(out, "Y:5:F:S:m");
}

/// Verifies native member-existence probes see eval-declared class metadata after the barrier.
#[test]
fn test_eval_declared_class_native_member_exists_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeMemberProbe {
    public int $value = 1;
    private int $secret = 2;
    public function bump() { return $this->value + 1; }
    private function hidden() { return $this->secret; }
}');
$box = new DynEvalNativeMemberProbe();
echo method_exists($box, "bump") ? "M" : "m";
echo ":";
echo method_exists("DynEvalNativeMemberProbe", "bump") ? "C" : "c";
echo ":";
echo method_exists($box, "missing") ? "x" : "X";
echo ":";
echo property_exists($box, "value") ? "P" : "p";
echo ":";
echo property_exists("DynEvalNativeMemberProbe", "value") ? "S" : "s";
echo ":";
echo property_exists($box, "missing") ? "y" : "Y";
echo ":";
echo function_exists("method_exists") ? "F" : "f";
echo function_exists("property_exists") ? "G" : "g";
"#,
    );
    assert_eq!(out, "M:C:X:P:S:Y:FG");
}

/// Verifies native class-relation probes see eval-declared metadata after the barrier.
#[test]
fn test_eval_declared_class_native_relations_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('interface DynEvalNativeRelIface {}
trait DynEvalNativeRelTrait {}
class DynEvalNativeRelBase {}
class DynEvalNativeRelChild extends DynEvalNativeRelBase implements DynEvalNativeRelIface {
    use DynEvalNativeRelTrait;
}');
$object = new DynEvalNativeRelChild();
$implements = class_implements($object);
foreach ($implements as $name) { echo $name . ","; }
echo ":";
$parents = class_parents("DynEvalNativeRelChild");
foreach ($parents as $name) { echo $name . ","; }
echo ":";
$uses = class_uses("DynEvalNativeRelChild");
foreach ($uses as $name) { echo $name . ","; }
echo ":";
echo class_implements("MissingDynEvalNativeRel") === false ? "F" : "f";
"#,
    );
    assert_eq!(
        out,
        "DynEvalNativeRelIface,:DynEvalNativeRelBase,:DynEvalNativeRelTrait,:F"
    );
}

/// Verifies native static member reads see eval-declared metadata after the barrier.
#[test]
fn test_eval_declared_class_native_static_members_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeStaticBase {
    public const BASE = 3;
    public static int $count = 5;
}
class DynEvalNativeStaticChild extends DynEvalNativeStaticBase {
    public const SEED = 4;
    public static int $value = 6;
}');
echo DynEvalNativeStaticChild::SEED . ":";
echo DynEvalNativeStaticChild::BASE . ":";
echo DynEvalNativeStaticChild::$value . ":";
echo DynEvalNativeStaticChild::$count;
"#,
    );
    assert_eq!(out, "4:3:6:5");
}

/// Verifies native static method calls can dispatch to eval-declared classes after the barrier.
#[test]
fn test_eval_declared_class_native_static_method_call_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeStaticCallBase {
    public static function base($n) { return $n + 10; }
}
class DynEvalNativeStaticCallChild extends DynEvalNativeStaticCallBase {
    public static function up($n) { return static::base($n) + 1; }
}');
echo DynEvalNativeStaticCallChild::up(3) . ":";
echo DynEvalNativeStaticCallChild::base(4);
"#,
    );
    assert_eq!(out, "14:14");
}

/// Verifies native static property writes can update eval-declared classes after the barrier.
#[test]
fn test_eval_declared_class_native_static_property_write_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeStaticWriteBase {
    public static int $count = 5;
}
class DynEvalNativeStaticWriteChild extends DynEvalNativeStaticWriteBase {
    public static string $label = "old";
}');
DynEvalNativeStaticWriteChild::$label = "new";
DynEvalNativeStaticWriteChild::$count = 7;
DynEvalNativeStaticWriteChild::$count += 2;
echo DynEvalNativeStaticWriteChild::$label . ":";
echo DynEvalNativeStaticWriteChild::$count;
"#,
    );
    assert_eq!(out, "new:9");
}

/// Verifies native static property array writes update eval-declared classes after the barrier.
#[test]
fn test_eval_declared_class_native_static_property_array_write_after_barrier() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalNativeStaticArrayWrite {
    public static $items = [1];
    public static $map = ["a" => "old"];
}');
DynEvalNativeStaticArrayWrite::$items[] = 4;
DynEvalNativeStaticArrayWrite::$items[0] = 5;
DynEvalNativeStaticArrayWrite::$map["a"] = "new";
DynEvalNativeStaticArrayWrite::$map["b"] = "bee";
echo DynEvalNativeStaticArrayWrite::$items[0] . ":";
echo DynEvalNativeStaticArrayWrite::$items[1] . ":";
echo DynEvalNativeStaticArrayWrite::$map["a"] . ":";
echo DynEvalNativeStaticArrayWrite::$map["b"];
"#,
    );
    assert_eq!(out, "5:4:new:bee");
}

/// Verifies eval class declarations from a namespace are registered globally.
#[test]
fn test_eval_declared_class_in_namespace_is_global() {
    let out = compile_and_run(
        r#"<?php
namespace EvalNs;
eval('class DynEvalNsGlobalClass {
    public function label() { return "global"; }
}');
echo class_exists('EvalNs\\DynEvalNsGlobalClass') ? '1' : '0';
echo ":";
echo class_exists('DynEvalNsGlobalClass') ? '1' : '0';
echo ":";
$box = new \DynEvalNsGlobalClass();
echo $box->label();
"#,
    );
    assert_eq!(out, "0:1:global");
}

/// Verifies eval-declared by-reference promoted properties remain aliased after construction.
#[test]
fn test_eval_declared_class_aliases_by_reference_promoted_property() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalPromotedRefSupported {
    public function __construct(public &$value) {}
}');
echo eval('$value = 1;
$box = new DynEvalPromotedRefSupported($value);
$box->value = 5;
echo $value . ":";
$value = 7;
return $box->value;');
"#,
    );
    assert_eq!(out, "5:7");
}

/// Verifies eval promoted by-reference properties alias static and nested property targets.
#[test]
fn test_eval_declared_class_aliases_by_reference_promoted_static_and_nested_properties() {
    let out = compile_and_run(
        r#"<?php
eval('class DynEvalPromotedStaticRefHolder {
    public static $value = 1;
    public $items = [1];
    public static $staticItems = [1];
}
class DynEvalPromotedStaticRefSupported {
    public function __construct(public &$value) {}
}');
echo eval('$box = new DynEvalPromotedStaticRefSupported(DynEvalPromotedStaticRefHolder::$value);
$box->value = 5;
echo DynEvalPromotedStaticRefHolder::$value . ":";
DynEvalPromotedStaticRefHolder::$value = 7;
echo $box->value . ":";
$holder = new DynEvalPromotedStaticRefHolder();
$itemBox = new DynEvalPromotedStaticRefSupported($holder->items[0]);
$itemBox->value = 11;
echo $holder->items[0] . ":";
$holder->items[0] = 13;
echo $itemBox->value . ":";
$staticItemBox = new DynEvalPromotedStaticRefSupported(DynEvalPromotedStaticRefHolder::$staticItems[0]);
$staticItemBox->value = 17;
echo DynEvalPromotedStaticRefHolder::$staticItems[0] . ":";
DynEvalPromotedStaticRefHolder::$staticItems[0] = 19;
return $staticItemBox->value;');
"#,
    );
    assert_eq!(out, "5:7:11:13:17:19");
}

/// Verifies eval `class_alias()` supports class-like interface, trait, enum, and class targets.
#[test]
fn test_eval_class_alias_supports_class_like_targets() {
    let out = compile_and_run(
        r#"<?php
echo eval('interface EvalAliasIface {}
trait EvalAliasTrait {}
enum EvalAliasEnum: string { case Ready = "ready"; }
class EvalAliasClass {}
echo class_alias("EvalAliasIface", "EvalAliasIfaceCopy") ? "I" : "i"; echo ":";
echo interface_exists("EvalAliasIfaceCopy") ? "IE" : "ie"; echo ":";
echo class_exists("EvalAliasIfaceCopy") ? "bad" : "IC"; echo ":";
echo is_a("EvalAliasIfaceCopy", "EvalAliasIface", true) ? "II" : "ii"; echo ":";
echo (new ReflectionClass("EvalAliasIfaceCopy"))->isInterface() ? "IR" : "ir"; echo ":";
echo class_alias("UnitEnum", "EvalAliasUnitEnum") ? "U" : "u"; echo ":";
echo interface_exists("EvalAliasUnitEnum") ? "UE" : "ue"; echo ":";
echo class_exists("EvalAliasUnitEnum") ? "bad" : "UC"; echo ":";
echo class_alias("EvalAliasTrait", "EvalAliasTraitCopy") ? "T" : "t"; echo ":";
echo trait_exists("EvalAliasTraitCopy") ? "TE" : "te"; echo ":";
echo class_exists("EvalAliasTraitCopy") ? "bad" : "TC"; echo ":";
echo is_a("EvalAliasTraitCopy", "EvalAliasTrait", true) ? "TI" : "ti"; echo ":";
echo class_alias("EvalAliasEnum", "EvalAliasEnumCopy") ? "E" : "e"; echo ":";
echo enum_exists("EvalAliasEnumCopy") ? "EE" : "ee"; echo ":";
echo class_exists("EvalAliasEnumCopy") ? "EC" : "bad"; echo ":";
echo (new ReflectionClass("EvalAliasEnumCopy"))->getName(); echo ":";
echo EvalAliasEnumCopy::Ready->value; echo ":";
echo class_alias("EvalAliasClass", "EvalAliasClassCopy") ? "C" : "c"; echo ":";
echo class_exists("EvalAliasClassCopy") ? "CE" : "ce"; echo ":";
$declaredClasses = get_declared_classes();
$classDeclared = false;
$enumDeclared = false;
$classAliasDeclared = false;
$enumAliasDeclared = false;
foreach ($declaredClasses as $name) {
    if ($name === "EvalAliasClass") { $classDeclared = true; }
    if ($name === "EvalAliasEnum") { $enumDeclared = true; }
    if ($name === "EvalAliasClassCopy") { $classAliasDeclared = true; }
    if ($name === "EvalAliasEnumCopy") { $enumAliasDeclared = true; }
}
echo ($classDeclared && $enumDeclared && !$classAliasDeclared && !$enumAliasDeclared) ? "DC" : "dc"; echo ":";
$declaredInterfaces = get_declared_interfaces();
$ifaceDeclared = false;
$ifaceAliasDeclared = false;
foreach ($declaredInterfaces as $name) {
    if ($name === "EvalAliasIface") { $ifaceDeclared = true; }
    if ($name === "EvalAliasIfaceCopy") { $ifaceAliasDeclared = true; }
}
echo ($ifaceDeclared && !$ifaceAliasDeclared) ? "DI" : "di"; echo ":";
$declaredTraits = get_declared_traits();
$traitDeclared = false;
$traitAliasDeclared = false;
foreach ($declaredTraits as $name) {
    if ($name === "EvalAliasTrait") { $traitDeclared = true; }
    if ($name === "EvalAliasTraitCopy") { $traitAliasDeclared = true; }
}
return ($traitDeclared && !$traitAliasDeclared) ? "DT" : "dt";');
"#,
    );
    assert_eq!(
        out,
        "I:IE:IC:II:IR:U:UE:UC:T:TE:TC:TI:E:EE:EC:EvalAliasEnum:ready:C:CE:DC:DI:DT"
    );
}

/// Verifies eval can construct an AOT class with no declared constructor.
#[test]
fn test_eval_dynamic_new_constructs_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewSupported {
    public int $x = 7;
}
echo eval('$box = new EvalDynamicNewSupported(); return $box->x;');
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval object construction runs an AOT zero-argument constructor.
#[test]
fn test_eval_dynamic_new_runs_zero_arg_constructor() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewZeroArgCtor {
    public int $x = 0;
    public function __construct() { $this->x = 9; }
}
echo eval('$box = new EvalDynamicNewZeroArgCtor(); return $box->x;');
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies eval object construction passes positional arguments to an AOT constructor.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewOneArgCtor {
    public int $x = 0;
    public function __construct(int $x) { $this->x = $x; }
}
echo eval('$box = new EvalDynamicNewOneArgCtor(11); return $box->x;');
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies eval dispatches generated/AOT constructors with untyped by-reference params.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_mixed_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewMixedRefCtor {
    public function __construct(mixed &$value) {
        $value = $value + 9;
    }
}

echo eval('$value = 30;
$box = new EvalDynamicNewMixedRefCtor($value);
return $value;');
"#,
    );
    assert_eq!(out, "39");
}

/// Verifies eval dispatches generated/AOT constructors with typed scalar by-reference params.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_typed_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewTypedRefCtor {
    public function __construct(int &$value) {
        $value = $value + 11;
    }
}

echo eval('$value = 40;
$box = new EvalDynamicNewTypedRefCtor($value);
return $value;');
"#,
    );
    assert_eq!(out, "51");
}

/// Verifies eval writes AOT constructor by-reference args back before a thrown fatal path.
#[test]
fn test_eval_dynamic_new_writes_back_constructor_by_ref_before_throw() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewThrowingRefCtor {
    public function __construct(int &$value) {
        $value = $value + 13;
        throw new Exception("ctor-fail");
    }
}

echo eval('$value = 7;
try {
    new EvalDynamicNewThrowingRefCtor($value);
    echo "bad";
} catch (Throwable $e) {
    echo get_class($e) . ":" . $e->getMessage() . ":";
}
return gettype($value) . ":" . $value;');
"#,
    );
    assert_eq!(out, "Exception:ctor-fail:integer:20");
}

/// Verifies eval writes nullable scalar by-reference AOT constructor results back to eval variables.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_nullable_scalar_by_ref_args() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewNullableScalarRefCtor {
    public function __construct(?string &$name, ?bool &$flag, ?float &$ratio) {
        $name = $name === null ? "ctor" : "eval-ctor";
        $flag = true;
        $ratio = $ratio === null ? 1.75 : 3.25;
    }
}

echo eval('$name = "eval";
$flag = false;
$ratio = 2.5;
$box = new EvalDynamicNewNullableScalarRefCtor($name, $flag, $ratio);
$first = $name . ":" . ($flag ? "T" : "F") . ":" . $ratio;
$name = null;
$flag = null;
$ratio = null;
$box = new EvalDynamicNewNullableScalarRefCtor($name, $flag, $ratio);
return $first . ":" . $name . ":" . ($flag ? "T" : "F") . ":" . $ratio;');
"#,
    );
    assert_eq!(out, "eval-ctor:T:3.25:ctor:T:1.75");
}

/// Verifies eval dispatches generated/AOT constructors with string by-reference params.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_string_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewStringRefCtor {
    public function __construct(string &$value) {
        $value = $value . "-ctor";
    }
}

echo eval('$value = "eval";
$box = new EvalDynamicNewStringRefCtor($value);
return $value;');
"#,
    );
    assert_eq!(out, "eval-ctor");
}

/// Verifies eval dispatches generated/AOT constructors with array by-reference params.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_array_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewArrayRefCtor {
    public function __construct(array &$items) {
        $items = [9, 10];
    }
}

echo eval('$items = [1, 2];
$box = new EvalDynamicNewArrayRefCtor($items);
return count($items) . ":" . $items[0] . ":" . $items[1];');
"#,
    );
    assert_eq!(out, "2:9:10");
}

/// Verifies eval dispatches generated/AOT constructors with object by-reference params.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_object_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewObjectRefCtorPayload {
    public int $value = 3;
}

class EvalDynamicNewObjectRefCtor {
    public function __construct(EvalDynamicNewObjectRefCtorPayload &$payload) {
        $payload = new EvalDynamicNewObjectRefCtorPayload();
        $payload->value = 11;
    }
}

echo eval('$payload = new EvalDynamicNewObjectRefCtorPayload();
$box = new EvalDynamicNewObjectRefCtor($payload);
return $payload->value;');
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies eval dispatches generated/AOT constructors with iterable by-reference params.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_iterable_by_ref_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewIterableRefCtor {
    public function __construct(iterable &$items) {
        $items = [12, 13];
    }
}

echo eval('$items = [1, 2];
$box = new EvalDynamicNewIterableRefCtor($items);
return is_iterable($items) . ":" . count($items) . ":" . $items[0] . ":" . $items[1];');
"#,
    );
    assert_eq!(out, "1:2:12:13");
}

/// Verifies eval object construction can call private AOT constructors from the declaring scope.
#[test]
fn test_eval_dynamic_new_runs_private_constructor_from_declaring_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewPrivateCtor {
    public int $x = 0;
    private function __construct(int $x) { $this->x = $x + 2; }

    public static function run(): void {
        $box = eval('return new EvalDynamicNewPrivateCtor(3);');
        echo $box->x;
    }
}

EvalDynamicNewPrivateCtor::run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval object construction rejects private AOT constructors outside the declaring scope.
#[test]
fn test_eval_dynamic_new_rejects_private_constructor_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewPrivateCtorBase {
    private function __construct(int $x) {}
}

class EvalDynamicNewPrivateCtorChild extends EvalDynamicNewPrivateCtorBase {
    public static function run(): void {
        eval('try {
            new EvalDynamicNewPrivateCtorBase(3);
            echo "bad";
        } catch (Error $e) {
            echo get_class($e) . ":" . $e->getMessage();
        }');
    }
}

EvalDynamicNewPrivateCtorChild::run();
"#,
    );
    assert_eq!(
        out,
        "Error:Call to private EvalDynamicNewPrivateCtorBase::__construct() from scope EvalDynamicNewPrivateCtorChild"
    );
}

/// Verifies eval object construction can call protected AOT constructors from child scopes.
#[test]
fn test_eval_dynamic_new_runs_protected_constructor_from_child_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewProtectedCtorBase {
    public int $x = 0;
    protected function __construct(int $x) { $this->x = $x + 2; }
}

class EvalDynamicNewProtectedCtorChild extends EvalDynamicNewProtectedCtorBase {
    public static function run(): void {
        $box = eval('return new EvalDynamicNewProtectedCtorBase(3);');
        echo $box->x;
    }
}

EvalDynamicNewProtectedCtorChild::run();
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval object construction rejects protected AOT constructors between sibling scopes.
#[test]
fn test_eval_dynamic_new_rejects_protected_constructor_from_sibling_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewProtectedCtorSiblingBase {}

class EvalDynamicNewProtectedCtorLeft extends EvalDynamicNewProtectedCtorSiblingBase {
    protected function __construct(int $x) {}
}

class EvalDynamicNewProtectedCtorRight extends EvalDynamicNewProtectedCtorSiblingBase {
    public static function run(): void {
        eval('try {
            new EvalDynamicNewProtectedCtorLeft(3);
            echo "bad";
        } catch (Error $e) {
            echo get_class($e) . ":" . $e->getMessage();
        }');
    }
}

EvalDynamicNewProtectedCtorRight::run();
"#,
    );
    assert_eq!(
        out,
        "Error:Call to protected EvalDynamicNewProtectedCtorLeft::__construct() from scope EvalDynamicNewProtectedCtorRight"
    );
}

/// Verifies eval object construction fills registered AOT constructor defaults.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_default_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewDefaultCtor {
    public string $label = "";
    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
}
echo eval('$first = new EvalDynamicNewDefaultCtor("A");
echo $first->label . ":";
$second = new EvalDynamicNewDefaultCtor(right: "Y", left: "X");
return $second->label;');
"#,
    );
    assert_eq!(out, "AB:XY");
}

/// Verifies eval materializes generated/AOT empty-array defaults during constructor dispatch.
#[test]
fn test_eval_dynamic_new_uses_constructor_empty_array_default() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewArrayDefaultCtor {
    public int $count = -1;
    public function __construct($items = []) {
        $this->count = is_array($items) ? 0 : 9;
    }
}
echo eval('$box = new EvalDynamicNewArrayDefaultCtor();
return $box->count;');
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies eval materializes generated/AOT non-empty array defaults during constructor dispatch.
#[test]
fn test_eval_dynamic_new_uses_constructor_array_default_values() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewArrayValueDefaultCtor {
    public string $label = "";
    public function __construct(array $items = [4, 5, 6]) {
        $this->label = $items[0] . ":" . $items[1] . ":" . $items[2];
    }
}
echo eval('$box = new EvalDynamicNewArrayValueDefaultCtor();
return $box->label;');
"#,
    );
    assert_eq!(out, "4:5:6");
}

/// Verifies eval materializes generated/AOT object defaults during constructor dispatch.
#[test]
fn test_eval_dynamic_new_uses_constructor_object_default() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewObjectDefaultDep {
    public string $label;
    public function __construct(string $left = "d", string $right = "e", string $third = "p", string $fourth = "") {
        $this->label = $left . $right . $third . $fourth;
    }
}

class EvalDynamicNewObjectDefaultCtor {
    public string $label = "";
    public function __construct(EvalDynamicNewObjectDefaultDep $dep = new EvalDynamicNewObjectDefaultDep("c", "t", "o", "r")) {
        $this->label = $dep->label;
    }
}

echo eval('$box = new EvalDynamicNewObjectDefaultCtor();
return $box->label;');
"#,
    );
    assert_eq!(out, "ctor");
}

/// Verifies eval materializes nested generated/AOT object defaults during constructor dispatch.
#[test]
fn test_eval_dynamic_new_uses_constructor_nested_object_default() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewNestedDefaultInner {
    public string $label;

    public function __construct(string $label = "inner") {
        $this->label = $label;
    }
}

class EvalDynamicNewNestedDefaultOuter {
    public EvalDynamicNewNestedDefaultInner $inner;

    public function __construct(EvalDynamicNewNestedDefaultInner $inner = new EvalDynamicNewNestedDefaultInner("outer")) {
        $this->inner = $inner;
    }
}

class EvalDynamicNewNestedDefaultCtor {
    public string $label = "";

    public function __construct(EvalDynamicNewNestedDefaultOuter $outer = new EvalDynamicNewNestedDefaultOuter(new EvalDynamicNewNestedDefaultInner("ctor"))) {
        $this->label = $outer->inner->label;
    }
}

echo eval('$box = new EvalDynamicNewNestedDefaultCtor();
return $box->label;');
"#,
    );
    assert_eq!(out, "ctor");
}

/// Verifies eval object construction passes more than two arguments to an AOT constructor.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_many_args() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewManyArgCtor {
    public string $label = "";
    public function __construct(int $a, int $b, int $c, string $suffix) {
        $this->label = ($a + $b + $c) . $suffix;
    }
}
echo eval('$box = new EvalDynamicNewManyArgCtor(1, 2, 3, "!"); return $box->label;');
"#,
    );
    assert_eq!(out, "6!");
}

/// Verifies inherited AOT methods returning eval results keep the boxed Mixed return ABI.
#[test]
fn test_eval_fragment_in_inherited_aot_method_returns_late_static_scope() {
    let out = compile_and_run(
        r#"<?php
class EvalInheritedAotScopeReturnBase {
    public function run() {
        return eval('return static::class;');
    }
}
class EvalInheritedAotScopeReturnChild extends EvalInheritedAotScopeReturnBase {}
echo (new EvalInheritedAotScopeReturnChild())->run();
"#,
    );
    assert_eq!(out, "EvalInheritedAotScopeReturnChild");
}

/// Verifies eval ReflectionClass::newInstanceArgs forwards named args to AOT constructors.
#[test]
fn test_eval_reflection_class_new_instance_args_constructs_aot_class() {
    let out = compile_and_run(
        r#"<?php
class EvalReflectNewArgsAotTarget {
    public string $label = "";
    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
}
echo eval('$ref = new ReflectionClass("EvalReflectNewArgsAotTarget");
$first = $ref->newInstanceArgs(["right" => "Y", "left" => "X"]);
echo $first->label . ":";
$second = $ref->newInstanceArgs(["Q", "R"]);
echo $second->label . ":";
$args = [];
$args["right"] = "N";
$args["left"] = "M";
$third = $ref->newInstanceArgs($args);
return $third->label;');
"#,
    );
    assert_eq!(out, "XY:QR:MN");
}

/// Verifies eval object construction passes AOT constructor arguments on the caller stack.
#[test]
fn test_eval_dynamic_new_runs_constructor_with_stack_string_arg() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewStackStringCtor {
    public string $label = "";
    public function __construct(string $a, string $b, string $c, string $d) {
        $this->label = $a . $b . $c . $d;
    }
}
echo eval('$box = new EvalDynamicNewStackStringCtor("Q", "R", "S", "T"); return $box->label;');
"#,
    );
    assert_eq!(out, "QRST");
}

/// Verifies eval follows PHP by accepting constructor arguments when no constructor exists.
#[test]
fn test_eval_dynamic_new_accepts_args_without_constructor() {
    let out = compile_and_run(
        r#"<?php
class EvalDynamicNewNoCtorArgs {
    public int $x = 4;
}
echo eval('$box = new EvalDynamicNewNoCtorArgs(99); return $box->x;');
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies eval object construction fails when no AOT class matches the name.
#[test]
fn test_eval_dynamic_new_missing_class_fails() {
    let err = compile_and_run_expect_failure("<?php eval('new EvalDynamicNewMissingClass();');");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal diagnostic: {err}"
    );
}

/// Verifies eval can construct explicitly qualified namespaced AOT classes.
#[test]
fn test_eval_dynamic_new_constructs_qualified_aot_class() {
    let out = compile_and_run(
        r#"<?php
namespace EvalDynamicNewNs;
class Box {
    public int $x = 13;
}
echo eval('return (new \EvalDynamicNewNs\Box())->x;');
"#,
    );
    assert_eq!(out, "13");
}

/// Verifies eval namespace imports resolve functions, constants, and AOT class aliases.
#[test]
fn test_eval_fragment_namespace_use_imports() {
    let out = compile_and_run(
        r#"<?php
namespace EvalUseBridge;
class Box {
    public int $x = 17;
}
eval('namespace EvalUseExec;
function imported_eval_func($x) { return $x + 1; }
define("EvalUseLib\\VALUE", 5);
use function EvalUseExec\\imported_eval_func as AliasFunc;
use const EvalUseLib\\VALUE as LocalValue;
use EvalUseBridge\\Box as BoxAlias;
$box = new BoxAlias();
echo AliasFunc(LocalValue) . ":" . $box->x;');
"#,
    );
    assert_eq!(out, "6:17");
}

/// Verifies eval grouped namespace imports resolve functions, constants, and AOT class aliases.
#[test]
fn test_eval_fragment_grouped_namespace_use_imports() {
    let out = compile_and_run(
        r#"<?php
namespace EvalGroupedUseBridge;
class Box {
    public int $x = 19;
}
eval('namespace EvalGroupedUseExec;
function imported_eval_func($x) { return $x + 1; }
define("EvalGroupedUseLib\\VALUE", 7);
use EvalGroupedUseBridge\\{Box as BoxAlias};
use function EvalGroupedUseExec\\{imported_eval_func as AliasFunc};
use const EvalGroupedUseLib\\{VALUE as LocalValue};
$box = new BoxAlias();
echo AliasFunc(LocalValue) . ":" . $box->x;');
"#,
    );
    assert_eq!(out, "8:19");
}

/// Verifies eval include executes PHP files through the bridge and shares caller scope.
#[test]
fn test_eval_fragment_include_executes_php_file_and_returns_value() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("eval-include-piece.php", '<?php echo "I"; $x = $x + 1; return $x;');
$x = 4;
echo eval('return include "eval-include-piece.php";');
echo ":" . $x;
"#,
    );
    assert_eq!(out, "I5:5");
}

/// Verifies eval include_once skips files already included and plain files echo as text.
#[test]
fn test_eval_fragment_include_once_and_plain_file() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("eval-once-piece.php", '<?php echo "O";');
file_put_contents("eval-plain-piece.txt", 'RAW');
eval('include_once "eval-once-piece.php"; include_once "eval-once-piece.php"; echo (include_once "eval-once-piece.php") ? "T" : "F";');
echo ":";
echo eval('return include "eval-plain-piece.txt";');
"#,
    );
    assert_eq!(out, "OT:RAW1");
}

/// Verifies missing eval require aborts through the runtime eval fatal path.
#[test]
fn test_eval_fragment_missing_require_fails() {
    let err =
        compile_and_run_expect_failure("<?php eval('require \"missing-eval-require.php\";');");
    assert!(
        err.contains("Fatal error: eval() runtime failed"),
        "stderr did not contain eval runtime fatal: {err}"
    );
}

/// Verifies eval reference assignments update the referenced caller local.
#[test]
fn test_eval_reference_assignment_updates_caller_local() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('$alias =& $x; $alias = 5;');
echo $x;
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies eval unset breaks a reference alias without unsetting the source variable.
#[test]
fn test_eval_unset_reference_alias_keeps_source_local() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('$alias =& $x; unset($alias); $alias = 9;');
echo $x . ":" . $alias;
"#,
    );
    assert_eq!(out, "1:9");
}

/// Verifies `return` inside eval becomes the expression result of `eval(...)`.
#[test]
fn test_eval_return_value_is_available_to_native_code() {
    let out = compile_and_run("<?php echo eval('return 7;');");
    assert_eq!(out, "7");
}

/// Verifies eval can read and write an existing native local through the materialized scope.
#[test]
fn test_eval_reads_and_writes_existing_local() {
    let out = compile_and_run(
        r#"<?php
$x = 10;
eval('$x = $x + 5;');
echo $x;
"#,
    );
    assert_eq!(out, "15");
}

/// Verifies eval-created variables are visible to later native code in the caller scope.
#[test]
fn test_eval_created_variable_is_visible_after_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$created = "yes";');
echo $created;
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies a variable created by one eval call is visible to a later eval call.
#[test]
fn test_eval_scope_persists_between_eval_calls() {
    let out = compile_and_run(
        r#"<?php
eval('$created = 2;');
eval('$created = $created + 5;');
echo $created;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval can replace an existing scalar local with a different runtime type.
#[test]
fn test_eval_can_change_existing_local_type() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
eval('$x = "changed";');
echo $x;
"#,
    );
    assert_eq!(out, "changed");
}

/// Verifies eval-created function locals can be returned from native function code.
#[test]
fn test_eval_created_function_local_can_be_returned() {
    let out = compile_and_run(
        r#"<?php
function make_value() {
    eval('$created = "fn";');
    return $created;
}
echo make_value();
"#,
    );
    assert_eq!(out, "fn");
}

/// Verifies eval return is independent from writes it performs to the caller scope.
#[test]
fn test_eval_return_and_scope_write_are_visible() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
$r = eval('$x = 3; return $x + 4;');
echo $x;
echo ":";
echo $r;
"#,
    );
    assert_eq!(out, "3:7");
}

/// Verifies an eval unset does not leave a stale Mixed local value visible.
#[test]
fn test_eval_unset_clears_existing_mixed_local() {
    let out = compile_and_run(
        r#"<?php
$x = eval('return 10;');
eval('unset($x);');
echo $x;
"#,
    );
    assert_eq!(out, "");
}

/// Verifies the eval bridge maps PHP opening tags inside fragments to parse diagnostics.
#[test]
fn test_eval_fragment_with_php_opening_tag_reports_parse_error() {
    let err = compile_and_run_expect_failure("<?php eval('<?php echo 1;');");
    assert!(
        err.contains("Parse error: eval() fragment is invalid"),
        "stderr did not contain eval parse diagnostic: {err}"
    );
}

/// Verifies Throwable objects thrown inside eval cross into the caller's catch block.
#[test]
fn test_eval_throw_crosses_caller_try_catch() {
    let out = compile_and_run(
        r#"<?php
$e = new Exception("eval boom");
try {
    eval('throw $e;');
    echo "bad";
} catch (Exception $caught) {
    echo "caught:" . $caught->getMessage();
}
"#,
    );
    assert_eq!(out, "caught:eval boom");
}

/// Verifies Throwable objects thrown by eval-declared functions cross native call sites.
#[test]
fn test_eval_declared_function_throw_crosses_native_try_catch() {
    let out = compile_and_run(
        r#"<?php
eval('function dyn_eval_throw($e) { throw $e; }');
try {
    dyn_eval_throw(new Exception("dyn boom"));
    echo "bad";
} catch (Exception $caught) {
    echo "caught:" . $caught->getMessage();
}
"#,
    );
    assert_eq!(out, "caught:dyn boom");
}

/// Verifies Throwable objects thrown by nested eval calls keep the original catch target.
#[test]
fn test_eval_nested_throw_crosses_caller_try_catch() {
    let out = compile_and_run(
        r#"<?php
$e = new Exception("nested boom");
try {
    eval('eval("throw $e;");');
    echo "bad";
} catch (Exception $caught) {
    echo "caught:" . $caught->getMessage();
}
"#,
    );
    assert_eq!(out, "caught:nested boom");
}

/// Verifies eval-internal try/catch consumes a thrown Throwable before returning.
#[test]
fn test_eval_try_catch_catches_throwable_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new Exception("eval boom");
} catch (Throwable $caught) {
    return 7;
}
return 0;');
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies eval-internal catch clauses can omit the Throwable variable.
#[test]
fn test_eval_try_catch_without_variable_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new Exception("eval boom");
} catch (Throwable) {
    return 8;
}
return 0;');
"#,
    );
    assert_eq!(out, "8");
}

/// Verifies eval's native Throwable bridge retains and exposes the third `$previous` argument.
#[test]
fn test_eval_exception_previous_round_trips_through_native_bridge() {
    let out = compile_and_run(
        r#"<?php
try {
    eval('throw new Exception("outer", 7, new RuntimeException("inner"));');
} catch (Exception $caught) {
    echo $caught->getMessage(), "|", $caught->getCode(), "|", $caught->getPrevious()->getMessage();
}
"#,
    );
    assert_eq!(out, "outer|7|inner");
}

/// Verifies eval-internal catch type narrowing uses the thrown object's class.
#[test]
fn test_eval_try_catch_matches_specific_exception_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new Exception("eval boom");
} catch (RuntimeException $wrong) {
    return "bad";
} catch (Exception $caught) {
    return is_a($caught, "Exception") ? "caught" : "bad-type";
}
return "miss";');
"#,
    );
    assert_eq!(out, "caught");
}

/// Verifies eval-internal union catch clauses match any listed class.
#[test]
fn test_eval_try_catch_matches_union_type_inside_eval() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    throw new RuntimeException("eval boom");
} catch (LogicException|RuntimeException $caught) {
    return is_a($caught, "RuntimeException") ? "union" : "bad-type";
} catch (Exception $fallback) {
    return "fallback";
}
return "miss";');
"#,
    );
    assert_eq!(out, "union");
}

/// Verifies eval-internal finally runs before returning from the fragment.
#[test]
fn test_eval_finally_runs_before_eval_return() {
    let out = compile_and_run(
        r#"<?php
echo eval('try {
    return 1;
} finally {
    echo "F";
}');
"#,
    );
    assert_eq!(out, "F1");
}
