//! Purpose:
//! Compile-time diagnostic tests for the mysqli prelude surface: the type
//! checker rejects wrong argument counts and wrong argument types to
//! representative mysqli functions/methods once the prelude is injected.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `check_mysqli` mirrors `check_source` but injects the mysqli prelude
//!   (which idempotently prepends the shared `elephc_pdo` externs) between
//!   alias collection and name resolution, exactly as the production pipeline
//!   does, so the checker sees the prelude's typed signatures.
//! - `expect_mysqli_error` asserts the program fails to compile and the error
//!   names the flagged callee, guarding against a broken injection
//!   masquerading as success ("unknown function" instead of an arity error).

use super::*;

/// Runs the frontend pipeline (tokenize → parse → conditional → autoload
/// aliases → mysqli prelude injection → name resolution → constant folding →
/// type-check) and returns `Ok` if no errors were reported, or `Err(message)`
/// on the first compile error. The mysqli prelude is injected at the same
/// point as in `src/pipeline.rs`.
fn check_mysqli(src: &str) -> Result<(), String> {
    let tokens = tokenize(src).map_err(|e| e.message.clone())?;
    let ast = parse(&tokens).map_err(|e| e.message.clone())?;
    let defines: HashSet<String> = HashSet::new();
    let ast = elephc::conditional::apply(ast, &defines);
    let ast = elephc::autoload::collect_aliases(ast);
    let mut prelude_inventory = elephc::optimize::reachability::PreludeInventory::new();
    let ast = elephc::mysqli_prelude::inject_if_used(
        ast,
        false,
        elephc::php_version::PhpVersion::default(),
        &mut prelude_inventory,
    );
    let ast = elephc::name_resolver::resolve(ast).map_err(|e| e.message.clone())?;
    let ast = elephc::optimize::fold_constants(ast);
    types::check(&ast).map_err(|e| e.message.clone())?;
    Ok(())
}

/// Asserts that `src` fails to compile and that the error names `needle`.
fn expect_mysqli_error(src: &str, needle: &str) {
    let msg = check_mysqli(src)
        .err()
        .unwrap_or_else(|| panic!("Expected error naming '{needle}', but got Ok"));
    assert!(
        msg.contains(needle),
        "Error '{msg}' doesn't name '{needle}'"
    );
}

/// A well-formed mysqli program passes the checker with the injected prelude.
#[test]
fn well_formed_mysqli_program_type_checks() {
    assert_eq!(
        check_mysqli(
            r#"<?php
mysqli_report(MYSQLI_REPORT_OFF);
$db = new mysqli();
$db->options(MYSQLI_OPT_CONNECT_TIMEOUT, 1);
"#
        ),
        Ok(())
    );
}

/// `mysqli::query()` without arguments is an arity error at compile time.
#[test]
fn query_requires_the_query_argument() {
    expect_mysqli_error(
        r#"<?php
$db = new mysqli();
$db->query();
"#,
        "query",
    );
}

/// The constructor rejects more arguments than its six connection parameters.
#[test]
fn constructor_rejects_extra_arguments() {
    expect_mysqli_error(
        r#"<?php
$db = new mysqli("h", "u", "p", "d", 3306, null, 999);
"#,
        "mysqli",
    );
}

/// `mysqli_connect_errno()` takes no arguments (the no-link procedural form).
#[test]
fn connect_errno_takes_no_arguments() {
    expect_mysqli_error(
        r#"<?php
$db = new mysqli();
echo mysqli_connect_errno($db);
"#,
        "mysqli_connect_errno",
    );
}

/// `mysqli_stmt::bind_param` requires the type string.
#[test]
fn bind_param_requires_the_types_argument() {
    expect_mysqli_error(
        r#"<?php
$stmt = new mysqli_stmt();
$stmt->bind_param();
"#,
        "bind_param",
    );
}

/// `mysqli_stmt::bind_param` preserves PHP's by-ref variadic lvalue requirement.
#[test]
fn bind_param_rejects_literal_bind_arguments() {
    expect_mysqli_error(
        r#"<?php
$stmt = new mysqli_stmt();
$stmt->bind_param("i", 42);
"#,
        "variadic parameter $vars must be passed a variable",
    );
}

/// `mysqli_select_db` requires the database argument.
#[test]
fn select_db_requires_database_argument() {
    expect_mysqli_error(
        r#"<?php
$db = new mysqli();
$db->select_db();
"#,
        "select_db",
    );
}

/// The internal static factory `mysqli_stmt::__elephcInit` is private (the
/// checker's friend channel exposes it to `mysqli::stmt_init` only): user code
/// calling it is rejected at compile time like any private static.
#[test]
fn internal_stmt_init_factory_is_private() {
    expect_mysqli_error(
        r#"<?php
$db = new mysqli();
$stmt = mysqli_stmt::__elephcInit($db, -1);
"#,
        "__elephcInit",
    );
}
