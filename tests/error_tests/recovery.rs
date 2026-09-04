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

/// Runs full type checking and returns every emitted diagnostic message.
fn checker_error_messages(src: &str) -> Vec<String> {
    check_source_full(src)
        .expect_err("expected source to fail type checking")
        .flatten()
        .iter()
        .map(|error| error.message.clone())
        .collect()
}

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

/// Verifies the type checker collects multiple errors instead of stopping at the first.
///
/// The witness is two undefined CONSTANTS across consecutive `echo` statements. It used to be
/// two undefined variables, which stopped being checker errors when elephc adopted PHP's
/// semantics for them. Recovery is the property under test, not which construct fails.
#[test]
fn test_type_checker_recovery_collects_multiple_errors() {
    let error = check_source_full("<?php echo UNDEF_A; echo UNDEF_B;").unwrap_err();
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
    let messages =
        checker_error_messages("<?php $x = 5; $s = [...$x]; echo count($s);");
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
    let messages = checker_error_messages(
        "<?php $x = 5; $s = [...$x]; echo count($s); echo $s[0]; var_dump($s); $y = $s;",
    );
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
/// broken name used later. Both the spread error and the later one are reported, while the
/// assigned `$s` produces no diagnostic of its own.
///
/// The later name is an undefined CONSTANT. It used to be an undefined variable, which stopped
/// being a checker error when elephc adopted PHP's semantics for those: a read of a name that
/// was never assigned warns at run time and answers null. The property under test is that
/// recovery does not swallow what follows it, so it moved to a diagnostic that still exists.
#[test]
fn test_failed_assignment_preserves_later_real_undefined() {
    let messages = checker_error_messages(
        "<?php $x = 5; $s = [...$x]; echo count($s); echo UNDEFINED_FOR_REAL;",
    );
    assert_eq!(
        messages.len(),
        2,
        "expected the spread error plus the genuine undefined-constant error, got {:?}",
        messages,
    );
    assert!(
        messages.iter().any(|message| message.contains("Spread operator requires an array")),
        "expected the spread diagnostic, got {:?}",
        messages,
    );
    assert!(
        messages.iter().any(|message| message.contains("Undefined constant: UNDEFINED_FOR_REAL")),
        "a genuinely undefined name must still be diagnosed, got {:?}",
        messages,
    );
    assert!(
        !messages.iter().any(|message| message.contains("$s")),
        "the recovered target must not be reported at all, got {:?}",
        messages,
    );
}

/// Regression for #597: a typed local declaration whose initializer fails to type-check binds
/// the target as unknown for recovery, so later uses do not cascade. Only the real RHS error is
/// reported.
#[test]
fn test_failed_typed_assignment_no_undefined_cascade() {
    let messages =
        checker_error_messages("<?php $x = 5; int $s = [...$x]; echo count($s); echo $s;");
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

/// Regression for #597: a typed local whose initializer has an incompatible type is poisoned as
/// unknown, preventing a later consumer from producing a second diagnostic based on the rejected
/// declaration.
#[test]
fn test_failed_typed_assignment_mismatch_no_follow_on_error() {
    let messages = checker_error_messages("<?php int $s = \"bad\"; echo count($s);");
    assert_eq!(
        messages.len(),
        1,
        "a typed-assignment mismatch must not trigger follow-on errors, got {:?}",
        messages,
    );
    assert!(
        messages[0].contains("cannot initialize $s as int with string"),
        "expected the typed-assignment diagnostic, got {:?}",
        messages,
    );
}

/// Regression for #597: list destructuring is also a local-binding assignment. A non-array RHS
/// must report its own diagnostic while every newly named target remains usable for recovery.
#[test]
fn test_failed_list_unpack_targets_no_undefined_cascade() {
    let messages =
        checker_error_messages("<?php [$a, $b] = 42; echo count($a); echo $b;");
    assert_eq!(
        messages.len(),
        1,
        "failed list unpacking must emit only its RHS-shape error, got {:?}",
        messages,
    );
    assert!(
        messages[0].contains("List unpacking requires an array"),
        "expected the list-unpacking diagnostic, got {:?}",
        messages,
    );
}

/// Regression for #597: if list destructuring cannot infer its RHS at all, all targets are still
/// bound for recovery, so exactly one diagnostic comes out of the whole snippet.
///
/// The source is now an undefined variable that types as null rather than one that fails to
/// type at all, so the single diagnostic is the RHS-shape one — the same one its `= 42` twin
/// above reports. Which of the two it is does not matter to this test; that it is ONE, with no
/// cascade from `$a` or `$b`, is the property.
///
/// DIVERGENCE, deliberately not asserted here: `php -n` 8.5.6 accepts this program outright,
/// warning `Undefined variable $missing` and binding both targets to NULL. elephc refuses
/// list unpacking from a non-array at compile time, for `null` and for `42` alike.
#[test]
fn test_failed_list_unpack_inference_targets_no_undefined_cascade() {
    let messages =
        checker_error_messages("<?php [$a, $b] = $missing; echo count($a); echo $b;");
    assert_eq!(
        messages,
        vec!["List unpacking requires an array on the right-hand side".to_string()],
        "failed list-unpack inference must not leave either target undefined",
    );
}

/// Regression for #597: a failed reference assignment still names its target. Poisoning an
/// unbound target prevents the undefined source diagnostic from cascading to later target reads.
#[test]
fn test_failed_reference_assignment_target_no_undefined_cascade() {
    let messages =
        checker_error_messages("<?php $target =& $missing; echo count($target);");
    assert_eq!(
        messages,
        vec!["Undefined variable: $missing".to_string()],
        "failed reference assignment must not leave its target undefined",
    );
}

/// Regression for #597: a static local whose initializer fails must still be present in the local
/// recovery environment, so later reads in the same function do not add undefined-variable noise.
#[test]
fn test_failed_static_local_target_no_undefined_cascade() {
    let messages = checker_error_messages(
        "<?php function f(int $x): void { static $s = [...$x]; echo count($s); }",
    );
    assert_eq!(
        messages.len(),
        1,
        "failed static-local initialization must emit only the RHS error, got {:?}",
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

/// Regression for #597: a failed top-level assignment target must not leak into a method-local
/// scope that happens to reuse the same name. Only the real RHS error may be reported.
#[test]
fn test_failed_top_level_assignment_does_not_poison_method_local() {
    let messages = checker_error_messages(
        "<?php $x = 5; $s = [...$x]; class C { public function f(): int { $s = 5; return $s; } }",
    );
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

/// Verifies method locals are isolated from ordinary top-level locals even when the top-level
/// binding is valid. Reusing a name with a different type must not cause a false reassignment or
/// return-type error inside the method.
#[test]
fn test_top_level_local_does_not_seed_method_scope() {
    assert!(
        check_source(
            "<?php $value = \"top\"; class C { public function f(): int { $value = 5; return $value; } }",
        )
        .is_ok(),
        "ordinary top-level locals must not participate in method-local type merging",
    );
}

/// Verifies the method-scope isolation keeps PHP's explicit `global` escape hatch. The method
/// resolves the final top-level type even when that binding is introduced by the program's last
/// statement.
#[test]
fn test_method_global_declaration_uses_final_top_level_environment() {
    assert!(
        check_source(
            "<?php class C { public function f(): string { global $value; return $value; } } $value = \"ok\";",
        )
        .is_ok(),
        "explicit global declarations must resolve final top-level bindings",
    );
}
