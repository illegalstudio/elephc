//! Purpose:
//! Pins file-wide strict typing across leading declare statements and complete control bodies.
//!
//! Called from:
//! - The parser integration harness.
//!
//! Key details:
//! - Strictness must stamp earlier nested statements before a later directive is parsed.
//! - A separate physical file still begins with its own coercive profile.

use super::*;

/// A leading strict directive affects the whole file and cannot be cleared by a later zero.
#[test]
fn test_declare_order_strictness_is_file_wide() {
    for prefix in [
        "declare(strict_types=1); declare(strict_types=0);",
        "declare(strict_types=0); declare(strict_types=1);",
        "declare(strict_types=0); declare(ticks=1) {echo 1;} declare(strict_types=1); declare(strict_types=0);",
    ] {
        let stmts = parse_source(&format!("<?php {prefix} echo 2;"));
        assert!(stmts.iter().all(|stmt| stmt.strict_types), "{prefix}");
        for stmt in &stmts {
            if let StmtKind::Synthetic(body) = &stmt.kind {
                assert!(body.iter().all(|stmt| stmt.strict_types), "{prefix}");
            }
        }
    }
    let loose = parse_source("<?php declare(strict_types=0); echo 1;");
    assert!(loose.iter().all(|stmt| !stmt.strict_types));
}

/// A complete single-statement body may precede a later strict directive.
#[test]
fn test_declare_order_control_tails_are_one_statement() {
    for body in [
        "if (true) echo 1; else echo 2;",
        "match (1) {1 => 1} + 2;",
        "declare(ticks=2) define('enum', true); declare(ticks=2) enum ? match (1) {1 => 1} : null;",
        "$f = function () {} ?? null;",
        "$f = #[A([1])] function () {} ?? null;",
        "if (false) {} elseif (true) {} else {}",
        "if (true) if (false) echo 1; else echo 2; else echo 3;",
        "do {echo 1;} while (false);",
        "try {} catch (Exception $e) {} finally {}",
        "if (true): echo 1; elseif (false): echo 2; else: echo 3; endif;",
        "while (false): endwhile;",
        "for ($i=0; $i<1; $i++): endfor;",
        "foreach ([1] as $i): endforeach;",
        "switch (1): case true ? 1 : 2: break; default: endswitch;",
        "declare(ticks=2): if (true): echo 1; endif; enddeclare;",
    ] {
        let stmts = parse_source(&format!(
            "<?php declare(ticks=1) {body} declare(strict_types=1); echo 4;"
        ));
        assert!(stmts.iter().all(|stmt| stmt.strict_types), "{body}");
    }
}

/// An ifdef body ends after its balanced blocks, before a following leading directive.
#[test]
fn test_declare_order_ifdef_tails_are_one_statement() {
    for body in [
        "ifdef FEATURE {}",
        "ifdef FEATURE {} else {}",
        "ifdef FEATURE { ifdef INNER {} }",
        "ifdef FEATURE {} else { ifdef INNER {} else {} }",
        "if (true) ifdef FEATURE {} else {}",
    ] {
        let stmts = parse_source(&format!(
            "<?php declare(ticks=1) {body} declare(strict_types=1); echo 4;"
        ));
        assert!(stmts.iter().all(|stmt| stmt.strict_types), "{body}");
    }
}

/// Incomplete control tails stay syntax errors instead of silently opening a new prefix.
#[test]
fn test_declare_order_rejects_incomplete_bodies() {
    for body in [
        "if (true): echo 1;",
        "do {echo 1;}",
        "try {} catch (Exception $e)",
        "if (true) echo 1; else",
        "ifdef FEATURE",
        "ifdef FEATURE {} else",
        "ifdef {}",
        "ifdef FEATURE { ifdef INNER {}",
    ] {
        assert!(parse_fails(&format!(
            "<?php declare(ticks=1) {body} declare(strict_types=1);"
        )), "{body}");
    }
}
