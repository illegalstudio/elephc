//! Purpose:
//! Home of the PHP `proc_get_status` builtin and its result-type contract.
//!
//! Called from:
//! - The builtin registry, type checker, and typed EIR runtime-call lowering.
//!
//! Key details:
//! - Windows reads the retained process HANDLE without consuming it, preserving
//!   PHP's repeated-status semantics until `proc_close()` closes the resource.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    name: "proc_get_status",
    area: Io,
    params: [process: Mixed],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ProcGetStatus,
    ),
    summary: "Retrieves the current status of a process opened by proc_open.",
    php_manual: "function.proc-get-status",
}

/// Types the status record as PHP's string-keyed mixed associative array or false.
///
/// The builtin registry has already inferred the process argument before it
/// calls this non-lazy hook, so this only refines the return shape.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(_cx.checker.normalize_union_type(vec![
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        PhpType::Bool,
    ]))
}
