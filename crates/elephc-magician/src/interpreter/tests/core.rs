//! Purpose:
//! Interpreter tests for scope mutation, exceptions, includes, and early execution results.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - These cases cover baseline eval execution before builtin-specific dispatch.

use super::super::*;
use super::support::*;

/// Native default cleanup releases owners even when binding or invocation failed.
#[test]
fn native_default_owners_release_after_failure() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let first = values.int(1).unwrap();
    let second = values.int(2).unwrap();
    let result: Result<(), EvalStatus> = super::super::statements::finish_native_default_owners(
        Err(EvalStatus::RuntimeFatal), vec![first, second], None, &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    assert_eq!(values.releases, vec![first, second]);
}

/// Native default cleanup transfers a returned cell and preserves a pending exception owner.
#[test]
fn native_default_owners_preserve_result_and_throwable() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let returned = values.int(1).unwrap();
    let thrown = values.new_object("Exception").unwrap();
    context.set_pending_throw(thrown);
    let result = super::super::statements::finish_native_default_owners(
        Ok(returned), vec![returned, thrown], Some(returned), &mut context, &mut values);
    assert_eq!(result, Ok(returned));
    assert!(values.releases.is_empty());
    assert_eq!(context.pending_throw(), Some(thrown));
}

/// ParseError construction releases argument cells but transfers the exception owner.
#[test]
fn parse_error_construction_releases_only_argument_temporaries() {
    let mut context = ElephcEvalContext::new();
    let mut values = FakeOps::default();
    let result: Result<RuntimeCellHandle, EvalStatus> =
        super::super::throwables::eval_throw_parse_failure(
            EvalStatus::ParseError, &mut context, &mut values);
    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
    let exception = context.take_pending_throw().unwrap();
    assert_eq!(values.releases.len(), 2);
    assert!(!values.releases.contains(&exception));
}

/// Nested eval replays cached warnings before returning its unchanged parse failure.
#[test]
fn nested_eval_failed_parse_replays_compile_warnings() {
    let program = parse_fragment(br#"eval('"\400"; return );');"#).unwrap();
    let mut context = ElephcEvalContext::new();
    context.set_call_site("caller.php", "", 7);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    for _ in 0..2 {
        assert_eq!(execute_program_with_context(&mut context, &program, &mut scope, &mut values),
            Err(EvalStatus::UncaughtThrowable));
    }
    assert_eq!(values.warnings.len(), 2);
    for warning in &values.warnings {
        assert_eq!(warning, "\nWarning: Octal escape sequence overflow \\400 is greater than \\377 in caller.php(7) : eval()'d code on line 1\n");
    }
    assert!(values.output.is_empty());
}

/// Included parse failures emit file-labelled warnings and restore the caller context.
#[test]
fn include_failed_parse_replays_compile_warnings() {
    let path = std::env::temp_dir().join(format!("elephc-include-parse-warning-{}.php", std::process::id()));
    std::fs::write(&path, br#"<?php "\400"; return );"#).unwrap();
    let source = format!("include '{}';", path.display());
    let program = parse_fragment(source.as_bytes()).unwrap();
    let mut context = ElephcEvalContext::new();
    context.set_call_site("caller.php", "", 7);
    let previous = context.call_site();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(result, Err(EvalStatus::UncaughtThrowable));
    assert_eq!(context.call_site(), previous);
    assert_eq!(values.warnings, vec![format!(
        "\nWarning: Octal escape sequence overflow \\400 is greater than \\377 in {} on line 1\n", path.display())]);
}

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
    let released_objects = values
        .releases
        .iter()
        .filter(|value| matches!(values.get(**value), FakeValue::Object(_)))
        .count();

    assert_eq!(scope.visible_cell("caught"), None);
    assert_eq!(released_objects, 1, "unbound catch releases its exception exactly once");
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
/// Verifies eval `catch` clauses can match eval-declared interfaces.
#[test]
fn execute_program_catches_eval_declared_interface_inside_eval() {
    let program = parse_fragment(
        br#"interface EvalCatchable {
    function value();
}
class EvalThrownBox implements EvalCatchable {
    public function value() { return 42; }
}
try {
    throw new EvalThrownBox();
} catch (EvalCatchable $caught) {
    return $caught->value();
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
    let released_objects = values
        .releases
        .iter()
        .filter(|value| matches!(values.get(**value), FakeValue::Object(_)))
        .count();

    assert_eq!(values.get(result), FakeValue::Int(2));
    assert_eq!(released_objects, 1, "finally releases the overridden exception exactly once");
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
///
/// The inner fragment is SINGLE-quoted because PHP interpolates a double-quoted eval
/// argument first: measured on PHP 8.5.6, `eval("throw $e;")` stringifies the exception
/// into the source and dies with `ParseError: syntax error, unexpected token ":"`. Only
/// the single-quoted form reaches the interpreter as `throw $e;`.
#[test]
fn execute_program_nested_eval_propagates_throw_as_uncaught_outcome() {
    let program = parse_fragment(br#"eval('throw $e;');"#).expect("parse eval fragment");
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
        "elephc-magician-include-{}-call-site",
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
    let dir = std::env::temp_dir().join(format!("elephc-magician-include-{}-once", std::process::id()));
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
        "elephc-magician-missing-{}-include.php",
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
        "elephc-magician-missing-{}-require.php",
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
