//! Purpose:
//! Dispatches the internal resource helpers composed by the injected proc prelude.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Every body is a thin call into `crate::codegen::lower_inst::builtins::io`.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{Instruction, RuntimeFnId};

/// Lowers one proc-prelude resource target, or returns `None` for another group.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    use crate::codegen::lower_inst::builtins::io;
    match target {
        RuntimeFnId::ElephcFdStream => Some(io::lower_elephc_fd_stream(ctx, inst)),
        RuntimeFnId::ElephcProcResource => Some(io::lower_elephc_proc_resource(ctx, inst)),
        RuntimeFnId::ElephcResourceClose => Some(io::lower_elephc_resource_close(ctx, inst)),
        RuntimeFnId::ElephcResourceKind => Some(io::lower_elephc_resource_kind(ctx, inst)),
        RuntimeFnId::ElephcResourcePayload => Some(io::lower_elephc_resource_payload(ctx, inst)),
        _ => None,
    }
}
