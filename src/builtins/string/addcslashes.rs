//! Purpose:
//! Home of the PHP `addcslashes` builtin and its backend-neutral runtime semantics.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The typed runtime target has a validated `Str, Str -> Str` EIR signature.
//! - The runtime helper owns range expansion, short C escapes, and octal escapes.

builtin! {
    contract: "addcslashes",
    semantics: crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::Addcslashes),
}
