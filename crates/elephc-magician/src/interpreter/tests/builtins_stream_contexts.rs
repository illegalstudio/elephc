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

/// Dropping one options result must leave the stream-context table owner available to later gets.
#[test]
fn stream_context_get_options_preserves_the_table_owner() {
    let drop_result = parse_fragment(
        br#"stream_context_get_options($ctx);
return true;"#,
    )
    .expect("parse first options fetch");
    let get_again = parse_fragment(br#"return stream_context_get_options($ctx);"#)
        .expect("parse second options fetch");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let options = values.assoc_new(0).expect("options array");
    let options_identity = options.as_ptr() as usize;
    let resource_id = context
        .stream_resources_mut()
        .open_stream_context(Some(options));
    let resource = values.resource(resource_id).expect("context resource");
    scope.set("ctx", resource, ScopeCellOwnership::Owned);

    let first = execute_program_with_context(
        &mut context,
        &drop_result,
        &mut scope,
        &mut values,
    )
    .expect("drop first options result");
    values.release(first).expect("release first program result");
    assert_eq!(values.cell_owners.get(&options_identity), Some(&1));

    let second = execute_program_with_context(
        &mut context,
        &get_again,
        &mut scope,
        &mut values,
    )
    .expect("fetch options again");
    assert_eq!(second.as_ptr(), options.as_ptr());
    assert!(!second.is_borrowed());
    assert_eq!(values.cell_owners.get(&options_identity), Some(&2));
    values.release(second).expect("release second options result");
    assert_eq!(values.cell_owners.get(&options_identity), Some(&1));
}
