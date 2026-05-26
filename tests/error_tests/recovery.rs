//! Purpose:
//! Integration or regression tests for diagnostic coverage of recovery, including parser recovery collects multiple errors, parser block recovery collects multiple errors, and type checker recovery collects multiple errors.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

// Verifies the parser collects multiple errors from malformed PHP with sequential
// `echo ;` statements missing expressions. Uses `tokenize` + `parse_with_recovery`
// to confirm at least 2 parse errors are reported.
#[test]
fn test_parser_recovery_collects_multiple_errors() {
    let tokens = tokenize("<?php echo ; echo ;").unwrap();
    let errors = parse_with_recovery(&tokens).unwrap_err();
    assert!(errors.len() >= 2, "expected multiple parse errors, got {:?}", errors);
}

// Verifies the parser collects multiple errors inside a function block with sequential
// `echo ;` statements missing expressions. Uses `parse` (not `parse_with_recovery`)
// to confirm the flattened error list contains at least 2 parse errors.
#[test]
fn test_parser_block_recovery_collects_multiple_errors() {
    let tokens = tokenize("<?php function foo() { echo ; echo ; }").unwrap();
    let error = parse(&tokens).unwrap_err();
    assert!(
        error.flatten().len() >= 2,
        "expected multiple parse errors in block, got {:?}",
        error.flatten(),
    );
}

// Verifies the type checker collects multiple errors when referencing undefined
// variables across consecutive `echo` statements. Uses `check_source_full` to confirm
// at least 2 checker errors are reported with their messages.
#[test]
fn test_type_checker_recovery_collects_multiple_errors() {
    let error = check_source_full("<?php echo $missing; echo $also_missing;").unwrap_err();
    let all = error.flatten();
    assert!(
        all.len() >= 2,
        "expected multiple checker errors, got {:?}",
        all.iter().map(|error| error.message.clone()).collect::<Vec<_>>(),
    );
}

// Verifies the type checker collects multiple early-errors from duplicate interface
// names and duplicate extern function signatures in the same source. Uses
// `check_source_full` to confirm at least 2 errors are reported with their messages.
#[test]
fn test_type_checker_recovery_collects_multiple_early_errors() {
    let error = check_source_full(
        "<?php interface A {} interface A {} extern function foo(): int; extern function foo(): int;",
    )
    .unwrap_err();
    let all = error.flatten();
    assert!(
        all.len() >= 2,
        "expected multiple early checker errors, got {:?}",
        all.iter().map(|error| error.message.clone()).collect::<Vec<_>>(),
    );
}

// Verifies the type checker collects multiple method return type mismatch errors from
// two methods in the same class that return an integer where string is expected. Uses
// `check_source_full` to confirm at least 2 errors are reported with their messages.
#[test]
fn test_type_checker_recovery_collects_multiple_method_return_errors() {
    let error = check_source_full(
        "<?php class Demo { public function one(): string { return 1; } public function two(): string { return 2; } }",
    )
    .unwrap_err();
    let all = error.flatten();
    assert!(
        all.len() >= 2,
        "expected multiple method return errors, got {:?}",
        all.iter().map(|error| error.message.clone()).collect::<Vec<_>>(),
    );
}
