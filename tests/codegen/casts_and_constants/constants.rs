//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection constants, including php integer max, php integer min, and m pi.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Function-first definitions and main-script constants reserve storage for their widest value.
#[test]
fn test_constant_redefinition_shared_storage_keeps_the_widest_value() {
    let source = r#"<?php
function reserve_constants(): void {
    define('CAPACITY_X', 2);
    define('CAPACITY_Y', 3);
    define('CAPACITY_NUMBER', 'ignored');
}
const CAPACITY_X = 'abc';
const CAPACITY_Y = 'def';
const CAPACITY_NUMBER = 42;
reserve_constants();
echo CAPACITY_X, ':', CAPACITY_Y, ':', CAPACITY_NUMBER;
"#;
    let dir = std::env::temp_dir().join(format!(
        "elephc_constant_capacity_{}_{:?}", std::process::id(), std::thread::current().id(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let (asm, _, _) = compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    for name in ["CAPACITY_X", "CAPACITY_Y", "CAPACITY_NUMBER"] {
        let symbol = elephc::names::ir_global_symbol(&elephc::names::constant_storage_name(name));
        assert!(asm.contains(&format!(".comm {symbol}, 16,")),
            "{name}: every wide declaration must fit in the shared slot");
    }
    std::fs::remove_dir_all(&dir).unwrap();

    let out = compile_and_run_capture(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "abc:def:42", "{}", out.stderr);
    for name in ["CAPACITY_X", "CAPACITY_Y", "CAPACITY_NUMBER"] {
        assert_eq!(out.stderr.matches(&format!("Constant {name} already defined")).count(), 1,
            "{}", out.stderr);
    }
}

/// Verifies `PHP_INT_MAX` constant is correctly substituted at compile time and the
/// resulting binary outputs the maximum 64-bit signed integer value.
/// Fixture: `<?php echo PHP_INT_MAX;` → expects `9223372036854775807`.
#[test]
fn test_php_int_max() {
    let out = compile_and_run("<?php echo PHP_INT_MAX;");
    assert_eq!(out, "9223372036854775807");
}

/// Verifies `PHP_INT_MIN` constant is correctly substituted at compile time and the
/// resulting binary outputs the minimum 64-bit signed integer value.
/// Fixture: `<?php echo PHP_INT_MIN;` → expects `-9223372036854775808`.
#[test]
fn test_php_int_min() {
    let out = compile_and_run("<?php echo PHP_INT_MIN;");
    assert_eq!(out, "-9223372036854775808");
}

/// Verifies `M_PI` math constant is correctly substituted at compile time and the
/// resulting binary outputs the correct float approximation.
/// Fixture: `<?php echo M_PI;` → expects `3.1415926535898`.
#[test]
fn test_m_pi() {
    let out = compile_and_run("<?php echo M_PI;");
    assert_eq!(out, "3.1415926535898");
}

/// Verifies `PHP_FLOAT_MAX` constant is correctly substituted and the resulting binary
/// runs without crash; also verifies `is_float()` returns true for the value.
/// Fixture: `<?php echo is_float(PHP_FLOAT_MAX);` → expects `1`.
#[test]
fn test_php_float_max() {
    let out = compile_and_run("<?php echo is_float(PHP_FLOAT_MAX);");
    assert_eq!(out, "1");
}

/// A namespaced user constant may be named after a predefined constant the lexer tokenizes
/// on its own (`NAN`, `PHP_EOL`). The declaration binds `Demo\NAN`, and the global constant
/// stays reachable through its import. It failed with `Expected constant name after
/// 'const'`. Regression for #834.
#[test]
fn test_namespaced_constant_named_after_a_predefined_constant() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;

use const NAN as PHP_NAN;

const NAN = PHP_NAN;
const PHP_EOL = "<eol>";

var_dump(is_nan(PHP_NAN));
var_dump(\defined('Demo\NAN'));
var_dump(is_nan(\constant('Demo\NAN')));
var_dump(\constant('Demo\PHP_EOL'));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "string(5) \"<eol>\"\n",
        )
    );
}

/// Redefining a constant keeps its first value and warns where the second declaration runs,
/// whichever of `const` and `define()` spells either one: `define()` then returns false. A
/// constant PHP predefines (`NAN`) warns on its first user declaration. A namespaced name
/// prints its namespace lowercased, as PHP stores it. Expected output measured on PHP 8.5.10.
/// Regression for #1482.
#[test]
fn test_constant_redefinition_warns_and_keeps_the_first_value() {
    let out = compile_and_run_capture(
        r#"<?php
namespace App {
    const X = 1;
    const X = 5;
    function f() { return X; }
    echo f(), " ", \defined('App\X') ? "d" : "n", "\n";
}
namespace {
    const FOO = 1;
    const FOO = 2;
    var_dump(FOO, constant("FOO"));
    define('BAR', 1);
    define('BAR', 2);
    define('BAZ', 1);
    const BAZ = 3;
    const QUX = 4;
    var_dump(BAR, BAZ, define('QUX', 5), QUX);
    const NAN = 1;
    var_dump(is_nan(NAN));
}
"#,
    );
    assert!(out.success, "program exited non-zero: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "1 d\nint(1)\nint(1)\nint(1)\nint(1)\nbool(false)\nint(4)\nbool(true)\n"
    );
    assert_eq!(
        out.stderr,
        concat!(
            "Warning: Constant app\\X already defined, this will be an error in PHP 9\n",
            "Warning: Constant FOO already defined, this will be an error in PHP 9\n",
            "Warning: Constant BAR already defined, this will be an error in PHP 9\n",
            "Warning: Constant BAZ already defined, this will be an error in PHP 9\n",
            "Warning: Constant QUX already defined, this will be an error in PHP 9\n",
            "Warning: Constant NAN already defined, this will be an error in PHP 9\n",
        )
    );
}

/// Namespace aliases share a guard while basename case remains significant.
#[test]
fn test_constant_redefinition_namespace_identity_matches_php() {
    let out = compile_and_run_with_heap_debug(r#"<?php
namespace App { const X = 1; }
namespace {
 var_dump(define('app\X', 2));
 var_dump(define('APP\x', 3));
 var_dump(define('aPP\X', 4));
 echo App\X, ":", app\x, "\n";
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(true)\nbool(false)\n1:3\n");
    assert_eq!(out.stderr.matches("Warning: Constant app\\X already defined").count(), 2, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Leading separators in definitions remain distinct; a fetch removes exactly one.
#[test]
fn test_constant_redefinition_literal_root_separators_match_php() {
    let out = compile_and_run_with_heap_debug(r#"<?php
const FOO = 4;
var_dump(define("\\FOO", 5));
var_dump(define("\\\\FOO", 6));
echo constant("FOO"), ":", constant("\\FOO"), ":", constant("\\\\FOO"), ":", constant("\\\\\\FOO"), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\n4:4:5:6\n");
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A separator-prefixed name does not redefine the similarly spelled predefined constant.
#[test]
fn test_constant_redefinition_prefixed_predefined_name_is_distinct() {
    let out = compile_and_run_with_heap_debug(r#"<?php
var_dump(define("\\NAN", 9));
var_dump(define("\\NAN", 10));
echo constant("\\\\NAN"), ":", is_nan(NAN) ? "nan" : "other", "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\n9:nan\n");
    assert!(out.stderr.contains("Warning: Constant \\NAN already defined"), "{}", out.stderr);
    assert_eq!(out.stderr.matches("Warning:").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native eval shares AOT constant identity while retaining literal registration separators.
#[test]
fn test_constant_redefinition_aot_eval_identity_matches_php() {
    let out = compile_and_run_capture(r#"<?php
namespace App { const X = 1; }
namespace {
 const FOO = 4;
 eval("var_dump(define(\"app\\\\X\", 2)); var_dump(define(\"\\\\FOO\", 5)); echo constant(\"APP\\\\X\"), \":\", constant(\"FOO\"), \":\", constant(\"\\\\FOO\"), \":\", constant(\"\\\\\\\\FOO\"), \"\\n\";");
}
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(true)\n1:4:4:5\n");
    assert!(out.stderr.contains("Warning: Constant app\\X already defined"), "{}", out.stderr);
    assert_eq!(out.stderr.matches("Warning:").count(), 1, "{}", out.stderr);
    // Fresh dynamic eval-constant storage has independent retained-cell cleanup; this checks identity.
}

/// A query for two separators cannot resolve an unprefixed definition by stripping them all.
#[test]
fn test_constant_redefinition_defined_removes_only_one_root_separator() {
    let out = compile_and_run_with_heap_debug(r#"<?php
const FOO = 4;
var_dump(defined("FOO"), defined("\\FOO"), defined("\\\\FOO"));
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(true)\nbool(false)\n");
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Public inventories keep the first declaration's spelling across namespace-alias redefinitions.
#[test]
fn test_constant_redefinition_inventory_preserves_first_spelling() {
    let out = compile_and_run_with_heap_debug(r#"<?php
namespace App { const X = 1; }
namespace {
 define('app\X', 2);
 $all = get_defined_constants(true);
 var_dump($all["user"]["App\X"], isset($all["user"]["app\X"]));
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "int(1)\nbool(false)\n");
    assert_eq!(out.stderr.matches("Warning: Constant app\\X already defined").count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
