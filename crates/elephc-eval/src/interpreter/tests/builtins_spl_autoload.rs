//! Purpose:
//! Interpreter tests for eval SPL autoload helper builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Autoload registration/call helpers mirror the main backend's conservative stubs.
//! - `spl_autoload_extensions()` persists an eval-local extension string.

use super::super::*;
use super::support::*;

/// Verifies eval SPL autoload helpers expose stubbed behavior and extension state.
#[test]
fn execute_program_dispatches_spl_autoload_builtins() {
    let program = parse_fragment(
        br#"echo spl_autoload_extensions() === ".inc,.php" ? "default" : "bad"; echo ":";
echo spl_autoload_extensions(".php,.inc") === ".php,.inc" ? "set" : "bad"; echo ":";
echo spl_autoload_extensions(null) === ".php,.inc" ? "read" : "bad"; echo ":";
echo spl_autoload_register("missing_loader") ? "register" : "bad"; echo ":";
echo spl_autoload_unregister("missing_loader") ? "unregister" : "bad"; echo ":";
$funcs = spl_autoload_functions();
echo is_array($funcs) && count($funcs) === 0 ? "functions" : "bad"; echo ":";
echo spl_autoload("MissingClass") === null ? "autoload" : "bad"; echo ":";
echo spl_autoload_call("MissingClass") === null ? "call" : "bad"; echo ":";
echo call_user_func("spl_autoload_register", "missing_loader") ? "callregister" : "bad"; echo ":";
$named = call_user_func_array("spl_autoload_extensions", ["file_extensions" => ".class.php"]);
echo $named === ".class.php" ? "namedext" : "bad"; echo ":";
echo function_exists("spl_autoload"); echo function_exists("spl_autoload_call");
echo function_exists("spl_autoload_extensions"); echo function_exists("spl_autoload_functions");
echo function_exists("spl_autoload_register"); echo function_exists("spl_autoload_unregister");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "default:set:read:register:unregister:functions:autoload:call:",
            "callregister:namedext:111111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
