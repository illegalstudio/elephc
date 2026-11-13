//! Purpose:
//! Verifies semantic profile boundaries in eval execution.
//!
//! Called from:
//! - Focused Magician interpreter unit tests.
//!
//! Key details:
//! - Uses the fake runtime to observe failures without loading native libraries.

use super::super::*;
use super::support::*;
use elephc_semantics::{SemanticProfile, StrictFeature};

/// Runtime source cannot bypass the strict boundary through the interpreter API.
#[test]
fn strict_profile_dynamic_code_blocks_nested_eval_and_include() {
    for source in [b"eval('return 1;');".as_slice(), b"include '/must-not-read-strict-source.php';"] {
        let program = crate::parser::parse_fragment_with_semantics(source, SemanticProfile::PERMISSIVE.with(StrictFeature::DynamicCode, true)).unwrap();
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::DynamicCode, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable));
        assert!(values.output.is_empty());
    }
}

/// An external host cannot execute with a profile whose guarantees are still unimplemented.
#[test]
fn strict_profile_pending_guarantees_cannot_execute_through_interpreter_api() {
    let program = crate::parser::parse_fragment_with_semantics(b"echo 'must not execute';", SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
        Err(EvalStatus::UnsupportedConstruct));
    assert!(values.output.is_empty());
}

/// Runs the same boolean rules through both execution engines with explicit cleanup.
#[test]
fn strict_profile_bools_walker_and_bytecode_require_boolean_conditions() {
    for source in [
        "if (1) { echo 'bad'; }", "echo !1;", "echo 1 ?: false;",
        "echo true && 1;", "echo true xor 1;", "while (1) { break; }",
    ] {
        for walker in [true, false] {
            let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let result = if walker {
                execute_program_statements(&program, &mut context, &mut scope, &mut values).map(|_| ())
            } else {
                execute_program_with_context(&mut context, &program, &mut scope, &mut values).map(|_| ())
            };
            assert_eq!(result, Err(EvalStatus::UncaughtThrowable), "{source}, walker={walker}");
            assert!(values.output.is_empty(), "{source}");
            assert!(context.take_pending_throw().is_some());
        }
    }
}

/// Explicit casts, boolean short circuits, and callback-free data filtering remain usable.
#[test]
fn strict_profile_bools_preserves_explicit_casts_and_boolean_callbacks() {
    let program = crate::parser::parse_fragment_with_semantics(b"if ((bool) 1) { echo 'B'; } echo !false; echo false && 1; echo count(array_filter([0, 1, 2]));", SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
    let mut context = ElephcEvalContext::new();
    context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "B12");
}

/// Eval callback predicates reject nonboolean results through the ordinary Throwable channel.
#[test]
fn strict_profile_bools_callback_results_are_checked_in_eval() {
    for source in [
        "function strict_eval_predicate($value) { return $value; } array_filter([1], 'strict_eval_predicate');",
        "function strict_eval_predicate() { return 1; } iterator_apply($it, 'strict_eval_predicate');",
    ] {
        let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let iterator = values.alloc(FakeValue::Iterator { len: 1, position: 0 });
        scope.set("it", iterator, crate::scope::ScopeCellOwnership::Borrowed);
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values), Err(EvalStatus::UncaughtThrowable));
        assert!(context.take_pending_throw().is_some());
    }
}

/// Simple interpolation routes textual keys through PHP key normalization without losing bytes.
#[test]
fn strict_integer_literal_eval_text_keys_preserve_php_bytes() {
    let program = parse_fragment(br#"$items = [0 => 'zero', '1' => 'one', '01' => 'leading', '-0' => 'negative', '9223372036854775808' => 'large']; echo "$items[01]|$items[-0]|$items[9223372036854775808]";"#).unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).unwrap();
    assert_eq!(values.output, "leading|negative|large");
}

/// Pre-parsed host programs cannot enter a context with a different normalization policy.
#[test]
fn strict_profile_preparsed_host_programs_require_parser_provenance() {
    let strict = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
    for (parser_profile, context_profile) in [(SemanticProfile::PERMISSIVE, strict), (strict, SemanticProfile::PERMISSIVE)] {
        let program = crate::parser::parse_fragment_with_semantics(b"echo 'must not execute';", parser_profile).unwrap();
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(context_profile).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values), Err(EvalStatus::AbiMismatch));
        assert_eq!(execute_program_statements(&program, &mut context, &mut scope, &mut values).map(|_| ()), Err(EvalStatus::AbiMismatch));
        assert!(values.output.is_empty());
    }
}

/// Native hook policy and context policy must match before output, allocation or declarations.
#[test]
fn strict_profile_native_hooks_require_matching_semantic_policy() {
    let strict = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
    for (runtime, context_profile) in [(SemanticProfile::PERMISSIVE, strict), (strict, SemanticProfile::PERMISSIVE), (strict, strict)] {
        let program = crate::parser::parse_fragment_with_semantics(b"echo 'ok';", context_profile).unwrap();
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(context_profile).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps { fixed_profile: Some(runtime), ..Default::default() };
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
        if runtime == context_profile {
            result.unwrap();
            assert_eq!(values.output, "ok");
        } else {
            assert_eq!(result, Err(EvalStatus::AbiMismatch));
            assert!(values.values.is_empty() && values.output.is_empty());
        }
    }
}


/// Exercises staged numeric consumers without enabling an incomplete profile at host entry points.
fn staged_numeric_execution(
    source: &str, vm: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut FakeOps,
) -> Result<EvalControl, EvalStatus> {
    let program = crate::parser::parse_fragment_with_semantics(source.as_bytes(), context.semantic_profile()).unwrap();
    if vm {
        let module = bytecode::compile_program(&program).expect("numeric fixture fits the VM");
        bytecode::execute_module(&module, context, scope, values).map(|(control, _)| control)
    } else {
        execute_statements(program.statements(), context, scope, values)
    }
}

/// Both eval consumers preserve exact integer kinds and transfer checked failures before storing.
#[test]
fn strict_eval_numeric_staged_walker_vm_values_and_failure_owners() {
    for (source, expected) in [
        ("return 9007199254740993 + 0;", Some(FakeValue::Int(9007199254740993))),
        ("return 2 ** 62;", Some(FakeValue::Int(4611686018427387904))),
        ("return 2 ** -1;", Some(FakeValue::Float(0.5))),
        ("return 2.0 ** 63;", Some(FakeValue::Float(9223372036854775808.0))),
        ("return '9007199254740993' + 0;", Some(FakeValue::Int(9007199254740993))),
        ("return true + null;", Some(FakeValue::Int(1))),
        ("return 9223372036854775807 + 1;", None),
        ("return -9223372036854775808 - 1;", None),
        ("return 9223372036854775807 * 2;", None),
        ("return 2 ** 63;", None),
        ("$minimum = -9223372036854775808; return -$minimum;", None),
        ("return '9223372036854775807' + 1;", None),
        ("$saved = 9223372036854775807; $saved += 1; return $saved;", None),
    ] {
        for vm in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let outcome = staged_numeric_execution(source, vm, &mut context, &mut scope, &mut values);
            match (&expected, outcome) {
                (Some(expected), Ok(EvalControl::Return(value))) => {
                    assert_eq!(&values.get(value), expected, "{source}, vm={vm}");
                    values.release(value).unwrap();
                }
                (None, Err(EvalStatus::UncaughtThrowable)) => {
                    let error = context.take_pending_throw().expect("checked exception owner");
                    assert!(values.object_is_a(error, "ArithmeticError", false).unwrap(), "{source}");
                    values.release(error).unwrap();
                    if source.contains("$saved") {
                        assert_eq!(values.get(scope.visible_cell("saved").unwrap()), FakeValue::Int(i64::MAX));
                    }
                }
                _ => panic!("unexpected checked numeric outcome: {source}, vm={vm}"),
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, count) in &values.cell_owners {
                assert_eq!(*count, 0, "{source}, vm={vm}: {:?}", values.values[id]);
            }
        }
    }
}

/// Public builtin dispatch selects checked abs/pow while PHP implementation hooks stay permissive.
#[test]
fn strict_eval_numeric_staged_builtin_boundaries_and_php_controls() {
    for source in [
        "return abs(-9223372036854775808);",
        "return pow(2, 63);",
        "$callable = 'pow'; return $callable(2, 63);",
        "$callable = pow(...); return $callable(2, 63);",
        r"return \PoW(2, 63);",
        "return call_user_func('abs', -9223372036854775808);",
        "return call_user_func_array('pow', [2, 63]);",
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert!(matches!(staged_numeric_execution(source, false, &mut context, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable)), "{source}");
        let error = context.take_pending_throw().expect("builtin checked exception owner");
        assert!(values.object_is_a(error, "ArithmeticError", false).unwrap(), "{source}");
        values.release(error).unwrap();
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for (id, count) in &values.cell_owners { assert_eq!(*count, 0, "{source}: {:?}", values.values[id]); }
    }
    let mut values = FakeOps::default();
    let maximum = values.int(i64::MAX).unwrap();
    let one = values.int(1).unwrap();
    let php = values.add(maximum, one).unwrap();
    assert_eq!(values.get(php), FakeValue::Float(9223372036854775808.0));
    for value in [maximum, one, php] { values.release(value).unwrap(); }
}


/// Array reductions use checked kernels on every builtin route and retire failed argument arrays.
#[test]
fn strict_eval_reductions_staged_calls_kinds_and_ownership() {
    for (source, expected) in [
        ("return array_sum([2, 3]);", Some(FakeValue::Int(5))),
        ("return array_product([2, 3]);", Some(FakeValue::Int(6))),
        ("return array_sum([]);", Some(FakeValue::Int(0))),
        ("return array_product([]);", Some(FakeValue::Int(1))),
        ("return array_sum(['9007199254740993', 0]);", Some(FakeValue::Int(9007199254740993))),
        ("return array_sum([1.0, 9223372036854775807, 1]);", Some(FakeValue::Float(9223372036854775808.0))),
        ("return array_sum([9223372036854775807, 1]);", None),
        ("return array_product([9223372036854775807, 2]);", None),
        (r"return \ARRAY_SUM([9223372036854775807, 1]);", None),
        ("$sum = array_sum(...); return $sum([9223372036854775807, 1]);", None),
        ("$name = 'array_product'; return $name([9223372036854775807, 2]);", None),
        ("return call_user_func('array_sum', [9223372036854775807, 1]);", None),
        ("return call_user_func_array('array_product', [[9223372036854775807, 2]]);", None),
    ] {
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        match (staged_numeric_execution(source, false, &mut context, &mut scope, &mut values), expected) {
            (Ok(EvalControl::Return(value)), Some(expected)) => {
                assert_eq!(values.get(value), expected, "{source}");
                values.release(value).unwrap();
            }
            (Err(EvalStatus::UncaughtThrowable), None) => {
                let error = context.take_pending_throw().expect("reduction failure owns its Throwable");
                assert!(values.object_is_a(error, "ArithmeticError", false).unwrap(), "{source}");
                values.release(error).unwrap();
            }
            _ => panic!("unexpected reduction result: {source}"),
        }
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for (id, count) in &values.cell_owners { assert_eq!(*count, 0, "{source}: {:?}", values.values[id]); }
    }
}


/// Scalar updates check before replacement and preserve string domains in both eval consumers.
#[test]
fn strict_eval_updates_staged_walker_vm_values_and_owners() {
    for (initial, update, expected) in [
        ("10", "$value++;", Some(FakeValue::Int(11))),
        ("10", "--$value;", Some(FakeValue::Int(9))),
        ("1.5", "$value++;", Some(FakeValue::Float(2.5))),
        ("'1.5'", "$value--;", Some(FakeValue::Float(0.5))),
        ("'az'", "$value++;", Some(FakeValue::String("ba".into()))),
        ("'abc'", "$value--;", Some(FakeValue::String("abc".into()))),
        ("''", "$value++;", Some(FakeValue::String("1".into()))),
        ("''", "$value--;", Some(FakeValue::Int(-1))),
        ("null", "$value--;", Some(FakeValue::Null)),
        ("9223372036854775807", "$value++;", None),
        ("-9223372036854775808", "--$value;", None),
        ("'9223372036854775807'", "$value++;", None),
    ] {
        let source = format!("$value = {initial}; {update} return $value;");
        for vm in [false, true] {
            let mut context = ElephcEvalContext::new();
            context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            match (staged_numeric_execution(&source, vm, &mut context, &mut scope, &mut values), &expected) {
                (Ok(EvalControl::Return(value)), Some(expected)) => {
                    assert_eq!(&values.get(value), expected, "{source}, vm={vm}");
                    values.release(value).unwrap();
                    if initial == "null" { assert_eq!(values.warnings, [elephc_semantics::numeric::NULL_DECREMENT_WARNING]); }
                }
                (Err(EvalStatus::UncaughtThrowable), None) => {
                    let error = context.take_pending_throw().expect("update exception owner");
                    assert!(values.object_is_a(error, "ArithmeticError", false).unwrap());
                    values.release(error).unwrap();
                    let expected = if initial.starts_with('\'') { FakeValue::String("9223372036854775807".into()) }
                        else { FakeValue::Int(if initial.starts_with('-') { i64::MIN } else { i64::MAX }) };
                    assert_eq!(values.get(scope.visible_cell("value").unwrap()), expected, "{source}, vm={vm}");
                }
                _ => panic!("unexpected update result: {source}, vm={vm}"),
            }
            for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
            for (id, count) in &values.cell_owners { assert_eq!(*count, 0, "{source}, vm={vm}: {:?}", values.values[id]); }
        }
    }
}


/// By-reference eval calls cannot publish an overflowing update through their caller's slot.
#[test]
fn strict_eval_updates_staged_reference_writeback_preserves_original() {
    for (initial, operation, expected) in [
        ("9223372036854775807", "$argument++;", FakeValue::Int(i64::MAX)),
        ("-9223372036854775808", "--$argument;", FakeValue::Int(i64::MIN)),
        ("'9223372036854775807'", "$argument++;", FakeValue::String("9223372036854775807".into())),
    ] {
        let source = format!("function updateStrictReference(&$argument) {{ {operation} }} $value = {initial}; updateStrictReference($value);");
        let mut context = ElephcEvalContext::new();
        context.install_semantic_profile(SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        assert!(matches!(staged_numeric_execution(&source, false, &mut context, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable)), "{source}");
        let error = context.take_pending_throw().expect("reference update failure owner");
        assert!(values.object_is_a(error, "ArithmeticError", false).unwrap());
        values.release(error).unwrap();
        assert_eq!(values.get(scope.visible_cell("value").unwrap()), expected, "{source}");
        for value in scope.drain_owned_cells() { values.release(value).unwrap(); }
        for (id, count) in &values.cell_owners { assert_eq!(*count, 0, "{source}: {:?}", values.values[id]); }
    }
}
