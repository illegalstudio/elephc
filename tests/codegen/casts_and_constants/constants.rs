//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of casts, constants, and introspection constants, including php integer max, php integer min, and m pi.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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
