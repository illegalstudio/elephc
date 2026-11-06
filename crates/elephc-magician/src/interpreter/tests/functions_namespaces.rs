//! Purpose:
//! Interpreter tests for nested eval, magic constants, namespaces, functions, globals, and argument rules.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases share a persistent eval context when declarations must survive across calls.

use super::super::*;
use super::support::*;

/// Verifies nested eval calls parse and execute against the same dynamic scope.
///
/// The inner fragment is SINGLE-quoted because PHP interpolates a double-quoted eval
/// argument before `eval()` ever sees it: measured on PHP 8.5.6,
/// `$x = 1; eval("$x = $x + 4;");` expands to `eval("1 = 1 + 4;")` and dies with
/// `syntax error, unexpected token "="`. The single-quoted form is the one that yields 5.
#[test]
fn execute_program_nested_eval_uses_same_scope() {
    let program =
        parse_fragment(br#"eval('$x = $x + 4;'); return $x;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(1).expect("create fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}
/// Verifies `__LINE__` inside eval uses the source line within the fragment.
#[test]
fn execute_program_magic_line_uses_fragment_line() {
    let program = parse_fragment(b"\nreturn __LINE__;").expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
}
/// Verifies file-dependent eval magic constants use call-site metadata from the context.
#[test]
fn execute_program_magic_file_and_dir_use_context_call_site() {
    let program =
        parse_fragment(br#"return __FILE__ . "|" . __DIR__;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/main.php", "/tmp", 17);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("/tmp/main.php(17) : eval()'d code|/tmp".to_string())
    );
}
/// Verifies eval class, namespace, and trait magic constants are empty in eval scope.
#[test]
fn execute_program_scope_magic_constants_are_empty_strings() {
    let program =
        parse_fragment(br#"return "[" . __CLASS__ . "|" . __NAMESPACE__ . "|" . __TRAIT__ . "]";"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("[||]".to_string()));
}
/// Verifies eval-declared functions can be called by the same fragment.
#[test]
fn execute_program_calls_declared_function() {
    let program = parse_fragment(br#"function dyn($x) { return $x + 1; } return dyn(4);"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}
/// Verifies eval namespace declarations qualify functions and namespace magic values.
#[test]
fn execute_program_namespace_qualifies_declared_function() {
    let program = parse_fragment(
        br#"namespace Eval\Ns;
function dyn() { return __NAMESPACE__ . ":" . __FUNCTION__; }
return dyn();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("Eval\\Ns:Eval\\Ns\\dyn".to_string())
    );
}
/// Verifies unqualified namespaced calls fall back to global builtins when needed.
#[test]
fn execute_program_namespace_call_falls_back_to_builtin() {
    let program = parse_fragment(br#"namespace Eval\Ns; return strlen("abcd");"#)
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(4));
}
/// Verifies namespaced dynamic functions take precedence over global builtin fallback.
#[test]
fn execute_program_namespace_function_overrides_builtin_fallback() {
    let program = parse_fragment(
        br#"namespace Eval\Ns;
function strlen($value) { return 99; }
return strlen("abcd");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(99));
}
/// Verifies unqualified namespaced constants fall back to global predefined constants.
#[test]
fn execute_program_namespace_const_fetch_falls_back_to_global() {
    let program =
        parse_fragment(br#"namespace Eval\Ns; return PHP_EOL;"#).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("\n".to_string()));
}
/// Verifies namespaced dynamic constants take precedence over global fallback.
#[test]
fn execute_program_namespace_const_fetch_reads_dynamic_constant_first() {
    let program =
        parse_fragment(br#"namespace Eval\Ns; return LOCAL;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let value = values.int(7).expect("create fake int");
    assert!(context.define_constant("Eval\\Ns\\LOCAL", value));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}
/// Verifies eval namespace `use function` imports dispatch to qualified dynamic functions.
#[test]
fn execute_program_namespace_use_function_import_dispatches() {
    let program = parse_fragment(
        br#"namespace Eval\Lib;
function target($x) { return $x + 1; }
namespace Eval\App;
use function Eval\Lib\target as AliasTarget;
return aliastarget(6);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}
/// Verifies eval namespace `use const` imports fetch qualified dynamic constants.
#[test]
fn execute_program_namespace_use_const_import_fetches_dynamic_constant() {
    let program = parse_fragment(
        br#"namespace Eval\App;
use const Eval\Lib\VALUE as LocalValue;
return LocalValue;"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let value = values.int(11).expect("create fake int");
    assert!(context.define_constant("Eval\\Lib\\VALUE", value));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(11));
}
/// Verifies eval grouped namespace imports dispatch dynamic functions and constants.
#[test]
fn execute_program_grouped_namespace_use_imports_dispatch() {
    let program = parse_fragment(
        br#"namespace Eval\Lib;
function target($x) { return $x + 2; }
namespace Eval\App;
use function Eval\Lib\{target as AliasTarget};
use const Eval\Lib\{VALUE as LocalValue};
return AliasTarget(LocalValue);"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let value = values.int(5).expect("create fake int");
    assert!(context.define_constant("Eval\\Lib\\VALUE", value));

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(7));
}
/// Verifies eval-declared functions bind named arguments by parameter name.
#[test]
fn execute_program_calls_declared_function_with_named_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(y: 2, x: 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies eval-declared functions unpack indexed arrays as positional arguments.
#[test]
fn execute_program_calls_declared_function_with_spread_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...[1, 2]);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies string keys unpack as named arguments for eval-declared functions.
#[test]
fn execute_program_calls_declared_function_with_named_spread_args() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...["y" => 2], x: 1);"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(12));
}
/// Verifies eval-declared function static locals persist between calls.
#[test]
fn execute_program_static_var_persists_in_declared_function() {
    let program = parse_fragment(
        br#"function dyn() { for ($i = 0; $i < 2; $i++) { static $n = 0; $n++; } return $n; }
return (dyn() * 10) + dyn();"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(24));
}
/// Verifies top-level eval static declarations reinitialize on each eval execution.
#[test]
fn execute_program_top_level_static_var_reinitializes_per_eval() {
    let program =
        parse_fragment(br#"static $n = 0; $n++; return $n;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let first = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute first eval ir");
    let second = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute second eval ir");

    assert_eq!(values.get(first), FakeValue::Int(1));
    assert_eq!(values.get(second), FakeValue::Int(1));
}
/// Verifies `global` declarations read and write the context global scope.
#[test]
fn execute_program_global_alias_writes_context_global_scope() {
    let program =
        parse_fragment(br#"global $g; $g = $g + 1; return $g;"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut global_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let initial = values.int(1).expect("allocate initial global");
    global_scope.set("g", initial, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut global_scope);

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    let global = global_scope
        .visible_cell("g")
        .expect("global scope should contain g");
    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(values.get(global), FakeValue::Int(2));
}
/// Verifies references to global aliases write the source global variable.
#[test]
fn execute_program_reference_alias_to_global_updates_source_global() {
    let program = parse_fragment(br#"global $g; $alias =& $g; $alias = 4; return $g;"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut global_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let initial = values.int(1).expect("allocate initial global");
    global_scope.set("g", initial, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut global_scope);

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    let global = global_scope
        .visible_cell("g")
        .expect("global scope should contain g");
    assert_eq!(values.get(result), FakeValue::Int(4));
    assert_eq!(values.get(global), FakeValue::Int(4));
    assert!(global_scope.visible_cell("alias").is_none());
}

/// Verifies superglobals resolve through the context global scope from a function-like scope
/// while an eval call is executing, with no `global` statement, as they do in PHP (#935).
///
/// The compiled program synchronizes the superglobals it names into that global scope, so a
/// read sees its value, a write lands there for the reload, and a CLI superglobal the fragment
/// names but nobody created (`$_ENV`) is seeded there rather than in the local scope.
#[test]
fn execute_program_superglobals_resolve_through_the_global_scope() {
    let program = parse_fragment(
        br#"echo $_SERVER["k"]; $_SERVER["w"] = "x"; return is_array($_ENV);"#,
    )
    .expect("parse eval fragment");
    let reread = parse_fragment(br#"return $_SERVER["w"];"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut global_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let server = values.assoc_new(1).expect("allocate server array");
    let key = values.string("k").expect("allocate key");
    let value = values.string("v").expect("allocate value");
    let server = values.array_set(server, key, value).expect("seed server array");
    global_scope.set("_SERVER", server, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut global_scope);
    context.push_eval_backtrace_boundary();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");
    let mut other_scope = ElephcEvalScope::new();
    let written = execute_program_with_context(&mut context, &reread, &mut other_scope, &mut values)
        .expect("execute second eval ir");
    context.pop_eval_backtrace_boundary();

    assert_eq!(values.output, "v");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    assert_eq!(values.get(written), FakeValue::String("x".to_string()));
    assert!(global_scope.visible_cell("_ENV").is_some());
    assert!(scope.visible_cell("_ENV").is_none());
    assert!(scope.visible_cell("_SERVER").is_none());
}

/// Verifies `unset()` of a superglobal inside an eval call reaches the global scope its reads
/// and writes resolve through, and that no later fragment creates the superglobal again.
///
/// PHP unsets the global itself from any scope and creates each auto-global once per request.
/// A local-only unset left the global scope's value in place, so the very next read saw it; a
/// CLI superglobal eval created would also have been created again by the next fragment that
/// named it.
#[test]
fn execute_program_superglobal_unset_reaches_the_global_scope() {
    /// Forgets the recorded superglobal unsets when the test ends, even on a failed assertion.
    struct ResetUnsetSuperglobals;
    impl Drop for ResetUnsetSuperglobals {
        /// Clears this thread's record so later tests still see superglobals created.
        fn drop(&mut self) {
            reset_unset_superglobals();
        }
    }
    let _reset = ResetUnsetSuperglobals;
    let unset_get = parse_fragment(br#"unset($_GET); return isset($_GET);"#).expect("parse eval fragment");
    let read_get = parse_fragment(br#"return isset($_GET);"#).expect("parse eval fragment");
    let unset_server =
        parse_fragment(br#"unset($_SERVER); return isset($_SERVER);"#).expect("parse eval fragment");
    let read_server = parse_fragment(br#"return isset($_SERVER);"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut global_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let get = values.assoc_new(0).expect("allocate the program's $_GET");
    global_scope.set("_GET", get, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut global_scope);
    context.push_eval_backtrace_boundary();

    let mut results = Vec::new();
    for program in [&unset_get, &read_get, &unset_server, &read_server] {
        let mut scope = ElephcEvalScope::new();
        results.push(
            execute_program_with_context(&mut context, program, &mut scope, &mut values)
                .expect("execute eval ir"),
        );
    }
    context.pop_eval_backtrace_boundary();

    for result in results {
        assert_eq!(values.get(result), FakeValue::Bool(false));
    }
    assert!(global_scope.visible_cell("_GET").is_none());
    assert!(global_scope.visible_cell("_SERVER").is_none());
}

/// Verifies an unset superglobal stays undefined in a fresh eval context on the same thread.
///
/// elephc gives each function frame that calls `eval()` its own context, but all of them run
/// in one PHP request, where an unset auto-global is never created again: after
/// `function a() { eval('unset($_SERVER);'); }`, PHP 8.5.10 answers `false` to `isset($_SERVER)`
/// in a later function's eval and in a top-level eval alike.
#[test]
fn execute_program_superglobal_unset_holds_in_a_fresh_context() {
    /// Forgets the recorded superglobal unsets when the test ends, even on a failed assertion.
    struct ResetUnsetSuperglobals;
    impl Drop for ResetUnsetSuperglobals {
        /// Clears this thread's record so later tests still see superglobals created.
        fn drop(&mut self) {
            reset_unset_superglobals();
        }
    }
    let _reset = ResetUnsetSuperglobals;
    let unset = parse_fragment(br#"unset($_SERVER); return isset($_SERVER);"#).expect("parse eval fragment");
    let read = parse_fragment(br#"return isset($_SERVER);"#).expect("parse eval fragment");
    let mut values = FakeOps::default();

    let mut first_context = ElephcEvalContext::new();
    let mut first_scope = ElephcEvalScope::new();
    let unset_result =
        execute_program_with_context(&mut first_context, &unset, &mut first_scope, &mut values)
            .expect("execute eval ir");
    let mut fresh_context = ElephcEvalContext::new();
    let mut fresh_scope = ElephcEvalScope::new();
    let read_result =
        execute_program_with_context(&mut fresh_context, &read, &mut fresh_scope, &mut values)
            .expect("execute eval ir");

    assert_eq!(values.get(unset_result), FakeValue::Bool(false));
    assert_eq!(values.get(read_result), FakeValue::Bool(false));
    assert!(fresh_scope.visible_cell("_SERVER").is_none());
}

/// Verifies a context outside an executing eval call keeps superglobals local.
///
/// A callback can run in a context retained past the native frame that installed its global
/// scope, so the redirect is limited to executing eval calls: here the global scope's
/// `$_SERVER` is left alone and the fragment gets its own CLI `$_SERVER`.
#[test]
fn execute_program_superglobals_stay_local_outside_an_eval_call() {
    let program = parse_fragment(br#"return $_SERVER["k"] ?? "absent";"#)
        .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut global_scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let server = values.assoc_new(1).expect("allocate server array");
    let key = values.string("k").expect("allocate key");
    let value = values.string("v").expect("allocate value");
    let server = values.array_set(server, key, value).expect("seed server array");
    global_scope.set("_SERVER", server, ScopeCellOwnership::Owned);
    context.set_global_scope(&mut global_scope);

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::String("absent".to_string()));
    assert!(scope.visible_cell("_SERVER").is_some());
}
/// Verifies named calls reject positional arguments that follow named arguments.
#[test]
fn execute_program_rejects_positional_after_named_arg() {
    let program = parse_fragment(
        br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, print "late");"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert_eq!(values.output, "");
}
/// Verifies named calls reject argument unpacking after named arguments.
#[test]
fn execute_program_rejects_spread_after_named_arg() {
    let program =
        parse_fragment(br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, ...[2]);"#)
            .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values);

    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
}
/// Verifies function-scope magic constants keep the eval declaration spelling.
#[test]
fn execute_program_magic_function_and_method_use_eval_declared_name() {
    let program = parse_fragment(
            br#"function DynMagicCase() { return __FUNCTION__ . ":" . __METHOD__; } return dynmagiccase();"#,
        )
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.get(result),
        FakeValue::String("DynMagicCase:DynMagicCase".to_string())
    );
}
/// Verifies eval-declared functions persist in a shared eval context.
#[test]
fn execute_program_context_keeps_declared_function() {
    let define =
        parse_fragment(br#"function dyn($x) { return $x + 1; }"#).expect("parse eval fragment");
    let call = parse_fragment(br#"return dyn(4);"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
        .expect("execute eval ir");
    let result = execute_program_with_context(&mut context, &call, &mut scope, &mut values)
        .expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(5));
}
