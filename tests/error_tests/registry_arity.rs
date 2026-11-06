//! Purpose:
//! Checks arity diagnostics for PCNTL and XML handler registry entries that lacked
//! direct diagnostic fixtures in the release coverage audit.
//!
//! Called from:
//! - The `error_tests` integration-test root through Rust's test harness.
//!
//! Key details:
//! - Fixtures stop in the frontend before native packages or process operations run.
//! - Invalid counts must be diagnosed before target availability or argument types.

use super::*;

/// Verifies PCNTL arity failures, including zero-argument and optional-argument APIs.
#[test]
fn pcntl_registry_entries_reject_invalid_argument_counts() {
    for (call, name) in [
        ("pcntl_async_signals(true, false)", "pcntl_async_signals"),
        ("pcntl_errno(1)", "pcntl_errno"),
        ("pcntl_get_last_error(1)", "pcntl_get_last_error"),
        ("pcntl_getcpu(1)", "pcntl_getcpu"),
        ("pcntl_getcpuaffinity(1, 2)", "pcntl_getcpuaffinity"),
        ("pcntl_getqos_class(1)", "pcntl_getqos_class"),
        // Both parameters are optional, so the invalid fixture exceeds the maximum.
        ("pcntl_setns(1, 2, 3)", "pcntl_setns"),
        ("pcntl_signal_dispatch(1)", "pcntl_signal_dispatch"),
        ("pcntl_signal_get_handler()", "pcntl_signal_get_handler"),
        ("pcntl_strerror()", "pcntl_strerror"),
        ("pcntl_unshare()", "pcntl_unshare"),
        ("pcntl_wexitstatus()", "pcntl_wexitstatus"),
        ("pcntl_wifcontinued()", "pcntl_wifcontinued"),
        ("pcntl_wifsignaled()", "pcntl_wifsignaled"),
        ("pcntl_wifstopped()", "pcntl_wifstopped"),
        ("pcntl_wstopsig()", "pcntl_wstopsig"),
        ("pcntl_wtermsig()", "pcntl_wtermsig"),
    ] {
        let error = match check_source(&format!("<?php {call};")) {
            Ok(()) => panic!("{call} must reject an invalid PCNTL argument count"),
            Err(error) => error,
        };
        assert!(error.contains(name), "{name}: {error}");
        assert!(error.contains("argument"), "{name}: {error}");
        assert!(!error.contains("not available"), "{name}: {error}");
    }
}

/// Verifies every previously uncovered XML handler setter requires its two arguments.
#[test]
fn xml_handler_registry_entries_require_parser_and_callback() {
    for name in [
        "xml_set_default_handler",
        "xml_set_end_namespace_decl_handler",
        "xml_set_external_entity_ref_handler",
        "xml_set_notation_decl_handler",
        "xml_set_processing_instruction_handler",
        "xml_set_start_namespace_decl_handler",
        "xml_set_unparsed_entity_decl_handler",
    ] {
        let error = check_source(&format!("<?php {name}();"))
            .expect_err("an XML handler setter without arguments must fail");
        assert!(error.contains(name), "{name}: {error}");
        assert!(error.contains("argument"), "{name}: {error}");
        assert!(!error.contains("Undefined"), "{name}: {error}");
    }
}
