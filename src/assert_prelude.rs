//! Purpose:
//! Injects an elephc-PHP implementation of `assert()`'s single-argument form.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before type checking.
//!
//! Key details:
//! - A truthy assertion returns true; a falsy assertion throws `AssertionError`.
//! - The optional description form stays on the native builtin path until Throwable-valued
//!   descriptions can be forwarded without losing their dynamic exception type.

use crate::parser::ast::Program;

/// Reserved function name used for one-argument `assert()` calls.
pub const ASSERT_ONE_ARG_NAME: &str = "__elephc_assert_one_arg";

/// PHP source for the injected one-argument assertion implementation.
pub const ASSERT_PRELUDE_SRC: &str = r#"<?php
function __elephc_assert_one_arg(mixed $assertion): bool {
    if (!$assertion) {
        throw new AssertionError('assertion failed');
    }
    return true;
}
"#;

/// Prepends the helper when the resolved program references `assert`.
pub fn inject_if_used(program: Program) -> Program {
    if !crate::ast_usage::collect(&program).references("assert") {
        return program;
    }
    let tokens = crate::lexer::tokenize(ASSERT_PRELUDE_SRC).expect("assert prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("assert prelude must parse");
    combined.extend(program);
    combined
}
