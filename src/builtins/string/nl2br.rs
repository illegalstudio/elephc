//! Purpose:
//! Home of the PHP `nl2br` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - Concrete helper symbols and registers are selected only by the target backend.

use crate::ir::{RuntimeCallTarget, UnaryStringRuntime};

builtin! {
    name: "nl2br",
    area: String,
    params: [string: Str],
    returns: Str,
    semantics: crate::builtins::semantics::unary_string_runtime(
        RuntimeCallTarget::UnaryString(UnaryStringRuntime::NlToBr),
        crate::ir::Effects::PURE,
    ),
    summary: "Inserts HTML line breaks before newlines in a string.",
    php_manual: "https://www.php.net/manual/en/function.nl2br.php",
}
