//! Purpose:
//! Lowers PHP `array_unshift()` calls for indexed arrays in the Phase 04 EIR backend.
//! Supports 8-byte payloads (Int/Bool/Float/Callable/refcounted/Mixed) via the hardened
//! `__rt_array_unshift` runtime helper, and 16-byte string payloads via inline slot shifting.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays::lower_array_unshift()`.
//!
//! Key details:
//! - Mutates the caller-visible array after copy-on-write splitting and optional growth.
//! - Returns the new indexed-array length as PHP `int`.
//! - Mirrors `array_shift()` element-type gating and `array_push()` value preparation.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{expect_operand, store_if_result};

/// Lowers `array_unshift()` by ensuring uniqueness, prepending one value, and returning count.
pub(super) fn lower_array_unshift(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_unshift", 2)?;
    let array = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let elem_ty = array_unshift_element_type(ctx.value_php_type(array)?)?;
    let value_ty = ctx.value_php_type(value)?;
    let effective_elem_ty = effective_unshift_element_type(&elem_ty, &value_ty);
    require_array_unshift_value_type(&elem_ty, &value_ty)?;
    require_array_unshift_result_type(&inst.result_php_type.codegen_repr())?;
    let source_local = super::source_load_local_slot(ctx, array)?;
    ensure_unique_array_unshift_source(ctx, array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_array_unshift_aarch64(ctx, array, value, &effective_elem_ty)?,
        Arch::X86_64 => lower_array_unshift_x86_64(ctx, array, value, &effective_elem_ty)?,
    }
    if let Some(slot) = source_local {
        ctx.store_value_to_local(slot, array)?;
    }
    ctx.writeback_symbol_array_source(array)?;
    // -- read the new count only after write-back: store_value_to_local/writeback both
    // reload `array` through the result register, which would otherwise clobber a count
    // computed by the arch-specific lowering before this point.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the new indexed-array length as the PHP return value
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rax")?;
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the new indexed-array length as the PHP return value
        }
    }
    store_if_result(ctx, inst)
}

/// Returns the supported element payload type for an indexed-array `array_unshift()`.
fn array_unshift_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Str
                    | PhpType::Callable
                    | PhpType::Mixed
                    | PhpType::Void
                    | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            if matches!(elem, PhpType::AssocArray { .. }) {
                return Err(CodegenIrError::unsupported(
                    "array_unshift indexed-array element PHP type AssocArray (out of scope)",
                ));
            }
            Err(CodegenIrError::unsupported(format!(
                "array_unshift indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_unshift for PHP type {:?}",
            other
        ))),
    }
}

/// Chooses the concrete element shape used for lowering.
///
/// Empty `array<never>` targets take on the shape of the first prepended value; otherwise
/// the declared element type drives the helper selection.
fn effective_unshift_element_type(elem_ty: &PhpType, value_ty: &PhpType) -> PhpType {
    if matches!(elem_ty.codegen_repr(), PhpType::Void | PhpType::Never) {
        value_ty.codegen_repr()
    } else {
        elem_ty.clone()
    }
}

/// Verifies the prepended value can be stored in the indexed array's element cells.
fn require_array_unshift_value_type(elem_ty: &PhpType, value_ty: &PhpType) -> Result<()> {
    let elem_repr = elem_ty.codegen_repr();
    let value_repr = value_ty.codegen_repr();

    // An empty array<never> accepts any first-write value; its shape will be fixed by the
    // lowering path.
    if matches!(elem_repr, PhpType::Void | PhpType::Never) {
        return Ok(());
    }

    // Concrete typed arrays require a value whose representation matches the element repr.
    if value_repr == elem_repr {
        return Ok(());
    }

    // Mixed arrays accept any boxable value (concrete values are boxed, existing boxes are
    // incref'd).
    if matches!(elem_repr, PhpType::Mixed) {
        return Ok(());
    }

    Err(CodegenIrError::unsupported(format!(
        "array_unshift value PHP type {:?} for indexed-array element PHP type {:?}",
        value_repr,
        elem_repr
    )))
}

/// Verifies the lowered `array_unshift()` result carries PHP's integer count metadata.
fn require_array_unshift_result_type(result_ty: &PhpType) -> Result<()> {
    if result_ty == &PhpType::Int {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_unshift result PHP type {:?}",
        result_ty
    )))
}

/// Splits a shared indexed array before `array_unshift()` mutates its slots.
fn ensure_unique_array_unshift_source(ctx: &mut FunctionContext<'_>, array: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(array, "x0")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(array, "rdi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_ensure_unique");
    ctx.store_result_value(array)
}

/// Lowers AArch64 `array_unshift()` for 8-byte and 16-byte (string) element slots.
fn lower_array_unshift_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    if elem_ty == &PhpType::Str {
        return lower_array_unshift_str_aarch64(ctx, array, value);
    }
    lower_array_unshift_scalar_aarch64(ctx, array, value, elem_ty)
}

/// Lowers x86_64 `array_unshift()` for 8-byte and 16-byte (string) element slots.
fn lower_array_unshift_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    if elem_ty == &PhpType::Str {
        return lower_array_unshift_str_x86_64(ctx, array, value);
    }
    lower_array_unshift_scalar_x86_64(ctx, array, value, elem_ty)
}

/// Returns true when an unshift into a Mixed array must box a concrete value first.
fn array_unshift_value_needs_mixed_box(elem_ty: &PhpType, value_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Mixed)
        && !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Returns true when the value being unshifted into a Mixed array is already a Mixed box.
fn array_unshift_value_is_mixed_box(elem_ty: &PhpType, value_ty: &PhpType) -> bool {
    matches!(elem_ty.codegen_repr(), PhpType::Mixed)
        && matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
}

/// Lowers an AArch64 scalar or boxed-Mixed `array_unshift()` via the runtime helper.
fn lower_array_unshift_scalar_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_unshift_value_needs_mixed_box(elem_ty, &value_ty) {
        ctx.load_value_to_reg(value, "x0")?;                                   // load the concrete value to be boxed as Mixed
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &value_ty); // allocate a Mixed box owned by the array
        ctx.emitter.instruction("mov x1, x0");                                  // pass the boxed Mixed payload to the unshift helper
    } else if array_unshift_value_is_mixed_box(elem_ty, &value_ty) {
        ctx.load_value_to_reg(value, "x0")?;                                   // load the existing Mixed box
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);                // take array ownership of the Mixed box
        ctx.emitter.instruction("mov x1, x0");                                  // pass the incref'd Mixed box to the unshift helper
    } else if matches!(value_ty.codegen_repr(), PhpType::Callable) {
        ctx.load_value_to_reg(value, "x0")?;                                   // load the callable descriptor
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);                // take array ownership of the callable
        ctx.emitter.instruction("mov x1, x0");                                  // pass the callable descriptor in the helper argument register
    } else if value_ty.codegen_repr().is_refcounted() {
        ctx.load_value_to_reg(value, "x0")?;                                   // load the refcounted payload
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);                // take array ownership of the refcounted payload
        ctx.emitter.instruction("mov x1, x0");                                  // pass the incref'd refcounted payload to the helper
    } else {
        ctx.load_value_to_reg(value, "x1")?;                                   // load the 8-bit scalar payload directly into the helper argument register
    }
    ctx.load_value_to_reg(array, "x0")?;                                       // load the unique indexed-array receiver into the helper argument register
    abi::emit_call_label(ctx.emitter, "__rt_array_unshift");
    // The caller reads the new count after write-back to avoid a result-register clobber.
    ctx.store_result_value(array)?;                                            // update the IR array value to the possibly reallocated array pointer
    Ok(())
}

/// Lowers an x86_64 scalar or boxed-Mixed `array_unshift()` via the runtime helper.
fn lower_array_unshift_scalar_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
    elem_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if array_unshift_value_needs_mixed_box(elem_ty, &value_ty) {
        ctx.load_value_to_reg(value, "rax")?;                                  // load the concrete value to be boxed as Mixed
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &value_ty); // allocate a Mixed box owned by the array
        ctx.emitter.instruction("mov rsi, rax");                                // pass the boxed Mixed payload to the unshift helper
    } else if array_unshift_value_is_mixed_box(elem_ty, &value_ty) {
        ctx.load_value_to_reg(value, "rax")?;                                  // load the existing Mixed box
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);                // take array ownership of the Mixed box
        ctx.emitter.instruction("mov rsi, rax");                                // pass the incref'd Mixed box to the unshift helper
    } else if matches!(value_ty.codegen_repr(), PhpType::Callable) {
        ctx.load_value_to_reg(value, "rax")?;                                  // load the callable descriptor
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);                // take array ownership of the callable
        ctx.emitter.instruction("mov rsi, rax");                                // pass the callable descriptor in the helper argument register
    } else if value_ty.codegen_repr().is_refcounted() {
        ctx.load_value_to_reg(value, "rax")?;                                  // load the refcounted payload
        abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);                // take array ownership of the refcounted payload
        ctx.emitter.instruction("mov rsi, rax");                                // pass the incref'd refcounted payload to the helper
    } else {
        ctx.load_value_to_reg(value, "rsi")?;                                  // load the 8-bit scalar payload directly into the helper argument register
    }
    ctx.load_value_to_reg(array, "rdi")?;                                      // load the unique indexed-array receiver into the helper argument register
    abi::emit_call_label(ctx.emitter, "__rt_array_unshift");
    // The caller reads the new count after write-back to avoid a result-register clobber.
    ctx.store_result_value(array)?;                                            // update the IR array value to the possibly reallocated array pointer
    Ok(())
}

/// Lowers AArch64 `array_unshift()` for a string-element indexed array.
///
/// The helper `__rt_array_unshift` only handles 8-byte slots, so 16-byte string slots are
/// shifted inline.  This path also handles the first-write shape change from an empty
/// `array<never>` buffer (8-byte slots) to a string array (16-byte slots).
fn lower_array_unshift_str_aarch64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
) -> Result<()> {
    let have_room_label = ctx.next_label("array_unshift_str_have_room");

    ctx.load_value_to_reg(array, "x0")?;                                       // load the unique indexed-array pointer after COW splitting
    emit_array_unshift_str_first_write_aarch64(ctx)?;                          // fix up empty array<never> shape for 16-byte string slots

    // -- check capacity; grow the backing store when full --
    ctx.emitter.instruction("ldr x9, [x0]");                                    // reload length after first-write shape specialization
    ctx.emitter.instruction("ldr x10, [x0, #8]");                               // load capacity counted in string slots
    ctx.emitter.instruction("cmp x9, x10");                                     // compare logical length with allocated capacity
    ctx.emitter.instruction(&format!("b.lt {}", have_room_label));              // skip growth when at least one free slot remains

    ctx.emitter.instruction("sub sp, sp, #16");                                 // allocate a temporary slot to save the array pointer across growth
    ctx.emitter.instruction("str x0, [sp]");                                    // preserve the array pointer before __rt_array_grow may reallocate it
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");                      // double capacity; x0 becomes the new array pointer
    ctx.emitter.instruction("add sp, sp, #16");                                 // release the temporary save slot, x0 holds the grown array pointer
    ctx.store_result_value(array)?;                                            // reflect the reallocated array pointer in the IR value
    ctx.load_value_to_reg(array, "x0")?;                                       // reload the grown array pointer for the inline shift

    ctx.emitter.label(&have_room_label);

    // -- persist the incoming string to heap-owned storage --
    ctx.load_string_value_to_regs(value, "x1", "x2")?;                         // load the volatile incoming string ptr/len
    ctx.emitter.instruction("sub sp, sp, #16");                                 // allocate a temporary slot to save the array pointer across persistence
    ctx.emitter.instruction("str x0, [sp]");                                    // preserve the array pointer before __rt_str_persist clobbers caller-saved registers
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");                     // copy the incoming string to owned heap storage
    ctx.emitter.instruction("ldr x0, [sp]");                                    // restore the array pointer after string persistence
    ctx.emitter.instruction("add sp, sp, #16");                                 // release the temporary save slot

    // -- slide existing string slots one position toward the back --
    ctx.emitter.instruction("ldr x9, [x0]");                                    // reload length after helper calls
    ctx.emitter.instruction("add x10, x0, #24");                                // compute the base address of the inline data region
    emit_array_unshift_compact_aarch64(ctx);                                   // move slot i to slot i+1 for i = length-1 .. 0

    // -- store the persisted string at slot 0 and publish the new length --
    ctx.emitter.instruction("str x1, [x0, #24]");                               // store the persisted string pointer in the first 16-byte slot
    ctx.emitter.instruction("str x2, [x0, #32]");                               // store the persisted string length in the first 16-byte slot
    ctx.emitter.instruction("ldr x9, [x0]");                                    // reload the original indexed-array length
    ctx.emitter.instruction("add x9, x9, #1");                                  // compute the new PHP count after prepending one string
    ctx.emitter.instruction("str x9, [x0]");                                    // write the updated logical length back to the array header
    // The caller reads the new count after write-back to avoid a result-register clobber.

    Ok(())
}

/// Fixes the shape of an empty indexed array so it can hold 16-byte string slots.
fn emit_array_unshift_str_first_write_aarch64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let shape_ready_label = ctx.next_label("array_unshift_str_shape_ready");
    ctx.emitter.instruction("ldr x9, [x0]");                                    // x9 = current array length before first-write specialization
    ctx.emitter.instruction(&format!("cbnz x9, {}", shape_ready_label));        // existing arrays already have their element shape fixed
    ctx.emitter.instruction("ldr x10, [x0, #16]");                              // x10 = old elem_size (8 for empty array<never> buffers)
    ctx.emitter.instruction("ldr x11, [x0, #8]");                               // x11 = old capacity counted in old-elem_size slots
    ctx.emitter.instruction("mul x11, x11, x10");                               // x11 = backing-store data bytes already reserved by __rt_array_new
    ctx.emitter.instruction("lsr x11, x11, #4");                                // reinterpret the same bytes as 16-byte string slots
    ctx.emitter.instruction("str x11, [x0, #8]");                               // publish slot-accurate capacity before the first 16-byte slot is written
    ctx.emitter.instruction("mov x10, #16");                                    // string slots carry pointer and length
    ctx.emitter.instruction("str x10, [x0, #16]");                              // elem_size = 16 before any future grow copies live string slots
    ctx.emitter.instruction("ldr x10, [x0, #-8]");                              // load packed array metadata for value_type stamping
    ctx.emitter.instruction("mov x11, #0x80ff");                                // keep indexed-array kind and persistent COW metadata only
    ctx.emitter.instruction("and x10, x10, x11");                               // clear any stale first-write value_type tag
    ctx.emitter.instruction("mov x11, #1");                                     // value_type 1 = string payload slots
    ctx.emitter.instruction("lsl x11, x11, #8");                                // move string value_type into the packed kind-word byte lane
    ctx.emitter.instruction("orr x10, x10, x11");                               // combine stable metadata with the string value_type tag
    ctx.emitter.instruction("str x10, [x0, #-8]");                              // publish string metadata before the first unshift
    ctx.emitter.label(&shape_ready_label);
    Ok(())
}

/// Slides AArch64 indexed-array string slots one slot toward the back.
///
/// Assumes x9 = original length and x10 = data base.  Iterates from the last live slot
/// down to slot 0 so the reverse copy does not overwrite unmoved slots.
fn emit_array_unshift_compact_aarch64(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("array_unshift_loop");
    let done_label = ctx.next_label("array_unshift_compact_done");
    ctx.emitter.instruction("sub x13, x9, #1");                                 // start the source cursor at the last live element
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x13, #0");                                     // compare the source cursor with the front of the array
    ctx.emitter.instruction(&format!("b.lt {}", done_label));                   // finish shifting once the cursor passes the front
    ctx.emitter.instruction("lsl x14, x13, #4");                                // scale the source cursor by the 16-byte string slot size
    ctx.emitter.instruction("add x15, x10, x14");                               // compute the source string slot address
    ctx.emitter.instruction("add x14, x14, #16");                               // compute the destination byte offset (one slot later)
    ctx.emitter.instruction("add x14, x10, x14");                               // compute the destination string slot address
    ctx.emitter.instruction("ldp x16, x17, [x15]");                             // load the trailing string payload that slides toward the back
    ctx.emitter.instruction("stp x16, x17, [x14]");                             // store the trailing string payload into the next slot
    ctx.emitter.instruction("sub x13, x13, #1");                                // move the source cursor toward the front
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue shifting until the front is reached
    ctx.emitter.label(&done_label);
}

/// Lowers x86_64 `array_unshift()` for a string-element indexed array.
///
/// The helper `__rt_array_unshift` only handles 8-byte slots, so 16-byte string slots are
/// shifted inline.  This path also handles the first-write shape change from an empty
/// `array<never>` buffer (8-byte slots) to a string array (16-byte slots).
fn lower_array_unshift_str_x86_64(
    ctx: &mut FunctionContext<'_>,
    array: ValueId,
    value: ValueId,
) -> Result<()> {
    let have_room_label = ctx.next_label("array_unshift_str_have_room");

    ctx.load_value_to_reg(array, "rdi")?;                                      // load the unique indexed-array pointer after COW splitting
    emit_array_unshift_str_first_write_x86_64(ctx)?;                           // fix up empty array<never> shape for 16-byte string slots

    ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                        // reload length after first-write shape specialization
    ctx.emitter.instruction("mov rcx, QWORD PTR [rdi + 8]");                    // load capacity counted in string slots
    ctx.emitter.instruction("cmp r10, rcx");                                    // compare logical length with allocated capacity
    ctx.emitter.instruction(&format!("jb {}", have_room_label));                // skip growth when at least one free slot remains

    ctx.emitter.instruction("sub rsp, 16");                                     // allocate a temporary slot to save the array pointer across growth
    ctx.emitter.instruction("mov QWORD PTR [rsp], rdi");                        // preserve the array pointer before __rt_array_grow may reallocate it
    abi::emit_call_label(ctx.emitter, "__rt_array_grow");                        // double capacity; rax becomes the new array pointer
    ctx.emitter.instruction("add rsp, 16");                                     // release the temporary save slot, rax holds the grown array pointer
    ctx.store_result_value(array)?;                                              // reflect the reallocated array pointer in the IR value
    ctx.load_value_to_reg(array, "rdi")?;                                        // reload the grown array pointer for the inline shift

    ctx.emitter.label(&have_room_label);

    // -- persist the incoming string to heap-owned storage --
    ctx.load_string_value_to_regs(value, "rsi", "rdx")?;                         // load the volatile incoming string ptr/len
    ctx.emitter.instruction("sub rsp, 16");                                     // allocate a temporary slot to save the array pointer across persistence
    ctx.emitter.instruction("mov QWORD PTR [rsp], rdi");                        // preserve the array pointer before __rt_str_persist clobbers caller-saved registers
    ctx.emitter.instruction("mov rax, rsi");                                    // move the string pointer into the x86_64 persist input register
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");                       // copy the incoming string to owned heap storage
    ctx.emitter.instruction("mov rsi, rax");                                    // move the persisted string pointer back to the string argument register
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // restore the array pointer after string persistence
    ctx.emitter.instruction("add rsp, 16");                                     // release the temporary save slot

    // -- slide existing string slots one position toward the back --
    ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                        // reload length after helper calls
    ctx.emitter.instruction("lea r11, [rdi + 24]");                             // compute the base address of the inline data region
    emit_array_unshift_compact_x86_64(ctx);                                      // move slot i to slot i+1 for i = length-1 .. 0

    // -- store the persisted string at slot 0 and publish the new length --
    ctx.emitter.instruction("mov QWORD PTR [rdi + 24], rsi");                   // store the persisted string pointer in the first 16-byte slot
    ctx.emitter.instruction("mov QWORD PTR [rdi + 32], rdx");                   // store the persisted string length in the first 16-byte slot
    ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                        // reload the original indexed-array length
    ctx.emitter.instruction("add r10, 1");                                      // compute the new PHP count after prepending one string
    ctx.emitter.instruction("mov QWORD PTR [rdi], r10");                        // write the updated logical length back to the array header
    // The caller reads the new count after write-back to avoid a result-register clobber.

    Ok(())
}

/// Fixes the shape of an empty indexed array so it can hold 16-byte string slots.
fn emit_array_unshift_str_first_write_x86_64(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let shape_ready_label = ctx.next_label("array_unshift_str_shape_ready");
    ctx.emitter.instruction("mov r10, QWORD PTR [rdi]");                        // r10 = current array length before first-write specialization
    ctx.emitter.instruction("test r10, r10");                                   // is this the first write into a freshly empty indexed array?
    ctx.emitter.instruction(&format!("jnz {}", shape_ready_label));             // existing arrays already have their element shape fixed
    ctx.emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                   // r10 = old elem_size (8 for empty array<never> buffers)
    ctx.emitter.instruction("mov r11, QWORD PTR [rdi + 8]");                    // r11 = old capacity counted in old-elem_size slots
    ctx.emitter.instruction("imul r11, r10");                                   // r11 = backing-store data bytes already reserved by __rt_array_new
    ctx.emitter.instruction("shr r11, 4");                                      // reinterpret the same bytes as 16-byte string slots
    ctx.emitter.instruction("mov QWORD PTR [rdi + 8], r11");                    // publish slot-accurate capacity before the first 16-byte slot is written
    ctx.emitter.instruction("mov QWORD PTR [rdi + 16], 16");                    // elem_size = 16 before any future grow copies live string slots
    ctx.emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                    // load packed array metadata for value_type stamping
    ctx.emitter.instruction("mov r11, 0xffffffff000080ff");                     // preserve heap marker, indexed-array kind, and persistent COW metadata
    ctx.emitter.instruction("and r10, r11");                                    // clear any stale first-write value_type tag
    ctx.emitter.instruction("or r10, 0x100");                                   // value_type 1 = string payload slots
    ctx.emitter.instruction("mov QWORD PTR [rdi - 8], r10");                    // publish string metadata before the first unshift
    ctx.emitter.label(&shape_ready_label);
    Ok(())
}

/// Slides x86_64 indexed-array string slots one slot toward the back.
///
/// Assumes r10 = original length and r11 = data base.  Iterates from the last live slot
/// down to slot 0 so the reverse copy does not overwrite unmoved slots.
fn emit_array_unshift_compact_x86_64(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("array_unshift_loop");
    let done_label = ctx.next_label("array_unshift_compact_done");
    ctx.emitter.instruction("mov rcx, r10");                                    // start the source cursor at the current logical length
    ctx.emitter.instruction("sub rcx, 1");                                      // move the source cursor to the last live element
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp rcx, 0");                                      // compare the source cursor with the front of the array
    ctx.emitter.instruction(&format!("jl {}", done_label));                     // finish shifting once the cursor passes the front
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 16]");             // load the trailing string pointer that slides toward the back
    ctx.emitter.instruction("mov QWORD PTR [r11 + rcx * 16 + 16], rax");        // store the trailing string pointer into the next slot
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 16 + 8]");         // load the trailing string length that slides toward the back
    ctx.emitter.instruction("mov QWORD PTR [r11 + rcx * 16 + 24], rax");        // store the trailing string length into the next slot
    ctx.emitter.instruction("sub rcx, 1");                                      // move the source cursor toward the front
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue shifting until the front is reached
    ctx.emitter.label(&done_label);
}
