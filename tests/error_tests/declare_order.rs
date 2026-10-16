//! Purpose:
//! Regression diagnostics for strict typing across multiple leading directives.
//!
//! Called from:
//! - The error integration harness.
//!
//! Key details:
//! - Earlier declare bodies are strict when any accepted leading directive enables it.
//! - Completing a control tail does not permit a following real statement before strict_types.

use super::*;

/// Calls cannot become coercive after a later zero or before a later leading one.
#[test]
fn test_error_declare_order_preserves_file_wide_strictness() {
    for (prefix, tail) in [
        ("declare(strict_types=1); declare(strict_types=0);", "echo f(true);"),
        ("declare(strict_types=0); declare(strict_types=1); declare(strict_types=0);", "echo f(true);"),
        ("declare(strict_types=0); declare(ticks=1) {echo f(true);} declare(strict_types=1);", "echo f(1);"),
        ("declare(strict_types=0); declare(ticks=1): echo f(true); enddeclare; declare(strict_types=1);", "echo f(1);"),
        ("declare(ticks=1) ifdef FEATURE {} else {echo f(true);} declare(strict_types=1);", "echo f(1);"),
    ] {
        expect_error(
            &format!("<?php {prefix} function f(int $i) {{return $i;}} {tail}"),
            "is active in this file",
        );
    }
}

/// A separate statement after the declare body still makes a later directive illegal.
#[test]
fn test_error_declare_order_keeps_the_prefix_boundary() {
    for body in [
        "if (true) {} else {}",
        "do {} while (false);",
        "try {} finally {}",
        "if (true): endif;",
        "ifdef FEATURE {}",
        "ifdef FEATURE {} else {}",
        "ifdef FEATURE { ifdef INNER {} }",
        "ifdef FEATURE {} else { ifdef INNER {} else {} }",
    ] {
        expect_error(
            &format!("<?php declare(ticks=1) {body} echo 1; declare(strict_types=1);"),
            "strict_types declaration must be the very first statement",
        );
    }
}
