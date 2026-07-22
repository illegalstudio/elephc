//! Purpose:
//! Home of the PHP `rawurlencode` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - Concrete helper symbols and registers are selected only by the target backend.

use crate::ir::{RuntimeCallTarget, UnaryStringRuntime};

builtin! {
    name: "rawurlencode",
    area: String,
    params: [string: Str],
    returns: Str,
    semantics: crate::builtins::semantics::unary_string_runtime(
        RuntimeCallTarget::UnaryString(UnaryStringRuntime::RawUrlEncode),
        crate::ir::Effects::PURE,
    ),
    summary: "URL-encodes a string using RFC 3986 percent-encoding (no '+' for spaces).",
    php_manual: "https://www.php.net/manual/en/function.rawurlencode.php",
}
