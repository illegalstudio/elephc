//! Purpose:
//! Interpreter tests for eval SPL autoload helper builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Autoload registration/call helpers mirror the main backend's conservative stubs.
//! - `spl_autoload_extensions()` persists an eval-local extension string.

use super::super::*;
use super::support::*;

/// Verifies the eval SPL autoload helpers behave as `php -n` 8.5.6 does.
///
/// php prints `default:set:read:register:1:unregister:functions:call:callregister:namedext:111111`
/// for this fragment. The fixture used to register `"missing_loader"` and assert that it
/// SUCCEEDED, which pinned the very defect this replaces: php refuses a callback naming nothing at
/// registration, so the loader here is a function that exists.
#[test]
fn execute_program_dispatches_spl_autoload_builtins() {
    let program = parse_fragment(
        br#"function real_loader($name) {}
echo spl_autoload_extensions() === ".inc,.php" ? "default" : "bad"; echo ":";
echo spl_autoload_extensions(".php,.inc") === ".php,.inc" ? "set" : "bad"; echo ":";
echo spl_autoload_extensions(null) === ".php,.inc" ? "read" : "bad"; echo ":";
echo spl_autoload_register("real_loader") ? "register" : "bad"; echo ":";
echo count(spl_autoload_functions()); echo ":";
echo spl_autoload_unregister("real_loader") ? "unregister" : "bad"; echo ":";
$funcs = spl_autoload_functions();
echo is_array($funcs) && count($funcs) === 0 ? "functions" : "bad"; echo ":";
echo spl_autoload_call("MissingClass") === null ? "call" : "bad"; echo ":";
echo call_user_func("spl_autoload_register", "real_loader") ? "callregister" : "bad"; echo ":";
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
        "default:set:read:register:1:unregister:functions:call:callregister:namedext:111111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies a callback that names nothing is refused AT REGISTRATION, with PHP's wording.
///
/// `php -n` 8.5.6 throws `TypeError` the moment `spl_autoload_register()` is called, naming what
/// is wrong: a class that does not exist, a function that does not exist, a private method, an
/// instance method called statically. elephc accepted all of them and failed at the first
/// autoload instead — a line the program never wrote, and no line at all for a program that
/// happens never to autoload anything.
#[test]
fn execute_program_refuses_an_invalid_autoload_callback_at_registration() {
    let program = parse_fragment(
        br#"class Reg {
    public static function ok($n) {}
    private static function hidden($n) {}
    public function inst($n) {}
}
function probe($cb) {
    try { spl_autoload_register($cb); echo "accepted;"; }
    catch (\TypeError $e) { echo $e->getMessage(); echo ";"; }
}
probe("NotYetDeclared::m");
probe("not_a_function");
probe("Reg::hidden");
probe("Reg::inst");
echo spl_autoload_register("Reg::ok") ? "valid" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, ",
            "class \"NotYetDeclared\" not found;",
            "spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, ",
            "function \"not_a_function\" not found or invalid function name;",
            "spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, ",
            "cannot access private method Reg::hidden();",
            "spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, ",
            "non-static method Reg::inst() cannot be called statically;",
            "valid",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies unregistering a callback that was never valid is refused, not answered `false`.
///
/// `php -n` 8.5.6 throws `TypeError: spl_autoload_unregister(): Argument #1 ($callback) must be a
/// valid callback, function "not_a_function" not found or invalid function name`. There is no
/// `or null` in that prefix, because unlike registration this parameter is not nullable, and PHP
/// distinguishes "not a callback" from "not registered".
#[test]
fn execute_program_refuses_an_invalid_autoload_callback_at_unregistration() {
    let program = parse_fragment(
        br#"function real_loader($n) {}
try { spl_autoload_unregister("not_a_function"); echo "accepted;"; }
catch (\TypeError $e) { echo $e->getMessage(); echo ";"; }
echo spl_autoload_unregister("real_loader") ? "removed" : "absent";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "spl_autoload_unregister(): Argument #1 ($callback) must be a valid callback, ",
            "function \"not_a_function\" not found or invalid function name;",
            "absent",
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
