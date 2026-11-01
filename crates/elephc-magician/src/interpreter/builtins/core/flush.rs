//! Purpose:
//! Eval registry entry and implementation for `flush`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - PHP's `flush()` pushes the SAPI's own write buffer to the client; it deliberately does
//!   NOT touch userland output buffers, which is `ob_flush()`'s job. elephc has no SAPI write
//!   buffer to push: `--web` assembles the whole response and writes it once at the end of the
//!   request, and a CLI binary writes straight through. PHP documents `flush()` as having no
//!   effect under exactly those conditions, so a no-op is the behavior, not a stub.
//! - It has to EXIST all the same: `Symfony\Component\HttpFoundation\Response::send()` calls
//!   it unconditionally on any SAPI outside `['cli', 'phpdbg', 'embed']`, which is the last
//!   statement of a `--web` request.

use super::super::super::*;

eval_builtin! {
    contract: "flush",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates PHP `flush()` over one eval expression list.
pub(in crate::interpreter) fn eval_builtin_flush(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_flush_result(values)
}

/// Returns null: there is no SAPI write buffer between elephc and the client.
pub(in crate::interpreter) fn eval_flush_result(
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.null()
}
