//! Purpose:
//! Interpreter tests for scope mutation, exceptions, includes, and early execution results.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases cover baseline eval execution before builtin-specific dispatch.

use super::super::*;
use super::support::*;

/// Verifies assignment writes a named scope entry and return reads it back.
#[test]
fn execute_program_stores_and_returns_scope_value() {
    let program = parse_fragment(b"$x = 3; return $x + 4;").expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");

    assert_eq!(values.get(x), FakeValue::Int(3));
    assert_eq!(values.get(result), FakeValue::Int(7));
}
/// Verifies reference assignment aliases variable names and writes through the alias.
#[test]
fn execute_program_reference_assignment_updates_source_variable() {
    let program = parse_fragment(b"$x = 1; $alias =& $x; $alias = 5; return $x;")
        .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let x = scope.visible_cell("x").expect("scope should contain x");
    let alias = scope
        .visible_cell("alias")
        .expect("scope should contain alias");

    assert_eq!(x, alias);
    assert_eq!(values.get(x), FakeValue::Int(5));
    assert_eq!(values.get(result), FakeValue::Int(5));
}
/// Verifies eval `throw` exits the program with a retained Throwable cell.
#[test]
fn execute_program_propagates_throw_as_uncaught_outcome() {
    let program =
        parse_fragment(br#"throw new Exception("eval boom");"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let outcome =
        execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("throw should be an eval outcome");

    match outcome {
        EvalOutcome::Throwable(value) => {
            assert_eq!(values.type_tag(value), Ok(EVAL_TAG_OBJECT));
        }
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
}
/// Verifies eval `try/catch` catches a thrown object and binds the catch variable.
#[test]
fn execute_program_catches_throwable_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Throwable $caught) {
    return $caught->answer();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let caught = scope
        .visible_cell("caught")
        .expect("scope should contain catch variable");

    assert_eq!(values.type_tag(caught), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(42));
}
/// Verifies eval `catch (Throwable)` can handle a throw without binding a variable.
#[test]
fn execute_program_catches_throwable_without_variable_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Throwable) {
    return 9;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let released = values
        .releases
        .first()
        .copied()
        .expect("unbound catch should release the thrown object");

    assert_eq!(scope.visible_cell("caught"), None);
    assert_eq!(values.type_tag(released), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(9));
}
/// Verifies eval `catch (Exception)` matches thrown exception objects.
#[test]
fn execute_program_catches_specific_exception_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (Exception $caught) {
    return $caught->answer();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let caught = scope
        .visible_cell("caught")
        .expect("scope should contain catch variable");

    assert_eq!(values.type_tag(caught), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(42));
}
/// Verifies eval catch clauses keep source order and skip non-matching types.
#[test]
fn execute_program_skips_non_matching_specific_catch_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (RuntimeException $wrong) {
    return 1;
} catch (Exception $caught) {
    return 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(scope.visible_cell("wrong"), None);
    assert_eq!(values.get(result), FakeValue::Int(2));
}
/// Verifies union catch clauses test later types in the same catch clause.
#[test]
fn execute_program_catches_union_type_inside_eval() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} catch (RuntimeException|Exception $caught) {
    return $caught->answer();
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let caught = scope
        .visible_cell("caught")
        .expect("scope should contain catch variable");

    assert_eq!(values.type_tag(caught), Ok(EVAL_TAG_OBJECT));
    assert_eq!(values.get(result), FakeValue::Int(42));
}
/// Verifies eval `finally` runs before a pending try-body return is observed.
#[test]
fn execute_program_runs_finally_before_returning_try_value() {
    let program = parse_fragment(
        br#"try {
    return 1;
} finally {
    echo "finally";
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "finally");
    assert_eq!(values.get(result), FakeValue::Int(1));
}
/// Verifies eval `finally` return values replace pending try-body returns.
#[test]
fn execute_program_finally_return_overrides_try_return() {
    let program = parse_fragment(
        br#"try {
    return 1;
} finally {
    return 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(values.releases.len(), 1);
}
/// Verifies eval `finally` return values replace pending uncaught throws.
#[test]
fn execute_program_finally_return_overrides_uncaught_throw() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} finally {
    return 2;
}"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    let released = values
        .releases
        .first()
        .copied()
        .expect("overridden throw should be released");

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(values.type_tag(released), Ok(EVAL_TAG_OBJECT));
}
/// Verifies eval `finally` runs before an uncaught throw leaves the fragment.
#[test]
fn execute_program_runs_finally_before_uncaught_throw_outcome() {
    let program = parse_fragment(
        br#"try {
    throw new Exception("eval boom");
} finally {
    echo "finally";
}"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let outcome =
        execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("throw should be an eval outcome");

    match outcome {
        EvalOutcome::Throwable(value) => {
            assert_eq!(values.type_tag(value), Ok(EVAL_TAG_OBJECT))
        }
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
    assert_eq!(values.output, "finally");
}
/// Verifies static locals declared inside eval catch blocks persist per function context.
#[test]
fn execute_context_function_persists_static_local_inside_catch() {
    let program = parse_fragment(
        br#"function dyn($e) {
    try {
        throw $e;
    } catch (Throwable $caught) {
        static $n = 0;
        $n++;
        return $caught->answer() + $n;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("declare dynamic function");
    let first_thrown = values
        .new_object("Exception")
        .expect("allocate first fake exception");
    let second_thrown = values
        .new_object("Exception")
        .expect("allocate second fake exception");

    let first = execute_context_function(&mut context, "dyn", vec![first_thrown], &mut values)
        .expect("execute first dynamic function call");
    let second = execute_context_function(&mut context, "dyn", vec![second_thrown], &mut values)
        .expect("execute second dynamic function call");

    assert_eq!(values.get(first), FakeValue::Int(43));
    assert_eq!(values.get(second), FakeValue::Int(44));
}
/// Verifies static locals declared inside eval finally blocks persist per function context.
#[test]
fn execute_context_function_persists_static_local_inside_finally() {
    let program = parse_fragment(
        br#"function dyn() {
    try {
        return 0;
    } finally {
        static $n = 0;
        $n++;
        return $n;
    }
}"#,
    )
    .expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("declare dynamic function");

    let first = execute_context_function_zero_args(&mut context, "dyn", &mut values)
        .expect("execute first dynamic function call");
    let second = execute_context_function_zero_args(&mut context, "dyn", &mut values)
        .expect("execute second dynamic function call");

    assert_eq!(values.get(first), FakeValue::Int(1));
    assert_eq!(values.get(second), FakeValue::Int(2));
}
/// Verifies throws from eval-declared functions escape through the shared context.
#[test]
fn execute_context_function_propagates_throw_as_uncaught_outcome() {
    let program =
        parse_fragment(br#"function dyn($e) { throw $e; }"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("declare dynamic function");
    let thrown = values
        .new_object("Exception")
        .expect("allocate fake exception");

    let outcome = execute_context_function_outcome(&mut context, "dyn", vec![thrown], &mut values)
        .expect("throw should be an eval function outcome");

    match outcome {
        EvalOutcome::Throwable(value) => assert_eq!(value, thrown),
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
}
/// Verifies nested eval preserves the thrown cell while returning an uncaught status.
#[test]
fn execute_program_nested_eval_propagates_throw_as_uncaught_outcome() {
    let program = parse_fragment(br#"eval("throw $e;");"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let thrown = values
        .new_object("Exception")
        .expect("allocate fake exception");
    scope.set("e", thrown, ScopeCellOwnership::Borrowed);

    let outcome =
        execute_program_outcome_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("nested throw should be an eval outcome");

    match outcome {
        EvalOutcome::Throwable(value) => assert_eq!(value, thrown),
        EvalOutcome::Value(value) => panic!("expected Throwable, got {:?}", values.get(value)),
    }
}
/// Verifies eval include resolves caller-relative paths, shares scope, and returns file values.
#[test]
fn execute_program_include_uses_call_site_and_returns_file_result() {
    let dir = std::env::temp_dir().join(format!(
        "elephc-eval-include-{}-call-site",
        std::process::id()
    ));
    let path = dir.join("piece.php");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create include fixture directory");
    std::fs::write(
            &path,
            format!(
                r#"<?php echo (__DIR__ === "{}" ? "D" : "d"); echo (__FILE__ === "{}" ? "F" : "f"); $x = $x + 1; return $x;"#,
                dir.to_string_lossy(),
                path.to_string_lossy()
            ),
        )
        .expect("write include fixture");
    let program = parse_fragment(br#"return include "piece.php";"#).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        dir.join("main.php").to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
        1,
    );
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let x = values.int(2).expect("allocate fake int");
    scope.set("x", x, ScopeCellOwnership::Owned);

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute eval include");

    assert_eq!(values.output, "DF");
    assert_eq!(values.get(result), FakeValue::Int(3));
    assert_eq!(
        values.get(scope.visible_cell("x").expect("scope should contain x")),
        FakeValue::Int(3)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
/// Verifies regular include marks a file so later include_once skips it and returns true.
#[test]
fn execute_program_include_once_skips_regularly_included_file() {
    let dir = std::env::temp_dir().join(format!("elephc-eval-include-{}-once", std::process::id()));
    let path = dir.join("once.php");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create include_once fixture directory");
    std::fs::write(&path, br#"<?php echo "O";"#).expect("write include_once fixture");
    let source = format!(
        r#"include "{}"; return include_once "{}";"#,
        path.to_string_lossy(),
        path.to_string_lossy()
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("execute include_once");

    assert_eq!(values.output, "O");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    let _ = std::fs::remove_dir_all(&dir);
}
/// Verifies missing include warns and returns false without aborting the eval program.
#[test]
fn execute_program_missing_include_warns_and_returns_false() {
    let missing = std::env::temp_dir().join(format!(
        "elephc-eval-missing-{}-include.php",
        std::process::id()
    ));
    let source = format!(r#"return include "{}";"#, missing.to_string_lossy());
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect("missing include returns false");

    assert_eq!(values.get(result), FakeValue::Bool(false));
    assert_eq!(values.warnings.len(), 2);
}
/// Verifies missing require emits warnings and aborts the eval program.
#[test]
fn execute_program_missing_require_is_runtime_fatal() {
    let missing = std::env::temp_dir().join(format!(
        "elephc-eval-missing-{}-require.php",
        std::process::id()
    ));
    let source = format!(r#"require "{}";"#, missing.to_string_lossy());
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let err = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
        .expect_err("missing require should fail");

    assert_eq!(err, EvalStatus::RuntimeFatal);
    assert_eq!(values.warnings.len(), 2);
}
