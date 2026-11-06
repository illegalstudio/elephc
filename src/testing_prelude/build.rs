//! Purpose:
//! Builds the testing prelude as AST: the `Elephc\Testing` namespace (the two error classes,
//! `TestCase`, `Runner`) and the `PHPUnit\Framework\TestCase` alias, so the runtime compiles
//! through the ordinary class pipeline without tokenizing PHP text at injection time.
//!
//! Called from:
//! - `crate::testing_prelude::inject_if_used`, after the hash prelude and before name
//!   resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration was generated from the parse of the PHP
//!   form in `crate::testing_prelude::fragments` (`synthetic_class::transcribe`, driven by
//!   `ELEPHC_TRANSCRIBE_WHICH=file`), and `testing_prelude::oracle_tests` compares the built
//!   AST against that parse node by node. Edit the shape here only with that comparison in
//!   hand.
//! - Declaration ORDER follows the PHP source (`AssertionFailedError`, `SkippedTestError`,
//!   `IncompleteTestError`, `TestCase`, `Runner`, then the alias namespace) so the oracle can
//!   zip the two programs.
//! - One helper per class, never one expression for the whole surface: a prelude built as a
//!   single nested builder expression overflows the stack.
//! - The alias class extends `\Elephc\Testing\TestCase` by its FULLY QUALIFIED spelling: a
//!   bare `TestCase` inside `namespace PHPUnit\Framework` would name itself.

use crate::parser::ast::{Program, Stmt};
use crate::synthetic_class::{class, internal_declarations, s_namespace};

mod runner;
mod test_case;

/// Builds the whole testing surface, one declaration per helper.
pub(crate) fn testing_declarations() -> Program {
    internal_declarations(|| vec![decl_elephc_testing_namespace(), decl_phpunit_namespace()])
}

/// `namespace Elephc\Testing { ... }`: the runtime proper.
fn decl_elephc_testing_namespace() -> Stmt {
    s_namespace(
        "Elephc\\Testing",
        vec![
            decl_assertion_failed_error(),
            decl_skipped_test_error(),
            decl_incomplete_test_error(),
            test_case::decl_test_case(),
            runner::decl_runner(),
        ],
    )
}

/// `final class AssertionFailedError extends \Exception`: what every failing assertion and
/// `fail()` throw, and what the runner catches as a FAILURE.
fn decl_assertion_failed_error() -> Stmt {
    class("AssertionFailedError")
        .final_()
        .extends("\\Exception")
        .build()
}

/// `final class SkippedTestError extends \Exception`: what `markTestSkipped()` throws.
fn decl_skipped_test_error() -> Stmt {
    class("SkippedTestError")
        .final_()
        .extends("\\Exception")
        .build()
}

/// `final class IncompleteTestError extends \Exception`: what `markTestIncomplete()` throws.
fn decl_incomplete_test_error() -> Stmt {
    class("IncompleteTestError")
        .final_()
        .extends("\\Exception")
        .build()
}

/// `namespace PHPUnit\Framework { abstract class TestCase extends \Elephc\Testing\TestCase }`:
/// the alias that lets a PHPUnit-written suite compile unchanged.
fn decl_phpunit_namespace() -> Stmt {
    s_namespace(
        "PHPUnit\\Framework",
        vec![class("TestCase")
            .abstract_()
            .extends("\\Elephc\\Testing\\TestCase")
            .build()],
    )
}
