//! Purpose:
//! Exercises eval argument and callable lifetimes across source-order side effects.
//!
//! Called from:
//! - The codegen integration harness through `runtime_gc`.
//!
//! Key details:
//! - Opaque eval prevents compile-time folding from bypassing runtime argument evaluation.
//! - Replacing a global argument source must not invalidate an earlier borrowed value.

use crate::support::*;

/// Recycled temporary arrays do not inherit PHP reference targets from previous allocations.
#[test]
fn test_core_eval_freed_array_reference_metadata_does_not_alias_new_values() {
    let source = r#"<?php
$source = '$value = 11;
for ($i = 0; $i < 24; $i++) {
    $references = [&$value];
    $references[0] = 12;
    unset($references);
    $copy = [99];
    echo $copy[0], ":", $value, "|";
    unset($copy);
    $value = 11;
}' . ' // ' . $argc;
eval($source);
"#;
    let output = compile_and_run_capture(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "99:12|".repeat(24), "{}", output.stderr);
}

/// Promoted local arrays and copied property arrays preserve references and independent value owners.
#[test]
fn test_core_eval_array_reference_writes_survive_container_release() {
    let source = r#"<?php
$source = '$value = "before";
$references = [&$value];
$references["extra"] = "unused";
$references["0"] = str_repeat("n", 3);
echo $references[0], ":", $value, "|";
unset($references);
echo $value, "|";
$box = new stdClass();
$box->items = [&$value];
$box->items[0] = str_repeat("z", 4);
unset($box);
echo $value;' . ' // ' . $argc;
eval($source);
"#;
    let output = compile_and_run_capture(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "nnn:nnn|nnn|zzzz", "{}", output.stderr);
}

/// Repeated reference writes release their key/value operands and both persistent storage owners.
#[test]
fn test_core_eval_array_reference_writes_balance_owners() {
    let operation = r#"$value = str_repeat("a", 4);
$references = [&$value];
$references[0] = str_repeat("b", 4);
unset($references, $value);"#;
    assert_eq!(
        live_blocks_after_eval_operations("", operation, 5),
        live_blocks_after_eval_operations("", operation, 1),
        "Array reference writes retained an operand or storage owner",
    );
}

/// Native functions, methods, and constructors consume the value captured before later arguments.
#[test]
fn test_core_eval_native_arguments_survive_later_source_replacement() {
    let source = r#"<?php
function native_argument_first(mixed $first, mixed $ignored): mixed { return $first; }
class NativeArgumentLifetime {
    public mixed $saved;
    public function __construct(mixed $first, mixed $ignored) { $this->saved = $first; }
    public function first(mixed $first, mixed $ignored): mixed { return $first; }
    public static function firstStatic(mixed $first, mixed $ignored): mixed { return $first; }
}
$source = 'function replaceArgument() {
    global $argument;
    $argument = str_repeat("replacement", 8);
    return 0;
}
$sink = new NativeArgumentLifetime("setup", 0);
$argument = str_repeat("old", 2);
echo native_argument_first($argument, replaceArgument()), ":", strlen($argument), "|";
$argument = str_repeat("old", 2);
echo $sink->first($argument, replaceArgument()), ":", strlen($argument), "|";
$argument = str_repeat("old", 2);
echo NativeArgumentLifetime::firstStatic($argument, replaceArgument()), ":", strlen($argument), "|";
$argument = str_repeat("old", 2);
$created = new NativeArgumentLifetime($argument, replaceArgument());
echo $created->saved, ":", strlen($argument), "|";
$class = "NativeArgumentLifetime";
$argument = str_repeat("old", 2);
$created = new $class($argument, replaceArgument());
echo $created->saved, ":", strlen($argument);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "oldold:88|oldold:88|oldold:88|oldold:88|oldold:88");
}

/// Eval functions and callable expressions retain source values and callback receivers until dispatch.
#[test]
fn test_core_eval_declared_arguments_and_callback_survive_replacement() {
    let source = r#"<?php
$source = 'function replaceArgument() {
    global $argument;
    $argument = str_repeat("replacement", 8);
    return 0;
}
function firstArgument($first, $ignored) { return $first; }
class EvalArgumentLifetime {
    public function first($first, $ignored) { return $first; }
    public function __invoke($first, $ignored) { return $first; }
}
function replaceCallback() { global $callback; $callback = null; return 0; }
$argument = str_repeat("old", 2);
echo firstArgument($argument, replaceArgument()), ":", strlen($argument), "|";
$name = "firstArgument";
$argument = str_repeat("old", 2);
echo $name($argument, replaceArgument()), ":", strlen($argument), "|";
$firstClass = firstArgument(...);
$argument = str_repeat("old", 2);
echo $firstClass($argument, replaceArgument()), ":", strlen($argument), "|";
$object = new EvalArgumentLifetime();
$argument = str_repeat("old", 2);
echo $object->first(first: $argument, ignored: replaceArgument()), ":", strlen($argument), "|";
$argument = str_repeat("old", 2);
echo $object->first(...[$argument], ignored: replaceArgument()), ":", strlen($argument), "|";
$callback = new EvalArgumentLifetime();
echo $callback("alive", replaceCallback()), ":", is_null($callback);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "oldold:88|oldold:88|oldold:88|oldold:88|oldold:88|alive:1");
}

/// Argument leases and descriptor-array keys leave no per-call native heap owners behind.
#[test]
fn test_core_eval_argument_leases_release_after_native_dispatch() {
    let live = |iterations| {
        let calls = "$argument = str_repeat(\"old\", 2); native_argument_ignore($argument, replaceArgument());"
            .repeat(iterations);
        let source = format!(r#"<?php
function native_argument_ignore(mixed $first, mixed $ignored): void {{}}
$source = 'function replaceArgument() {{ global $argument; $argument = "replacement"; return 0; }}
{calls}
unset($argument); return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42", "{}", output.stderr);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "source argument leases or descriptor keys leaked");
}

/// Eval CUFA arguments release their lease without consuming a native Mixed return.
#[test]
fn test_core_eval_named_invoker_arguments_release_after_return() {
    let live = |iterations| {
        let calls = "$result = call_user_func_array(\"native_argument_identity\", [\"value\" => str_repeat(\"v\", 2)]); unset($result);"
            .repeat(iterations);
        let source = format!(r#"<?php
function native_argument_identity(mixed $value): mixed {{ return $value; }}
$source = '{calls} return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42", "{}", output.stderr);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "named invoker argument or result owners leaked");
}

/// Runtime-selected AOT descriptors balance associative argument and omitted-default owners.
#[test]
fn test_core_aot_named_invoker_arguments_release_after_return() {
    let live = |iterations| {
        let calls = "$result = call_user_func_array($callback, [\"value\" => str_repeat(\"v\", 2)]); echo $result; unset($result);"
            .repeat(iterations);
        let source = format!(r#"<?php
function core_invoker_identity(mixed $value, mixed $unused = null): mixed {{ return $value; }}
function core_invoker_other(mixed $value, mixed $unused = null): mixed {{ return $value; }}
$callback = $argc > 1 ? core_invoker_identity(...) : core_invoker_other(...);
{calls}
unset($callback);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "vv".repeat(iterations), "{}", output.stderr);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "associative descriptor argument or default owners leaked");
}

/// Ref-aware and named builtin adapters keep earlier values alive through later global replacement.
#[test]
fn test_core_eval_builtin_arguments_survive_later_source_replacement() {
    let source = r#"<?php
$source = 'function replaceCallable() { global $callback; $callback = null; return false; }
$callback = str_repeat("strlen", 1);
echo is_callable($callback, replaceCallable(), $name), ":", $name, ":", is_null($callback), "|";
function replaceText() { global $text; $text = "new"; return 2; }
$text = str_repeat("old", 2);
echo str_repeat(string: $text, times: replaceText()), ":", $text;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "1:strlen:1|oldoldoldold:new");
}

/// Named date-alias fallback reuses evaluated arguments instead of executing their side effects twice.
#[test]
fn test_core_eval_named_date_alias_evaluates_arguments_once() {
    let source = r#"<?php
$source = '$calls = 0;
function hourOnce() { global $calls; $calls = $calls + 1; return 0; }
echo gmmktime(hour: hourOnce(), minute: 0, second: 0, month: 1, day: 1, year: 2000), ":", $calls;'
    . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "946684800:1");
}

/// Main eval variables and function globals share storage across successive opaque evaluations.
#[test]
fn test_core_eval_main_globals_share_persistent_scope() {
    let source = r#"<?php
$source = '$counter = 10;
function updateEvalCounter() { global $counter; $counter = $counter + 1; }
updateEvalCounter(); echo $counter;' . ' // ' . $argc;
eval($source);
$source = 'updateEvalCounter(); echo ":", $counter;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "11:12");
}

/// Function-local eval variables stay distinct from globals synchronized with native functions.
#[test]
fn test_core_eval_function_locals_remain_separate_from_globals() {
    let source = r#"<?php
$counter = 10;
function nativeEvalCounter(): int { global $counter; return $counter; }
function runEvalWithLocalCounter(string $source): void {
    $counter = 40;
    eval($source);
    echo ":", $counter;
}
$source = 'function updateEvalGlobalCounter() { global $counter; $counter = $counter + 1; }
updateEvalGlobalCounter(); echo $counter;' . ' // ' . $argc;
runEvalWithLocalCounter($source);
echo "|", nativeEvalCounter();
"#;
    assert_eq!(compile_and_run(source), "40:40|11");
}

/// Reflection constructor, method, and function arrays balance extracted native argument owners.
#[test]
fn test_core_eval_reflection_call_array_arguments_release_after_return() {
    let live = |iterations| {
        let calls = r#"$object = $class->newInstanceArgs([str_repeat("v", 2)]);
$methodResult = $method->invokeArgs($object, ["value" => str_repeat("v", 2)]);
$functionResult = $function->invokeArgs(["value" => str_repeat("v", 2)]);
echo $methodResult, $functionResult;
unset($object, $methodResult, $functionResult);"#.repeat(iterations);
        let source = format!(r#"<?php
class NativeReflectionArguments {{
    public function __construct(mixed $value) {{}}
    public function first(mixed $value): mixed {{ return $value; }}
}}
function nativeReflectionFirst(mixed $value): mixed {{ return $value; }}
$source = '$class = new ReflectionClass("NativeReflectionArguments");
$method = new ReflectionMethod("NativeReflectionArguments", "first");
$function = new ReflectionFunction("nativeReflectionFirst");
{calls}
unset($class, $method, $function); return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, format!("{}42", "vvvv".repeat(iterations)), "{}", output.stderr);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "Reflection call-array argument or result owners leaked");
}

/// A reflected eval exception releases extracted argument cells before propagating to the caller.
#[test]
fn test_core_eval_reflection_call_array_arguments_release_after_throw() {
    let live = |iterations| {
        let calls = r#"try { $function->invokeArgs([str_repeat("v", 2)]); }
catch (RuntimeException $error) { echo "caught"; unset($error); }"#.repeat(iterations);
        let source = format!(r#"<?php
$source = 'function failReflectedArguments($value) {{ throw new RuntimeException("stop"); }}
$function = new ReflectionFunction("failReflectedArguments");
{calls}
unset($function); return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, format!("{}42", "caught".repeat(iterations)), "{}", output.stderr);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "Reflection arguments leaked while propagating an exception");
}

/// Counts live blocks after repeated opaque-eval operations without retaining each operation's output.
fn live_blocks_after_eval_operations(setup: &str, operation: &str, iterations: usize) -> i128 {
    let source = format!(r#"<?php
$source = '{setup}
{}
return 42;' . ' // ' . $argc;
echo eval($source);
"#, operation.repeat(iterations));
    let output = compile_and_run_with_gc_stats(&source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "42", "{}", output.stderr);
    let (allocated, freed) = parse_gc_stats(&output.stderr);
    allocated as i128 - freed as i128
}

/// Repeated eval echo statements release literal and computed string cells after printing.
#[test]
fn test_core_eval_echo_temporary_owners_release_after_output() {
    let live = |iterations| {
        let operations = r#"echo "caught"; echo str_repeat("x", 2);"#.repeat(iterations);
        let source = format!(r#"<?php
$source = '{operations} return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, format!("{}42", "caughtxx".repeat(iterations)));
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "Echo retained temporary output cells");
}

/// Echo releases a separate __toString result while preserving the borrowed receiver's owner.
#[test]
fn test_core_eval_echo_tostring_result_and_receiver_owners() {
    let live = |iterations| {
        let operations = "echo $value;".repeat(iterations);
        let source = format!(r#"<?php
$source = 'class EchoOwner {{
    public function __toString(): string {{ return str_repeat("x", 2); }}
    public function __destruct() {{ echo "drop"; }}
}}
$value = new EchoOwner();
{operations}
echo "before:"; unset($value); return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, format!("{}before:drop42", "xx".repeat(iterations)));
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "Echo retained converted strings or receiver leases");
}

/// A failed string conversion releases the temporary receiver before propagating its exception.
#[test]
fn test_core_eval_echo_throwing_tostring_releases_temporary_receiver() {
    let source = r#"<?php
$source = 'class ThrowingEchoOwner {
    public function __toString(): string { throw new RuntimeException("no output"); }
    public function __destruct() { echo "drop:"; }
}
try { echo new ThrowingEchoOwner(); }
catch (RuntimeException $error) { echo "caught"; unset($error); }' . ' // ' . $argc;
eval($source);
"#;
    let output = compile_and_run_capture(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "drop:caught", "{}", output.stderr);
}

/// Throwable construction and destruction balance owners even when no exception is thrown.
#[test]
fn test_core_eval_throwable_constructor_owners_release_without_throw() {
    let operation = r#"$error = new RuntimeException("stop"); unset($error);"#;
    assert_eq!(
        live_blocks_after_eval_operations("", operation, 5),
        live_blocks_after_eval_operations("", operation, 1),
        "Throwable construction or destruction retained an owner",
    );
}

/// Direct eval exceptions balance owners independently of Reflection's call-array adapter.
#[test]
fn test_core_eval_throwable_catch_owners_release_without_reflection() {
    let setup = r#"function failDirectArguments($value) { throw new RuntimeException("stop"); }"#;
    let operation = r#"try { failDirectArguments(str_repeat("v", 2)); echo "missing throw"; }
catch (RuntimeException $error) { unset($error); }"#;
    assert_eq!(
        live_blocks_after_eval_operations(setup, operation, 5),
        live_blocks_after_eval_operations(setup, operation, 1),
        "Direct eval exception propagation retained an owner",
    );
}

/// Reflected eval-declared functions release extracted argument cells on an ordinary return.
#[test]
fn test_core_eval_reflection_declared_call_array_owners_release_after_return() {
    let setup = r#"function returnReflectedArgument($value) { return $value; }
$function = new ReflectionFunction("returnReflectedArgument");"#;
    let operation = r#"$result = $function->invokeArgs([str_repeat("v", 2)]);
if ($result !== "vv") { echo "invalid return"; }
unset($result);"#;
    assert_eq!(
        live_blocks_after_eval_operations(setup, operation, 5),
        live_blocks_after_eval_operations(setup, operation, 1),
        "Reflected eval return retained an argument or result owner",
    );
}
