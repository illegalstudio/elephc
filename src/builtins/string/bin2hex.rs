//! Purpose:
//! Home of the PHP `bin2hex` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - Concrete helper symbols and registers are selected only by the target backend.

use crate::ir::{RuntimeCallTarget, UnaryStringRuntime};

builtin! {
    name: "bin2hex",
    area: String,
    params: [string: Str],
    returns: Str,
    semantics: crate::builtins::semantics::unary_string_runtime(
        RuntimeCallTarget::UnaryString(UnaryStringRuntime::BinToHex),
        crate::ir::Effects::PURE,
    ),
    summary: "Converts binary data into its hexadecimal string representation.",
    php_manual: "https://www.php.net/manual/en/function.bin2hex.php",
}
