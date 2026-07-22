//! Purpose:
//! Home of the PHP `rawurldecode` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - Its fresh result ownership replaces the historical independent-storage flag.

use crate::ir::{RuntimeCallTarget, UnaryStringRuntime};

builtin! {
    name: "rawurldecode",
    area: String,
    params: [string: Str],
    returns: Str,
    semantics: crate::builtins::semantics::unary_string_runtime(
        RuntimeCallTarget::UnaryString(UnaryStringRuntime::RawUrlDecode),
        crate::ir::Effects::PURE,
    ),
    summary: "Decodes an RFC 3986 percent-encoded string without treating '+' as a space.",
    php_manual: "https://www.php.net/manual/en/function.rawurldecode.php",
}
