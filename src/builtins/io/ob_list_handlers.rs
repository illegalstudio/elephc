//! Purpose:
//! Home of the PHP `ob_list_handlers` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array<Str>` (the macro cannot express array returns inline):
//!   one "default output handler" entry per active buffer level.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "ob_list_handlers",
    area: Io,
    params: [],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObListHandlers,
    ),
    summary: "Lists all output handlers in use.",
    php_manual: "function.ob-list-handlers",
}

/// Returns `Array<Str>`: one "default output handler" name per active buffer level.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
