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
