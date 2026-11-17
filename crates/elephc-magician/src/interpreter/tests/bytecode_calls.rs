//! Purpose:
//! Checks bytecode call guards against the shared source-level dispatch rules.
//!
//! Called from:
//! - The Magician library tests with the `bytecode_slice_call_` filter.
//!
//! Key details:
//! - Unreachable declarations admit call shapes without replacing retained callees.
//! - Slow calls must resolve before argument effects and preserve reference targets.

use super::super::*;
use super::support::*;

/// A matching declaration cannot override builtin precedence, even across executable cache hits.
#[test]
fn bytecode_slice_call_preserves_builtin_precedence() {
    for declaration in [
        "function strlen($v) { return 9; }",
        "if (false) { function strlen($v) { return 9; } }",
    ] {
        let source = format!("{declaration} return sTrLeN('hi');");
        for _ in 0..2 {
            let mut context = ElephcEvalContext::new();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let program = parse_fragment(source.as_bytes()).unwrap();
            assert!(bytecode::compile_program(&program).is_ok());
            let result = with_executable_cache_key(source.as_bytes(), || {
                execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            }).unwrap();
            assert!(bytecode::last_execution_used_vm());
            assert_eq!(values.get(result), FakeValue::Int(2));
        }
    }
}

/// Retained by-reference signatures keep the original caller variables at a slow call site.
#[test]
fn bytecode_slice_call_preserves_by_reference_arguments() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let setup = parse_fragment(b"
function bytecode_swap(&$a, &$b) { $saved = $a; $a = $b; $b = $saved; }
").unwrap();
    execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
    let program = parse_fragment(b"
if (false) { function bytecode_swap($a, $b) { return 0; } }
$x = 1; $y = 2; bytecode_swap($x, $y); return $x * 10 + $y;
").unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert!(bytecode::last_execution_used_vm());
    assert_eq!(values.get(result), FakeValue::Int(21));
}

/// Resolving an undefined callee fails before any argument function can emit output.
#[test]
fn bytecode_slice_call_resolves_before_argument_effects() {
    let program = parse_fragment(b"
if (false) { function bytecode_absent($x) { return $x; } }
function bytecode_side() { echo 'S'; return 1; }
return bytecode_absent(bytecode_side());
").unwrap();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let outcome = execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let EvalOutcome::Throwable(error) = outcome else { panic!("expected undefined-function error"); };
    assert!(values.object_is_a(error, "Error", false).unwrap());
    assert!(values.output.is_empty());
    assert!(context.backtrace_frames().is_empty());
}

/// A skipped ordinary declaration leaves a retained generic template available for dispatch.
#[test]
fn bytecode_slice_call_dispatches_retained_generic_template() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let setup = parse_fragment(b"function bytecode_generic<T>(T $x): T { return $x; }").unwrap();
    execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
    let program = parse_fragment(b"
if (false) { function bytecode_generic($x) { return 0; } }
return bytecode_generic(42);
").unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Cached executable images resolve native availability in the current context on each call.
#[test]
fn bytecode_slice_call_dispatches_retained_native_function() {
    let source = b"if (false) { function bytecode_native($x) { return 0; } } return bytecode_native(42);";
    let program = parse_fragment(source).unwrap();
    for available in [true, false] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).unwrap();
        if available {
            let native = NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 1);
            assert!(context.define_native_function("bytecode_native", native).is_ok());
        }
        let outcome = with_executable_cache_key(source, || {
            execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values)
        }).unwrap();
        assert!(bytecode::last_execution_used_vm());
        match outcome {
            EvalOutcome::Value(result) => {
                assert!(available);
                assert_eq!(values.get(result), FakeValue::Int(42));
            }
            EvalOutcome::Throwable(error) => {
                assert!(!available);
                assert!(values.object_is_a(error, "Error", false).unwrap());
            }
        }
    }
}

/// Builtin arity errors evaluate each argument exactly once before raising, as PHP does.
#[test]
fn bytecode_slice_call_preserves_arity_argument_effects() {
    let program = parse_fragment(b"
if (false) { function strlen($a, $b) { return 0; } }
function bytecode_arity_side() { echo 'S'; return 'x'; }
return strlen(bytecode_arity_side(), bytecode_arity_side());
").unwrap();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert!(bytecode::compile_program(&program).is_ok());
    let outcome = execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let EvalOutcome::Throwable(error) = outcome else { panic!("expected builtin arity error"); };
    assert!(values.object_is_a(error, "ArgumentCountError", false).unwrap());
    assert_eq!(values.output, "SS");
}

/// Nested argument calls preserve each previously selected callee on the explicit VM stack.
#[test]
fn bytecode_slice_call_keeps_nested_prepared_targets() {
    let program = parse_fragment(b"
function bytecode_sum($a, $b) { return $a + $b; }
function bytecode_id($x) { return $x; }
return bytecode_sum(bytecode_id(20), bytecode_sum(1, bytecode_id(21)));
").unwrap();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let module = bytecode::compile_program(&program).unwrap();
    let (control, stats) = bytecode::execute_module(&module, &mut context, &mut scope, &mut values).unwrap();
    let EvalControl::Return(result) = control else { panic!("expected call result"); };
    assert_eq!(values.get(result), FakeValue::Int(42));
    assert_eq!(stats.max_frames, 2);
    assert!(context.backtrace_frames().is_empty());
}

/// A retained NoDiscard declaration warns only for a statement that ignores its result.
#[test]
fn bytecode_slice_call_warns_for_retained_nodiscard() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let setup = parse_fragment(b"#[NoDiscard] function bytecode_marked(): int { return 7; }").unwrap();
    execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
    let program = parse_fragment(b"
if (false) { function bytecode_marked() { return 0; } }
bytecode_marked();
$_ = bytecode_marked();
$used = bytecode_marked();
$elvis = bytecode_marked() ?: 1;
return $used + $elvis;
").unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert!(bytecode::last_execution_used_vm());
    assert_eq!(values.get(result), FakeValue::Int(14));
    assert_eq!(values.warnings.len(), 1);
    assert!(values.warnings[0].contains("The return value of bytecode_marked() should either be used or explicitly ignored"));
}

/// A throwing warning handler still consumes the discarded function's returned owner.
#[test]
fn bytecode_slice_call_releases_result_when_nodiscard_throws() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let setup = parse_fragment(b"
#[NoDiscard] function bytecode_warning_result(): int { return 7; }
set_error_handler(function($level, $message) { throw new Exception($message); });
").unwrap();
    execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
    let program = parse_fragment(b"
if (false) { function bytecode_warning_result() { return 0; } }
bytecode_warning_result();
").unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    let outcome = execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let EvalOutcome::Throwable(error) = outcome else { panic!("expected warning-handler exception"); };
    assert!(values.object_is_a(error, "Exception", false).unwrap());
    assert!(context.backtrace_frames().is_empty());
    let results = values.values.iter().filter(|(_, value)| **value == FakeValue::Int(7)).collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    assert_eq!(values.cell_owners.get(results[0].0), Some(&0));
}
