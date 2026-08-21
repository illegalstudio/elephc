//! Purpose:
//! Interpreter tests for eval stream context metadata builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Context resources are eval-owned resource cells with inspectable options.
//! - Params currently mirror the main backend's empty-array behavior.

use super::super::*;
use super::support::*;

/// Verifies eval stream context builtins create resources and persist options.
#[test]
fn execute_program_dispatches_stream_context_builtins() {
    let program = parse_fragment(
        br#"$ctx = stream_context_create(["http" => ["method" => "POST"]]);
echo is_resource($ctx) ? "ctx" : "bad"; echo ":";
echo get_resource_type($ctx) === "stream" ? "rtype" : "bad"; echo ":";
$opts = stream_context_get_options($ctx);
echo $opts["http"]["method"] === "POST" ? "initial" : "bad"; echo ":";
echo stream_context_set_option($ctx, "http", "header", "X-Test: 1") ? "setone" : "bad"; echo ":";
$opts = stream_context_get_options($ctx);
echo $opts["http"]["header"] === "X-Test: 1" ? "gotone" : "bad"; echo ":";
echo stream_context_set_option($ctx, ["ssl" => ["verify_peer" => false]]) ? "setall" : "bad"; echo ":";
$opts = stream_context_get_options($ctx);
echo $opts["ssl"]["verify_peer"] === false ? "gotall" : "bad"; echo ":";
echo stream_context_set_params($ctx, ["notification" => "noop"]) ? "paramsset" : "bad"; echo ":";
$params = stream_context_get_params($ctx);
echo count($params) === 0 ? "params" : "bad"; echo ":";
$default = stream_context_get_default();
echo is_resource($default) ? "default" : "bad"; echo ":";
$set_default = stream_context_set_default(["http" => ["timeout" => "1"]]);
echo is_resource($set_default) ? "setdefault" : "bad"; echo ":";
$call = call_user_func_array("stream_context_create", ["options" => ["ftp" => ["user" => "u"]]]);
$call_opts = call_user_func("stream_context_get_options", $call);
echo $call_opts["ftp"]["user"] === "u" ? "callcreate" : "bad"; echo ":";
echo call_user_func("stream_context_set_option", $call, "ftp", "mode", "passive") ? "callset" : "bad"; echo ":";
$call_opts = call_user_func("stream_context_get_options", $call);
echo $call_opts["ftp"]["mode"] === "passive" ? "callgot" : "bad"; echo ":";
echo function_exists("stream_context_create"); echo function_exists("stream_context_get_default");
echo function_exists("stream_context_set_default"); echo function_exists("stream_context_set_option");
echo function_exists("stream_context_set_params"); echo function_exists("stream_context_get_options");
echo function_exists("stream_context_get_params");
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "ctx:rtype:initial:setone:gotone:setall:gotall:paramsset:params:",
            "default:setdefault:callcreate:callset:callgot:1111111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies every `stream_context_set_option()` shape php refuses, plus its php 8.3 notice.
///
/// The fourth stub parameter carries NO default — it is `UNKNOWN`, not `null` — and the second
/// is `array|string`, so the arity alone does not decide what php accepts. MEASURED on
/// `php -n` 8.5.6:
///
/// ```text
/// ($c, ['http' => [...]])        E_DEPRECATED, then bool(true)
/// ($c, ['http' => [...]], null)  bool(true), and NO deprecation — the notice counts arguments
/// ($c, ['http' => [...]], 'x')   ValueError: Argument #3 ($option_name) must be null when argument #2 ($wrapper_or_options) is an array
/// ($c, 'http')                   E_DEPRECATED, then ValueError: Argument #3 ($option_name) cannot be null when argument #2 ($wrapper_or_options) is a string
/// ($c, 'http', 'header')         ValueError: Argument #4 ($value) must be provided when argument #2 ($wrapper_or_options) is a string
/// ($c, 'http', null)             ValueError: Argument #3 ($option_name) cannot be null when argument #2 ($wrapper_or_options) is a string
/// ($c, 'http', 'header', 'X: 1') bool(true)
/// ```
///
/// The three-argument form used to be an uncatchable `RuntimeFatal` here, and no shape printed
/// the notice.
#[test]
fn execute_program_stream_context_set_option_refuses_phps_invalid_shapes() {
    let program = parse_fragment(
        br#"$c1 = stream_context_create();
try { echo stream_context_set_option($c1, ["http" => ["a" => 1]]) === true ? "true" : "other"; }
catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
$c2 = stream_context_create();
try { echo stream_context_set_option($c2, ["http" => ["a" => 1]], null) === true ? "true" : "other"; }
catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
$c3 = stream_context_create();
try { echo stream_context_set_option($c3, ["http" => ["a" => 1]], "x") === true ? "true" : "other"; }
catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
$c4 = stream_context_create();
try { echo stream_context_set_option($c4, "http") === true ? "true" : "other"; }
catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
$c5 = stream_context_create();
try { echo stream_context_set_option($c5, "http", "header") === true ? "true" : "other"; }
catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
$c6 = stream_context_create();
try { echo stream_context_set_option($c6, "http", null) === true ? "true" : "other"; }
catch (ValueError $e) { echo $e->getMessage(); }
echo "|";
$c7 = stream_context_create();
try { echo stream_context_set_option($c7, "http", "header", "X: 1") === true ? "true" : "other"; }
catch (ValueError $e) { echo $e->getMessage(); }
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        concat!(
            "true",
            "|true",
            "|stream_context_set_option(): Argument #3 ($option_name) must be null when \
             argument #2 ($wrapper_or_options) is an array",
            "|stream_context_set_option(): Argument #3 ($option_name) cannot be null when \
             argument #2 ($wrapper_or_options) is a string",
            "|stream_context_set_option(): Argument #4 ($value) must be provided when \
             argument #2 ($wrapper_or_options) is a string",
            "|stream_context_set_option(): Argument #3 ($option_name) cannot be null when \
             argument #2 ($wrapper_or_options) is a string",
            "|true",
        )
    );
    assert_eq!(
        values.warnings,
        vec![
            "Deprecated: Calling stream_context_set_option() with 2 arguments is deprecated, \
             use stream_context_set_options() instead\n"
                .to_string();
            2
        ],
        "the notice fires on the ARITY, so the three-argument array form stays quiet"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
