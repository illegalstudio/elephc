//! Purpose:
//! Dispatches one bounded group of typed builtin runtime targets.
//!
//! Called from:
//! - `super::lower()` while lowering typed EIR runtime calls.
//!
//! Key details:
//! - Dispatch is by enum identity, never by PHP function-name strings.
//! - Extracted bodies remain thin calls into target-aware backend emitters.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::{RuntimeFnId, Instruction};

/// Lowers a target owned by bounded dispatch group 08, or returns `None`.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Option<Result<()>> {
    match target {
        RuntimeFnId::Sqrt => Some({
            crate::codegen::lower_inst::builtins::math::lower_sqrt(ctx, inst)
        }),
        RuntimeFnId::Tan => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "tan")
        }),
        RuntimeFnId::Tanh => Some({
            crate::codegen::lower_inst::builtins::math::lower_unary_libm(ctx, inst, "tanh")
        }),
        RuntimeFnId::ElephcPtrIsNull => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_is_null(ctx, inst)
        }),
        RuntimeFnId::ElephcPtrReadString => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_read_string(ctx, inst)
        }),
        RuntimeFnId::ElephcPtrWriteString => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_write_string(ctx, inst)
        }),
        RuntimeFnId::BufferFree => Some({
            crate::codegen::lower_inst::builtins::buffers::lower_buffer_free(ctx, inst)
        }),
        RuntimeFnId::BufferLen => Some({
            crate::codegen::lower_inst::builtins::buffers::lower_buffer_len(ctx, inst)
        }),
        RuntimeFnId::Ptr => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr(ctx, inst)
        }),
        RuntimeFnId::PtrGet => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_get(ctx, inst)
        }),
        RuntimeFnId::PtrIsNull => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_is_null(ctx, inst)
        }),
        RuntimeFnId::PtrNull => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_null(ctx, inst)
        }),
        RuntimeFnId::PtrOffset => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_offset(ctx, inst)
        }),
        RuntimeFnId::PtrRead16 => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_read16(ctx, inst)
        }),
        RuntimeFnId::PtrRead32 => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_read32(ctx, inst)
        }),
        RuntimeFnId::PtrRead8 => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_read8(ctx, inst)
        }),
        RuntimeFnId::PtrReadString => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_read_string(ctx, inst)
        }),
        RuntimeFnId::PtrSet => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_set(ctx, inst)
        }),
        RuntimeFnId::PtrSizeof => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_sizeof(ctx, inst)
        }),
        RuntimeFnId::PtrWrite16 => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_write16(ctx, inst)
        }),
        RuntimeFnId::PtrWrite32 => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_write32(ctx, inst)
        }),
        RuntimeFnId::PtrWrite8 => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_write8(ctx, inst)
        }),
        RuntimeFnId::PtrWriteString => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_ptr_write_string(ctx, inst)
        }),
        RuntimeFnId::ZvalFree => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_zval_free(ctx, inst)
        }),
        RuntimeFnId::ZvalPack => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_zval_pack(ctx, inst)
        }),
        RuntimeFnId::ZvalType => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_zval_type(ctx, inst)
        }),
        RuntimeFnId::ZvalUnpack => Some({
            crate::codegen::lower_inst::builtins::pointers::lower_zval_unpack(ctx, inst)
        }),
        RuntimeFnId::IteratorApply => Some({
            crate::codegen::lower_inst::builtins::spl::lower_iterator_apply(ctx, inst)
        }),
        RuntimeFnId::IteratorCount => Some({
            crate::codegen::lower_inst::builtins::spl::lower_iterator_count(ctx, inst)
        }),
        RuntimeFnId::IteratorToArray => Some({
            crate::codegen::lower_inst::builtins::spl::lower_iterator_to_array(ctx, inst)
        }),
        RuntimeFnId::SplAutoload => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_void(
                    ctx,
                    inst,
                    "spl_autoload",
                )
        }),
        RuntimeFnId::SplAutoloadCall => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_void(
                    ctx,
                    inst,
                    "spl_autoload_call",
                )
        }),
        RuntimeFnId::SplAutoloadExtensions => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_extensions(ctx, inst)
        }),
        RuntimeFnId::SplAutoloadFunctions => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_functions(ctx, inst)
        }),
        RuntimeFnId::SplAutoloadRegister => Some({
            crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_bool(
                    ctx,
                    inst,
                    "spl_autoload_register",
                )
        }),
        _ => None,
    }
}
