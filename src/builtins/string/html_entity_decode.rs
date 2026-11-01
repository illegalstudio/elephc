//! Purpose:
//! Home of the PHP `html_entity_decode` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - php's `$flags`/`$encoding` are DECLARED so the checker accepts the ordinary three-argument
//!   call, and dropped from the operand list by `BuiltinArgumentLowering::HtmlEntityDecode`
//!   before the one-operand runtime target sees them. Same treatment its inverse
//!   `htmlspecialchars` already gets in `lower_html_escape`.
//! - Concrete helper symbols and registers are selected only by the target backend.

use crate::builtins::semantics::{
    unary_string_runtime, with_argument_lowering, BuiltinArgumentLowering, BuiltinSemantics,
};
use crate::ir::{RuntimeCallTarget, UnaryStringRuntime};

builtin! {
    contract: "html_entity_decode",
    semantics: html_entity_decode_semantics(),
}

/// Builds `Str -> Str` runtime semantics that accept php's optional `$flags`/`$encoding`.
const fn html_entity_decode_semantics() -> BuiltinSemantics {
    with_argument_lowering(
        unary_string_runtime(
            RuntimeCallTarget::UnaryString(UnaryStringRuntime::HtmlEntityDecode),
            crate::ir::Effects::PURE,
        ),
        BuiltinArgumentLowering::HtmlEntityDecode,
    )
}
