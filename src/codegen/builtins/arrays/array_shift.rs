//! Purpose:
//! Emits PHP `array_shift` builtin calls that mutate array arguments in place.
//! Handles COW preparation and writes any replacement array pointer back to caller storage.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Mutating/ref-like arguments must avoid value-temp preevaluation so PHP-visible storage is updated.

use super::ensure_unique_arg::emit_ensure_unique_arg;
use super::store_mutating_arg::emit_store_mutating_arg;
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `array_shift()` builtin, which removes and returns the first element.
///
/// ## Inputs
/// - `args[0]`: the array to shift from (passed by reference, mutated in place)
/// - `emitter`: target-aware assembly emitter
/// - `ctx`: codegen context (carries variable layout, caller storage for ref-like args)
/// - `data`: data section for literals and runtime metadata
///
/// ## Outputs
/// - Returns `Option<PhpType>`: the element type that was removed (`Int` if the array
///   type is non-array or unknown, `inner` element type if wrapped in `PhpType::Array`)
///
/// ## Side effects & invariants
/// - Uses COW (copy-on-write): calls `ensure_unique_arg` to ensure the array is uniquely
///   owned before mutation to avoid modifying shared storage.
/// - Calls `store_mutating_arg` to write the replacement array pointer back to the
///   caller's variable slot after the runtime helper mutates the array in place.
/// - On ARM64: uses `bl __rt_array_shift`; on x86_64: uses `mov rdi, rax` then
///   `bl __rt_array_shift` to pass the array pointer via the first integer argument register.
/// - Preserves source evaluation order; argument side effects occur before the runtime call.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_shift()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(inner) => (**inner).clone(),
        _ => PhpType::Int,
    };
    let tagged_int_result =
        crate::codegen::sentinels::null_repr_is_tagged() && matches!(elem_ty, PhpType::Int);
    if emitter.target.arch == Arch::X86_64 {
        emit_ensure_unique_arg(emitter, &arr_ty);
        emit_store_mutating_arg(emitter, ctx, &args[0]);
        if tagged_int_result {
            // distinguish the empty-array case by length before the helper consumes the
            // pointer, so the result can carry a real null tag instead of the sentinel
            let empty_label = ctx.next_label("array_shift_empty");
            let end_label = ctx.next_label("array_shift_end");
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // load the indexed-array length before deciding whether the shift is empty
            emitter.instruction("test r10, r10");                               // check whether the indexed array currently stores any elements
            emitter.instruction(&format!("jz {}", empty_label));                // produce a tagged null when array_shift runs on an empty indexed array
            emitter.instruction("mov rdi, rax");                                // move the unique indexed-array pointer into the first x86_64 runtime argument register
            abi::emit_call_label(emitter, "__rt_array_shift");                  // remove and return the first scalar indexed-array element through the x86_64 runtime helper
            crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
            emitter.instruction(&format!("jmp {}", end_label));                 // skip the empty-array tagged-null path after the successful shift
            emitter.label(&empty_label);
            crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
            emitter.label(&end_label);
            return Some(PhpType::TaggedScalar);
        }
        emitter.instruction("mov rdi, rax");                                    // move the unique indexed-array pointer into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_array_shift");                      // remove and return the first scalar indexed-array element through the x86_64 runtime helper
        return Some(elem_ty);
    }

    emit_ensure_unique_arg(emitter, &arr_ty);
    emit_store_mutating_arg(emitter, ctx, &args[0]);
    if tagged_int_result {
        // distinguish the empty-array case by length before the helper consumes the
        // pointer, so the result can carry a real null tag instead of the sentinel
        let empty_label = ctx.next_label("array_shift_empty");
        let end_label = ctx.next_label("array_shift_end");
        emitter.instruction("ldr x9, [x0]");                                    // load the indexed-array length before deciding whether the shift is empty
        emitter.instruction(&format!("cbz x9, {}", empty_label));               // produce a tagged null when array_shift runs on an empty indexed array
        emitter.instruction("bl __rt_array_shift");                             // call runtime: shift first element -> x0=removed element
        crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
        emitter.instruction(&format!("b {}", end_label));                       // skip the empty-array tagged-null path after the successful shift
        emitter.label(&empty_label);
        crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
        emitter.label(&end_label);
        return Some(PhpType::TaggedScalar);
    }
    // -- call runtime to remove and return first element --
    emitter.instruction("bl __rt_array_shift");                                 // call runtime: shift first element -> x0=removed element

    Some(elem_ty)
}
