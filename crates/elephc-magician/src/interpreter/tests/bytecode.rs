//! Purpose:
//! Tests the EvalIR bytecode slice: named gaps, the executable cache, explicit frames,
//! and a compiled body reached from the tree walker.
//!
//! Called from:
//! - `cargo test -p elephc-magician bytecode_slice`.
//!
//! Key details:
//! - Recursion is deep enough that a Rust call per frame would abort the test process.
//! - The slice flag is thread-local, so parallel tests do not share it.

use super::super::*;
use super::support::*;

/// Runs a straight-line fragment on the slice and returns the echoed value.
#[test]
fn bytecode_slice_runs_arithmetic_on_the_vm() {
    let program = parse_fragment(b"$x = 2; $x = $x + 3; echo $x; return $x * 4;").expect("parse");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program(&program, &mut scope, &mut values).expect("execute");
    assert!(bytecode::last_execution_used_vm());
    assert_eq!(values.output, "5");
    assert_eq!(values.get(result), FakeValue::Int(20));
}

/// Names a property read instead of executing part of the fragment.
#[test]
fn bytecode_slice_names_the_unsupported_variant() {
    let program = parse_fragment(b"$value = $box->name;").expect("parse");
    let gap = bytecode::compile_program(&program).expect_err("property read is a gap");
    assert_eq!(gap.what(), "PropertyGet");
    assert_eq!(gap.to_string(), "bytecode compile: unsupported PropertyGet");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let _ = execute_program(&program, &mut scope, &mut values);
    assert!(!bytecode::last_execution_used_vm());
}

/// Reuses one compiled module for the same source, and remembers a gap.
#[test]
fn bytecode_slice_caches_the_executable_form() {
    let positive = b"$n = 1; echo $n;";
    let positive_program = parse_fragment(positive).expect("parse");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    with_executable_cache_key(positive, || {
        execute_program(&positive_program, &mut scope, &mut values).expect("execute");
    });
    let first = bytecode::lookup_executable(positive).expect("cached module").expect("module");
    with_executable_cache_key(positive, || {
        execute_program(&positive_program, &mut scope, &mut values).expect("execute again");
    });
    let second = bytecode::lookup_executable(positive).expect("cached module").expect("module");
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(values.output, "11");

    let negative = b"$value = $box->name;";
    let negative_program = parse_fragment(negative).expect("parse");
    with_executable_cache_key(negative, || {
        let _ = execute_program(&negative_program, &mut scope, &mut values);
    });
    let gap = bytecode::lookup_executable(negative).expect("cached gap").expect_err("gap");
    assert_eq!(gap.what(), "PropertyGet");
    assert!(!bytecode::last_execution_used_vm());
}

/// Recurses through explicit frames instead of one Rust call per PHP call.
#[test]
fn bytecode_slice_recursion_keeps_frames_on_the_vm_stack() {
    let program = parse_fragment(
        b"function rec($n) { if ($n) { return rec($n - 1); } return 1; } return rec(1500);",
    )
    .expect("parse");
    let module = bytecode::compile_program(&program).expect("recursion fits the slice");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let (control, stats) = bytecode::execute_module(&module, &mut context, &mut scope, &mut values)
        .expect("deep recursion");
    let EvalControl::Return(result) = control else {
        panic!("expected a returned value");
    };
    assert!(stats.max_frames >= 1500, "max frames {}", stats.max_frames);
    assert_eq!(values.get(result), FakeValue::Int(1));
}

/// A later fragment stays on the walker and still runs the compiled callee.
#[test]
fn bytecode_slice_walker_enters_a_compiled_body() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let definition = parse_fragment(b"function add($a, $b) { return $a + $b; }").expect("parse");
    execute_program_with_context(&mut context, &definition, &mut scope, &mut values).expect("define");
    assert!(context.function("add").expect("add").bytecode().is_some());

    let call = parse_fragment(b"return add(20, 22);").expect("parse");
    let result = execute_program_with_context(&mut context, &call, &mut scope, &mut values).expect("call");
    assert!(!bytecode::last_execution_used_vm());
    assert_eq!(values.get(result), FakeValue::Int(42));
}

/// Preserves object adapters when a later fragment contains only output or a scalar cast.
#[test]
fn bytecode_slice_preserves_object_string_contexts() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let setup = parse_fragment(br#"
class BytecodeStringBox {
    public function __toString() { return "converted"; }
}
$box = new BytecodeStringBox();
"#).expect("parse object setup");
    let result = execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, result).unwrap();
    for source in ["echo $box;", "print $box;", "echo (string) $box;"] {
        let program = parse_fragment(source.as_bytes()).unwrap();
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        assert!(bytecode::last_execution_used_vm(), "{source}");
        eval_release_value(&mut context, &mut values, result).unwrap();
    }
    assert_eq!(values.output, "convertedconvertedconverted");
}

/// Releases fallback source arguments after both a successful call and a binding error.
#[test]
fn bytecode_slice_releases_fallback_argument_owners() {
    for (definition, succeeds) in [
        ("function bytecode_identity($x = 0) { return $x; }", true),
        ("function bytecode_identity($x, $y) { return $x; }", false),
    ] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let setup = parse_fragment(definition.as_bytes()).unwrap();
        let result = execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
        eval_release_value(&mut context, &mut values, result).unwrap();
        // The unreachable declaration admits this call shape; the retained function wins.
        let program = parse_fragment(br#"
if (false) { function bytecode_identity($x) { return $x; } }
return bytecode_identity("argument");
"#).unwrap();
        assert!(bytecode::compile_program(&program).is_ok());
        let result = execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        let result = match result {
            EvalOutcome::Value(value) => {
                assert!(succeeds);
                assert_eq!(values.get(value), FakeValue::String("argument".into()));
                value
            }
            EvalOutcome::Throwable(error) => {
                assert!(!succeeds);
                assert!(values.object_is_a(error, "ArgumentCountError", false).unwrap());
                error
            }
        };
        eval_release_value(&mut context, &mut values, result).unwrap();
        assert!(context.backtrace_frames().is_empty());
        for (id, value) in &values.values {
            if value == &FakeValue::String("argument".into()) {
                assert_eq!(values.cell_owners.get(id), Some(&0), "{definition}");
            }
        }
    }
}

/// Retires popped operands and results even when the first source release throws.
#[test]
fn bytecode_slice_balances_failed_operand_cleanup() {
    for (source, failing_release) in [
        ("return 2 + 3;", 0),
        ("return 2 + 3;", 1),
        ("return -3;", 0),
        ("return -3;", 1),
        ("return (int) 3;", 0),
        ("$n = 1; $n++;", 1),
    ] {
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        values.fail_release_call = Some(failing_release);
        let program = parse_fragment(source.as_bytes()).unwrap();
        assert!(execute_program_with_context(&mut context, &program, &mut scope, &mut values).is_err());
        for cell in scope.drain_owned_cells() {
            eval_release_value(&mut context, &mut values, cell).unwrap();
        }
        assert_eq!(values.cell_owners.values().sum::<usize>(), 0, "{source}");
    }
}

/// Null decrement returns an acquired owner of the same cell without an extra identity retain.
#[test]
fn bytecode_slice_null_decrement_balances_same_cell_result() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let program = parse_fragment(b"$n = null; $n--; return $n;").unwrap();
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert_eq!(values.get(result), FakeValue::Null);
    eval_release_value(&mut context, &mut values, result).unwrap();
    for cell in scope.drain_owned_cells() {
        eval_release_value(&mut context, &mut values, cell).unwrap();
    }
    assert_eq!(values.cell_owners.values().sum::<usize>(), 0);
}

/// Backtrace arguments keep their scope addresses when recursion grows the explicit frame stack.
#[test]
fn bytecode_slice_backtrace_preserves_recursive_callers_and_arguments() {
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let setup = parse_fragment(b"function bytecode_inspect($x) { return debug_backtrace(); }").unwrap();
    execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
    let program = parse_fragment(b"
if (false) { function bytecode_inspect($x) { return $x; } }
function bytecode_recurse($n) {
    if ($n) { return bytecode_recurse($n - 1); }
    return bytecode_inspect($n);
}
$trace = bytecode_recurse(40);
").unwrap();
    assert!(bytecode::compile_program(&program).is_ok());
    execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    assert!(context.backtrace_frames().is_empty());
    let dump = parse_fragment(br#"
foreach ($trace as $frame) {
    echo $frame["function"]; echo ":"; echo $frame["args"][0]; echo ",";
}
"#).unwrap();
    execute_program_with_context(&mut context, &dump, &mut scope, &mut values).unwrap();
    let mut expected = "bytecode_inspect:0,".to_string();
    for n in 0..=40 {
        expected.push_str(&format!("bytecode_recurse:{n},"));
    }
    assert_eq!(values.output, expected);
}

/// Shared string adapters preserve missing-hook errors and exceptions raised by __toString.
#[test]
fn bytecode_slice_preserves_object_string_exceptions() {
    for (class_body, expected_class) in [
        ("public function __toString() { throw new Exception('string-failed'); }", "Exception"),
        ("", "Error"),
    ] {
        for source in ["echo $box;", "print $box;", "return (string) $box;"] {
            let mut context = ElephcEvalContext::new();
            let mut scope = ElephcEvalScope::new();
            let mut values = FakeOps::default();
            let setup = format!("class BytecodeErrorBox {{ {class_body} }} $box = new BytecodeErrorBox();");
            let setup = parse_fragment(setup.as_bytes()).unwrap();
            execute_program_with_context(&mut context, &setup, &mut scope, &mut values).unwrap();
            let program = parse_fragment(source.as_bytes()).unwrap();
            let outcome = execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
            assert!(bytecode::last_execution_used_vm());
            let EvalOutcome::Throwable(error) = outcome else { panic!("expected {expected_class}: {source}"); };
            assert!(values.object_is_a(error, expected_class, false).unwrap());
            assert!(values.output.is_empty());
            assert!(context.backtrace_frames().is_empty());
            assert!(context.take_pending_throw().is_none());
        }
    }
}

/// An error below multiple fast frames removes their argument metadata and restores caller scopes.
#[test]
fn bytecode_slice_unwind_restores_activation_metadata() {
    let program = parse_fragment(b"
if (false) { function bytecode_missing() { return 1; } }
function bytecode_fail($n) {
    if ($n) { return bytecode_fail($n - 1); }
    return bytecode_missing();
}
return bytecode_fail(16);
").unwrap();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    context.push_function("caller");
    context.push_class_scope("CallerClass");
    context.push_called_class_scope("CalledClass");
    let outcome = execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    let EvalOutcome::Throwable(error) = outcome else { panic!("expected undefined-function error"); };
    assert!(values.object_is_a(error, "Error", false).unwrap());
    assert_eq!(context.current_function(), Some("caller"));
    assert_eq!(context.current_class_scope(), Some("CallerClass"));
    assert_eq!(context.current_called_class_scope(), Some("CalledClass"));
    assert!(context.backtrace_frames().is_empty());
    assert!(context.take_pending_throw().is_none());
}
