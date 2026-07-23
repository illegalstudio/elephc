//! Purpose:
//! Integration or regression tests for diagnostic coverage of recovery, including parser recovery
//! collects multiple errors, parser block recovery collects multiple errors, type checker recovery
//! collects multiple errors, and failed-assignment targets not cascading into `Undefined variable`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies the parser collects multiple errors from malformed PHP with sequential
/// `echo ;` statements missing expressions. Uses `tokenize` + `parse_with_recovery`
/// to confirm at least 2 parse errors are reported.
#[test]
fn test_parser_recovery_collects_multiple_errors() {
    let tokens = tokenize("<?php echo ; echo ;").unwrap();
    let errors = parse_with_recovery(&tokens).unwrap_err();
    assert!(errors.len() >= 2, "expected multiple parse errors, got {:?}", errors);
}

/// Verifies the parser collects multiple errors inside a function block with sequential
/// `echo ;` statements missing expressions. Uses `parse` (not `parse_with_recovery`)
/// to confirm the flattened error list contains at least 2 parse errors.
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

/// Verifies the type checker collects multiple errors when referencing undefined
/// variables across consecutive `echo` statements. Uses `check_source_full` to confirm
/// at least 2 checker errors are reported with their messages.
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

/// Verifies the type checker collects multiple early-errors from duplicate interface
/// names and duplicate extern function signatures in the same source. Uses
/// `check_source_full` to confirm at least 2 errors are reported with their messages.
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

/// Verifies the type checker collects multiple method return type mismatch errors from
/// two methods in the same class that return an integer where string is expected. Uses
/// `check_source_full` to confirm at least 2 errors are reported with their messages.
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

/// Regression for #597: when an assignment's RHS fails to type-check, the checker must
/// still register the target so a later use does not cascade into a misleading
/// `Undefined variable` diagnostic. The exact issue repro must emit only the real RHS
/// error (the spread diagnostic), and nothing about the assigned `$s`.
#[test]
fn test_failed_assignment_target_no_undefined_cascade() {
    let error = check_source_full("<?php $x = 5; $s = [...$x]; echo count($s);").unwrap_err();
    let messages: Vec<String> = error
        .flatten()
        .iter()
        .map(|error| error.message.clone())
        .collect();
    assert_eq!(
        messages.len(),
        1,
        "expected exactly one diagnostic, got {:?}",
        messages,
    );
    assert!(
        messages[0].contains("Spread operator requires an array"),
        "expected the spread diagnostic, got {:?}",
        messages,
    );
    assert!(
        !messages.iter().any(|message| message.contains("Undefined variable")),
        "failed-assignment target must not cascade into Undefined variable, got {:?}",
        messages,
    );
}

/// Regression for #597: a single failed assignment followed by many uses of the target
/// must emit exactly one diagnostic (the real RHS error), not one per later use. Exercises
/// several downstream forms (count, index, var_dump, reassignment) to prove the poisoned
/// `mixed` target is accepted silently rather than producing spurious type errors.
#[test]
fn test_failed_assignment_many_uses_single_error() {
    let error = check_source_full(
        "<?php $x = 5; $s = [...$x]; echo count($s); echo $s[0]; var_dump($s); $y = $s;",
    )
    .unwrap_err();
    let messages: Vec<String> = error
        .flatten()
        .iter()
        .map(|error| error.message.clone())
        .collect();
    assert_eq!(
        messages.len(),
        1,
        "one bad assignment with many later uses must emit exactly one error, got {:?}",
        messages,
    );
    assert!(
        messages[0].contains("Spread operator requires an array"),
        "expected the spread diagnostic, got {:?}",
        messages,
    );
}

/// Regression for #597: recovering a failed-assignment target must not suppress a genuinely
/// undefined variable used later. The spread error and the real `$undefined_for_real` error
/// are both reported, while the assigned `$s` produces no `Undefined variable` noise.
#[test]
fn test_failed_assignment_preserves_later_real_undefined() {
    let error = check_source_full(
        "<?php $x = 5; $s = [...$x]; echo count($s); echo $undefined_for_real;",
    )
    .unwrap_err();
    let messages: Vec<String> = error
        .flatten()
        .iter()
        .map(|error| error.message.clone())
        .collect();
    assert_eq!(
        messages.len(),
        2,
        "expected the spread error plus the genuine undefined-variable error, got {:?}",
        messages,
    );
    assert!(
        messages.iter().any(|message| message.contains("Spread operator requires an array")),
        "expected the spread diagnostic, got {:?}",
        messages,
    );
    assert!(
        messages.iter().any(|message| message.contains("Undefined variable: $undefined_for_real")),
        "a genuinely undefined variable must still be diagnosed, got {:?}",
        messages,
    );
    assert!(
        !messages.iter().any(|message| message.contains("Undefined variable: $s")),
        "the recovered target must not be reported as undefined, got {:?}",
        messages,
    );
}

/// Regression for #597: a typed local declaration whose initializer fails to type-check must
/// bind the target to its declared type for recovery, so later uses do not cascade. Only the
/// real RHS (spread) error is reported.
#[test]
fn test_failed_typed_assignment_no_undefined_cascade() {
    let error =
        check_source_full("<?php $x = 5; int $s = [...$x]; echo count($s); echo $s;").unwrap_err();
    let messages: Vec<String> = error
        .flatten()
        .iter()
        .map(|error| error.message.clone())
        .collect();
    assert_eq!(
        messages.len(),
        1,
        "a failed typed assignment must emit exactly one error, got {:?}",
        messages,
    );
    assert!(
        messages[0].contains("Spread operator requires an array"),
        "expected the spread diagnostic, got {:?}",
        messages,
    );
}

/// Control for #597: a valid spread assignment must still type-check cleanly, proving the
/// error-recovery poisoning never fires for well-typed initializers.
#[test]
fn test_valid_spread_assignment_still_compiles() {
    assert!(
        check_source("<?php $x = [1, 2, 3]; $s = [...$x]; echo count($s);").is_ok(),
        "a valid spread assignment must type-check without recovery poisoning",
    );
}

/// Regression for #597: a failed top-level assignment poisons its target as `Mixed`, but
/// method bodies seed their base environment from the top-level env. The poisoned top-level
/// local must not leak into a method that happens to reuse the same name, or the merge with
/// the method's own local would keep `Mixed` and spawn a spurious return-type error. Only the
/// real RHS (spread) error may be reported.
#[test]
fn test_failed_top_level_assignment_does_not_poison_method_local() {
    let error = check_source_full(
        "<?php $x = 5; $s = [...$x]; class C { public function f(): int { $s = 5; return $s; } }",
    )
    .unwrap_err();
    let messages: Vec<String> = error
        .flatten()
        .iter()
        .map(|error| error.message.clone())
        .collect();
    assert_eq!(
        messages.len(),
        1,
        "a poisoned top-level target must not leak into a same-named method local, got {:?}",
        messages,
    );
    assert!(
        messages[0].contains("Spread operator requires an array"),
        "expected the spread diagnostic, got {:?}",
        messages,
    );
}
