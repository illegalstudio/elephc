//! Purpose:
//! Home of the PHP `base64_decode` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - Concrete helper symbols and registers are selected only by the target backend.

use crate::ir::{RuntimeCallTarget, UnaryStringRuntime};

builtin! {
    name: "base64_decode",
    area: String,
    params: [string: Str],
    returns: Str,
    semantics: crate::builtins::semantics::unary_string_runtime(
        RuntimeCallTarget::UnaryString(UnaryStringRuntime::Base64Decode),
        crate::ir::Effects::PURE,
    ),
    summary: "Decodes a Base64-encoded string back into its original data.",
    php_manual: "https://www.php.net/manual/en/function.base64-decode.php",
}
