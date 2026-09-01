//! Purpose:
//! Regression coverage for residual ext/date differences found by the frozen php-src PHPT audit.
//!
//! Called from:
//! - `cargo test --test codegen_tests date_php_src_residuals` through Rust's test harness.
//!
//! Key details:
//! - Object-handle assertions are intentional because php-src exposes allocation order through
//!   `var_dump()` and `spl_object_id()`.

use crate::support::*;

/// Ensures `DateTime::diff()` allocates only its returned interval and consumes object handle 3.
#[test]
fn test_datetime_diff_does_not_allocate_a_temporary_timezone_object() {
    let out = compile_and_run(
        r#"<?php
date_default_timezone_set("Europe/Paris");
$start = new DateTime("2016-03-01");
$end = new DateTime("2016-03-31");
$interval = $start->diff($end, true);
echo spl_object_id($start), "|", spl_object_id($end), "|", spl_object_id($interval), "\n";
echo $interval->d, "|", $interval->days, "\n";
"#,
    );

    assert_eq!(out, "1|2|3\n30|30\n");
}

/// Verifies a fixed receiver gets its PHP object id before a named object argument is evaluated.
#[test]
fn test_fixed_constructor_receiver_precedes_named_object_argument() {
    let out = compile_and_run(
        r#"<?php
class ReceiverBeforeNamedArg {
    public function __construct(public stdClass $payload) {}
}
$receiver = new ReceiverBeforeNamedArg(payload: new stdClass);
echo spl_object_id($receiver), "|", spl_object_id($receiver->payload), "\n";
"#,
    );

    assert_eq!(out, "1|2\n");
}

/// Verifies inherited user constructors preserve source effects and optional defaults after preallocation.
#[test]
fn test_fixed_constructor_preallocation_preserves_effects_and_defaults() {
    let out = compile_and_run(
        r#"<?php
function build_payload(): stdClass {
    echo "arg|";
    return new stdClass;
}
class ParentReceiverBeforeArg {
    public function __construct(public stdClass $payload, public string $label = "default") {
        echo "ctor|";
    }
}
class ChildReceiverBeforeArg extends ParentReceiverBeforeArg {}
$receiver = new ChildReceiverBeforeArg(build_payload());
echo spl_object_id($receiver), "|", spl_object_id($receiver->payload), "|", $receiver->label, "\n";
"#,
    );

    assert_eq!(out, "arg|ctor|1|2|default\n");
}

/// Verifies surplus user-constructor arguments are evaluated but do not widen the fixed ABI.
#[test]
fn test_fixed_zero_arg_constructor_evaluates_and_ignores_surplus_argument() {
    let out = compile_and_run(
        r#"<?php
function build_surplus_payload(): stdClass {
    $payload = new stdClass;
    echo "arg:", spl_object_id($payload), "|";
    return $payload;
}
class ZeroArgReceiverBeforeArg {
    public function __construct() { echo "ctor|"; }
}
$receiver = new ZeroArgReceiverBeforeArg(build_surplus_payload());
echo spl_object_id($receiver), "\n";
"#,
    );

    assert_eq!(out, "arg:2|ctor|1\n");
}

/// Verifies the hidden variadic still forwards constructor surplus used by `func_num_args()`.
#[test]
fn test_fixed_constructor_surplus_remains_visible_to_argument_introspection() {
    let out = compile_and_run(
        r#"<?php
class IntrospectiveConstructorReceiver {
    public int $count = 0;
    public function __construct() { $this->count = func_num_args(); }
}
$receiver = new IntrospectiveConstructorReceiver(1, 2, 3);
echo spl_object_id($receiver), "|", $receiver->count, "\n";
"#,
    );

    assert_eq!(out, "1|3\n");
}

/// Verifies php-src allows exactly one fixed `__set_state` parameter plus a variadic tail.
#[test]
fn test_set_state_allows_one_fixed_parameter_and_variadic_tail() {
    let out = compile_and_run(
        r#"<?php
class ValidSetState {
    public static function __set_state(array $state, &...$rest): static {
        return new static();
    }
}
echo "ok\n";
"#,
    );
    assert_eq!(out, "ok\n");
}

/// Verifies declaration-time `__set_state` fatals match php-src's complete contract.
#[test]
fn test_set_state_invalid_contracts_match_php_src() {
    let cases = [
        (
            "<?php class Bad { public static function __set_state(...$state) {} }",
            "Method Bad::__set_state() must take exactly 1 argument",
        ),
        (
            "<?php class Bad { public function __set_state($state) {} }",
            "Method Bad::__set_state() must be static",
        ),
        (
            "<?php class Bad { public static function __set_state(&$state) {} }",
            "Method Bad::__set_state() cannot take arguments by reference",
        ),
        (
            "<?php class Bad { public function __set_state(array &$state): static { return new static(); } }",
            "Method Bad::__set_state() cannot take arguments by reference",
        ),
        (
            "<?php class Bad { public static function __set_state(object $state) {} }",
            "Bad::__set_state(): Parameter #1 ($state) must be of type array when declared",
        ),
        (
            "<?php class Bad { public static function __set_state(array $state): int { return 1; } }",
            "Bad::__set_state(): Return type must be object when declared",
        ),
        (
            "<?php trait Bad { public static function __set_state($state, $extra) {} }",
            "Method Bad::__set_state() must take exactly 1 argument",
        ),
        (
            "<?php interface Bad { public static function __set_state($state, $extra); }",
            "Method Bad::__set_state() must take exactly 1 argument",
        ),
    ];
    for (source, expected) in cases {
        let output = compile_and_run_capture(source);
        assert!(
            !output.success,
            "invalid declaration unexpectedly ran: {source}; stdout={:?}; stderr={:?}",
            output.stdout,
            output.stderr
        );
        assert!(
            output.stderr.contains(expected),
            "missing `{expected}` in stderr: {}",
            output.stderr
        );
    }
}

/// Verifies php-src's non-fatal visibility warning precedes ordinary program output.
#[test]
fn test_set_state_non_public_visibility_warning_matches_php_src() {
    let output = compile_and_run_capture(
        r#"<?php
class HiddenSetState {
    private static function __set_state(array $state) {}
}
echo "ok\n";
"#,
    );
    assert!(output.success, "visibility fixture failed: {}", output.stderr);
    assert_eq!(output.stdout, "ok\n");
    assert!(
        output.stderr.starts_with(
            "\nWarning: The magic method HiddenSetState::__set_state() must have public visibility in "
        ) && output.stderr.ends_with(" on line 3\n"),
        "unexpected visibility warning: {}",
        output.stderr
    );
}

/// Verifies php-src emits the visibility warning before a later parameter-type fatal.
#[test]
fn test_set_state_visibility_warning_precedes_type_fatal() {
    let output = compile_and_run_capture(
        r#"<?php
class HiddenInvalidSetState {
    private static function __set_state(string $state): static { return new static(); }
}
"#,
    );
    assert!(!output.success, "invalid declaration unexpectedly ran");
    let warning = "The magic method HiddenInvalidSetState::__set_state() must have public visibility";
    let fatal = "HiddenInvalidSetState::__set_state(): Parameter #1 ($state) must be of type array when declared";
    let warning_position = output.stderr.find(warning).expect("visibility warning");
    let fatal_position = output.stderr.find(fatal).expect("parameter-type fatal");
    assert!(
        warning_position < fatal_position,
        "warning did not precede fatal: {}",
        output.stderr
    );
}
