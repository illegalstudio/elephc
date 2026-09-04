//! Purpose:
//! Interpreter regression tests for PHP Core runtime and declaration introspection.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Cases cover stacks, nested handlers, constants, functions, scope values, and mangled keys.

use super::super::*;
use super::support::*;

/// Verifies eval error masks, nested handlers, callback arguments, and restoration.
#[test]
fn execute_program_dispatches_error_reporting_and_user_handlers() {
    let program = parse_fragment(
        br#"function first_handler($level, $message, $file, $line) {
    echo "first:" . $level . ":" . $message . ":" . $line . "|";
    return true;
}
function second_handler($level, $message) {
    echo "second:" . $message . "|";
    return true;
}
echo error_reporting() . "|";
echo error_reporting(1234) . "|";
echo error_reporting() . "|";
echo is_null(set_error_handler("first_handler", E_USER_WARNING)) ? "null|" : "bad|";
$previous = set_error_handler("second_handler", E_USER_WARNING);
echo is_string($previous) ? $previous . "|" : "bad|";
trigger_error("two", E_USER_WARNING);
restore_error_handler();
user_error("one", E_USER_WARNING);
restore_error_handler();
return error_reporting(null);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/core.php", "/tmp", 42);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(
        values.output,
        "30719|30719|1234|null|first_handler|second:two|first:512:one:42|"
    );
    assert_eq!(values.get(result), FakeValue::Int(1234));
    assert!(values.warnings.is_empty());
}

/// Verifies an unhandled eval `E_USER_ERROR` emits PHP's diagnostic and reports a fatal status.
#[test]
fn execute_program_reports_unhandled_user_error_as_fatal() {
    let program = parse_fragment(br#"trigger_error("boom", E_USER_ERROR); return true;"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/eval-fatal.php", "/tmp", 17);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::UserFatal));
    assert_eq!(
        values.warnings,
        ["Fatal error: boom in /tmp/eval-fatal.php on line 17\n"]
    );
}

/// Verifies a suppressing eval handler may recover from `E_USER_ERROR` like PHP.
#[test]
fn execute_program_allows_handler_to_suppress_user_error() {
    let program = parse_fragment(
        br#"function suppress_fatal($level, $message) {
    echo $level . ":" . $message . "|";
    return true;
}
set_error_handler("suppress_fatal", E_USER_ERROR);
echo trigger_error("handled", E_USER_ERROR) ? "alive" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "256:handled|alive");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert!(values.warnings.is_empty());
}

/// Verifies eval rejects engine error levels with a catchable PHP `ValueError`.
#[test]
fn execute_program_rejects_non_user_trigger_error_level() {
    let program = parse_fragment(
        br#"try {
    trigger_error("invalid", E_WARNING);
} catch (ValueError $error) {
    echo $error->getMessage();
}
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "trigger_error(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies pure eval exception-handler registration preserves PHP callback stack results.
#[test]
fn execute_program_stacks_and_restores_exception_handlers() {
    let program = parse_fragment(
        br#"function first_exception_handler($exception) {}
function second_exception_handler($exception) {}
echo is_null(set_exception_handler("first_exception_handler")) ? "N|" : "bad|";
echo set_exception_handler("second_exception_handler") === "first_exception_handler" ? "P|" : "bad|";
echo restore_exception_handler() ? "R|" : "bad|";
echo set_exception_handler(null) === "first_exception_handler" ? "F|" : "bad|";
echo restore_exception_handler() ? "R" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.output, "N|P|R|F|R");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies stack frames include the active function, arguments, source metadata, and limits.
#[test]
fn execute_program_materializes_and_prints_backtraces() {
    let program = parse_fragment(
        br#"function traced($value) {
    $trace = debug_backtrace();
    echo $trace[0]["function"] . ":" . $trace[0]["args"][0] . ":" . $trace[0]["line"] . "|";
    $ignored = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 1);
    echo isset($ignored[0]["args"]) ? "bad|" : "ignored|";
    debug_print_backtrace(0, 1);
}

traced(7);
return function_exists("debug_backtrace");"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/trace.php", "/tmp", 19);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(
        values.output,
        "traced:7:19|ignored|#0 /tmp/trace.php(19): traced(7)\n"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval printed backtraces use PHP byte escaping for string arguments.
#[test]
fn execute_program_escapes_printed_backtrace_string_arguments() {
    let program = parse_fragment(
        br#"function escaped_trace($value) {
    debug_print_backtrace(0, 1);
}
$escaped = "a\\b\nc\t" . chr(1) . chr(195) . chr(169);
escaped_trace($escaped);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/escaped-trace.php", "/tmp", 7);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(
        values.output,
        "#0 /tmp/escaped-trace.php(7): escaped_trace('a\\\\b\\nc\\t\\x01\\xC3\\xA9')\n"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies nested eval method frames preserve class, object, call type, arguments, and limits.
#[test]
fn execute_program_materializes_nested_method_backtraces() {
    let program = parse_fragment(
        br#"class EvalTraceFrames {
    public function outer($value) {
        self::middle($value + 1);
    }
    public static function middle($value) {
        (new EvalTraceFrames())->inner($value + 1);
    }
    public function inner($value) {
        $trace = debug_backtrace();
        echo count($trace) . ":";
        echo $trace[0]["function"] . ":" . $trace[0]["class"] . ":" . $trace[0]["type"] . ":";
        echo is_object($trace[0]["object"]) ? "object:" : "bad:";
        echo $trace[0]["args"][0] . "|";
        echo $trace[1]["function"] . ":" . $trace[1]["class"] . ":" . $trace[1]["type"] . ":";
        echo isset($trace[1]["object"]) ? "bad:" : "static:";
        echo $trace[1]["args"][0] . "|";
        echo $trace[2]["function"] . ":" . $trace[2]["class"] . ":" . $trace[2]["type"] . ":";
        echo is_object($trace[2]["object"]) ? "object:" : "bad:";
        echo $trace[2]["args"][0] . "|";
        $withoutObject = debug_backtrace(0, 1);
        echo isset($withoutObject[0]["object"]) ? "bad|" : "no-object|";
        echo count(debug_backtrace(0, -1)) . "|";
        debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2);
    }
}
(new EvalTraceFrames())->outer(2);
return true;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/nested-trace.php", "/tmp", 31);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(
        values.output,
        "3:inner:EvalTraceFrames:->:object:4|middle:EvalTraceFrames::::static:3|outer:EvalTraceFrames:->:object:2|no-object|0|#0 /tmp/nested-trace.php(31): EvalTraceFrames->inner()\n#1 /tmp/nested-trace.php(31): EvalTraceFrames::middle()\n"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies declaration, scope, extension, include, and visibility-mangled introspection.
#[test]
fn execute_program_materializes_core_introspection_arrays() {
    let program = parse_fragment(
        br#"define("LOCAL_CONSTANT", 9);
function local_function() {}
class IntrospectionBag {
    public $open = 1;
    protected $guarded = 2;
    private $secret = 3;
}
$local = 7;
$vars = get_defined_vars();
$callbackVars = call_user_func("get_defined_vars");
$arrayCallbackVars = call_user_func_array("GET_DEFINED_VARS", []);
$constants = get_defined_constants(true);
$functions = get_defined_functions();
$extension = get_extension_funcs("cOrE");
$mangled = get_mangled_object_vars(new IntrospectionBag());
echo $vars["local"] . ":" . $callbackVars["local"] . ":";
echo $arrayCallbackVars["local"] . ":" . $constants["user"]["LOCAL_CONSTANT"] . ":";
echo in_array("local_function", $functions["user"]) ? "function:" : "bad:";
echo in_array("debug_backtrace", $extension) ? count($extension) . ":" : "bad:";
$protected = chr(0) . "*" . chr(0) . "guarded";
$private = chr(0) . "IntrospectionBag" . chr(0) . "secret";
echo $mangled["open"] . ":" . $mangled[$protected] . ":";
echo $mangled[$private] . ":";
echo get_included_files()[0] === get_required_files()[0] ? "files" : "bad";
return get_extension_funcs("missing");"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/main.php", "/tmp", 1);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.output, "7:7:7:9:function:59:1:2:3:files");
    assert_eq!(values.get(result), FakeValue::Bool(false));
}

/// Verifies only explicit `call_user_func` string dispatch may inspect its caller variables.
#[test]
fn execute_program_get_defined_vars_rejects_other_dynamic_invocation() {
    let program = parse_fragment(
        br#"function inspect_dynamic_scope() {
    $local = 7;
    $name = "get_defined_vars";
    try {
        $name();
    } catch (Error $error) {
        echo $error->getMessage() . "|";
    }
    try {
        $callback = get_defined_vars(...);
        call_user_func($callback);
    } catch (Error $error) {
        echo $error->getMessage() . "|";
    }
    $visible = call_user_func("get_defined_vars");
    echo $visible["local"];
}
inspect_dynamic_scope();
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "Cannot call get_defined_vars() dynamically|Cannot call get_defined_vars() dynamically|7"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies resource enumeration, PHP ids, type filters, and retained closed entries.
#[test]
fn execute_program_enumerates_live_resources() {
    let program = parse_fragment(
        br#"$first = fopen("php://memory", "w+");
$second = fopen("php://memory", "w+");
$all = get_resources();
echo count($all) . ":";
echo array_key_exists(get_resource_id($first), $all) ? "first:" : "bad:";
echo array_key_exists(get_resource_id($second), $all) ? "second:" : "bad:";
echo count(get_resources("stream")) . ":";
try {
    get_resources("missing");
} catch (ValueError $error) {
    echo $error->getMessage() . ":";
}
fclose($first);
echo count(call_user_func("get_resources")) . ":";
echo count(get_resources("Unknown"));
fclose($second);
return function_exists("get_resources");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "6:first:second:5:get_resources(): Argument #1 ($type) must be a valid resource type:6:1"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
