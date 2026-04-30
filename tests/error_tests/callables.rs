use super::*;

#[test]
fn test_error_call_user_func_wrong_args() {
    expect_error(
        r#"<?php call_user_func();"#,
        "call_user_func() takes at least 1 argument",
    );
}

#[test]
fn test_error_function_exists_wrong_args() {
    expect_error(
        r#"<?php function_exists();"#,
        "function_exists() takes exactly 1 argument",
    );
}

// --- Closure / arrow function errors ---

#[test]
fn test_error_call_non_callable_variable() {
    expect_error(r#"<?php $x = 5; $x(1);"#, "not a callable");
}

#[test]
fn test_error_call_user_func_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); call_user_func($f, 1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_call_user_func_string_literal_ref_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } call_user_func(\"bump\", 1);",
        "parameter $n must be passed a variable",
    );
}

#[test]
fn test_error_closure_return_type_rejects_mismatch() {
    expect_error(
        "<?php $f = function(): string { return 1; };",
        "Closure return type expects Str, got Int",
    );
}

#[test]
fn test_error_arrow_return_type_rejects_mismatch() {
    expect_error(
        "<?php $f = fn(): int => \"nope\";",
        "Closure return type expects Int, got Str",
    );
}

#[test]
fn test_error_closure_void_return_type_rejects_value() {
    expect_error(
        "<?php $f = function(): void { return 1; };",
        "Closure return type expects Void, got Int",
    );
}
