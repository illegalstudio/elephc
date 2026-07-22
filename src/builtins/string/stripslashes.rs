//! Purpose:
//! Home of the PHP `stripslashes` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str -> Str` EIR signature.
//! - Concrete helper symbols and registers are selected only by the target backend.

use crate::ir::{RuntimeCallTarget, UnaryStringRuntime};

builtin! {
    name: "stripslashes",
    area: String,
    params: [string: Str],
    returns: Str,
    semantics: crate::builtins::semantics::unary_string_runtime(
        RuntimeCallTarget::UnaryString(UnaryStringRuntime::StripSlashes),
        crate::ir::Effects::PURE,
    ),
    summary: "Removes backslashes from a string previously escaped by addslashes.",
    php_manual: "https://www.php.net/manual/en/function.stripslashes.php",
}
