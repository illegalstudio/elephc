//! Purpose:
//! Emits the module's one copy of the wrong-tag `TypeError` ladder, so the eleven per-type
//! raises behind a parameter or return guard exist once per program instead of once per guard.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before the module's own functions.
//! - `crate::codegen::lower_inst::builtins::arrays::union_type_guard` at every wrong-tag dispatch.
//!
//! Key details:
//! - Each inlined ladder was a tag switch of ~11 arms, and every arm loaded its own static
//!   message and went through the shared static throw: ~60 instructions and eleven interned
//!   message/report/record triples per guard. The messages differ only in the "X given" word.
//! - A site now loads the address of a four-word record (prefix and suffix text) and calls the
//!   helper with the unbox tag and payload still in their registers. The helper spells the word
//!   at run time -- the class name for an object, `true`/`false` by value for a bool -- builds
//!   `prefix + word + suffix` and raises through the same unwinder path as every other
//!   codegen-raised `TypeError`.
//! - The helper never returns, so the call site keeps no code after it.
//! - It is emitted exactly when the module shares its static throws, so a site and the emitter
//!   cannot disagree about whether the label exists; `--counters` and `--instrument` builds keep
//!   the inlined ladder.

use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::Module;
use crate::types::PhpType;

use super::lower_inst::emit_shared_type_error_given_body;
use super::shared_helper::emit_shared_helper;
use super::shared_static_throw::module_shares_static_throw;
use super::Result;

/// Label of the helper that raises the wrong-tag `TypeError` for a boxed value.
pub(super) const TYPE_ERROR_GIVEN_LABEL: &str = "_eir_shared_type_error_given";

/// Returns the helper label a wrong-tag dispatch in `ctx` should call, if any.
///
/// `None` inside the helper itself, which is what stops the body from calling the label it is
/// defining.
pub(in crate::codegen) fn shared_label(ctx: &mut FunctionContext<'_>) -> Option<&'static str> {
    if ctx.function.name == TYPE_ERROR_GIVEN_LABEL {
        return None;
    }
    let module = ctx.module;
    module_shares_static_throw(module, ctx.shared).then_some(TYPE_ERROR_GIVEN_LABEL)
}

/// Emits the shared helper when the module uses it.
pub(super) fn emit_shared_type_error_given(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module_shares_static_throw(module, shared) {
        return Ok(());
    }
    emit_shared_helper(
        module,
        emitter,
        data,
        shared,
        regalloc_linear,
        TYPE_ERROR_GIVEN_LABEL,
        PhpType::Void,
        &format!(
            "--- shared wrong-tag TypeError: {} (tag, payload and message record in registers) ---",
            TYPE_ERROR_GIVEN_LABEL
        ),
        emit_shared_type_error_given_body,
    )
}
