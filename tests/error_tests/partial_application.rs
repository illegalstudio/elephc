//! Purpose:
//! Rejects invalid PHP 8.6 partial-function-application shapes before codegen.
//!
//! Called from:
//! - `cargo test --test error_tests partial_application`.
//!
//! Key details:
//! - The desugaring pass owns these diagnostics, so the helper mirrors the pipeline through
//!   `pfa::desugar_for_version` with the 8.6 profile. One case asserts the profile gate itself.

use super::*;

/// Runs the frontend through partial-application desugaring under PHP 8.6.
fn check86(src: &str) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    let ast = elephc::autoload::collect_aliases(ast).map_err(|error| error.message)?;
    let ast = elephc::name_resolver::resolve(ast).map_err(|e| e.message.clone())?;
    let ast = elephc::func_args::desugar(ast).map_err(|e| e.message.clone())?;
    let ast = elephc::pfa::desugar_for_version(ast, elephc::php_version::PhpVersion::Php86)
        .map_err(|e| e.message.clone())?;
    let ast = elephc::optimize::fold_constants(ast);
    types::check(&ast).map(|_| ()).map_err(|e| e.message.clone())
}

/// Asserts one PHP 8.6 partial-application diagnostic.
fn expect_pfa_error(src: &str, expected: &str) {
    match check86(src) {
        Ok(()) => panic!("Expected error containing '{expected}', but got Ok"),
        Err(message) => assert!(
            message.contains(expected),
            "Error '{message}' doesn't contain '{expected}'"
        ),
    }
}

/// Too few placeholders leave a required parameter unbound.
#[test]
fn test_pfa_rejects_too_few_arguments_php86() {
    expect_pfa_error(
        "<?php function foo(int $a, int $b) {} foo(?);",
        "Partial application of foo() expects at least 2 arguments, 1 given",
    );
}

/// Too many positional placeholders exceed the declared parameters.
#[test]
fn test_pfa_rejects_too_many_arguments_php86() {
    expect_pfa_error(
        "<?php function foo(int $a, int $b) {} foo(?, ?, ?);",
        "Partial application of foo() expects at most 2 arguments, 3 given",
    );
}

/// Positional placeholders cannot follow named arguments.
#[test]
fn test_pfa_rejects_positional_after_named_php86() {
    expect_pfa_error(
        "<?php function foo(int $a, int $b) {} foo(a: 1, ?);",
        "Cannot use positional placeholder after named argument",
    );
}

/// A named placeholder must name a declared parameter.
#[test]
fn test_pfa_rejects_unknown_named_php86() {
    expect_pfa_error(
        "<?php function foo(int $a) {} foo(x: ?);",
        "Unknown named parameter $x",
    );
}

/// Two placeholders cannot claim the same closure parameter.
#[test]
fn test_pfa_rejects_duplicate_placeholder_php86() {
    expect_pfa_error(
        "<?php function foo(int $a, int $b) {} foo(?, a: ?);",
        "Named parameter $a overwrites previous placeholder",
    );
}

/// Constructors are not partial-function-application targets.
#[test]
fn test_pfa_rejects_constructor_php86() {
    expect_pfa_error(
        "<?php class C {} new C(?);",
        "Cannot create Closure for new expression",
    );
}

/// The variadic marker must be the final argument.
#[test]
fn test_pfa_rejects_variadic_not_last_php86() {
    expect_pfa_error(
        "<?php function foo(...$args) {} foo(..., 1);",
        "The variadic placeholder must be the last argument",
    );
}

/// A method whose signature is invisible cannot be partially applied safely.
#[test]
fn test_pfa_rejects_undeclared_method_php86() {
    expect_pfa_error(
        "<?php class C {} $c = new C; $c->nope(?);",
        "Cannot create Closure for a method whose signature is not declared",
    );
}

/// Partial application is syntax from PHP 8.6 and is rejected on earlier profiles.
#[test]
fn test_pfa_requires_php_86_profile() {
    expect_error(
        "<?php function foo($a) {} foo(?);",
        "Partial function application requires --php-version=8.6",
    );
}
