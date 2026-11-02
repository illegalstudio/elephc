//! Purpose:
//! Emits declared, reference, and packed property loads.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Typed-property guards and target register shapes are preserved.

use super::*;

/// Emits the declared-property load into the canonical result register(s).
pub(super) fn emit_property_load(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    if slot.is_packed {
        return emit_packed_field_load(ctx, slot, base_reg);
    }
    if slot.is_reference {
        return emit_reference_property_load(ctx, slot, base_reg);
    }
    match slot.php_type.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            if base_reg == ptr_reg {
                abi::emit_load_from_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
                abi::emit_load_from_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            } else {
                abi::emit_load_from_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
                abi::emit_load_from_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
            }
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool | PhpType::False | PhpType::Int | PhpType::Void | PhpType::Never => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        PhpType::TaggedScalar => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            // Mixed-receiver dispatch hands the object pointer in the integer result register,
            // so loading the payload first would overwrite the base before the tag word is read
            // and the second load would dereference the payload. Same guard as the `Str` arm.
            if base_reg == int_reg {
                abi::emit_load_from_address(ctx.emitter, tag_reg, base_reg, slot.offset + 8);
                abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
            } else {
                abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
                abi::emit_load_from_address(ctx.emitter, tag_reg, base_reg, slot.offset + 8);
            }
        }
        ty if is_pointer_sized_property_type(&ty) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "property load for PHP type {:?}",
                slot.php_type
            )))
        }
    }
    Ok(())
}

/// Emits a declared reference-property load by dereferencing the stored ref-cell pointer.
pub(super) fn emit_reference_property_load(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    let pointer_reg = reference_pointer_reg(ctx, base_reg);
    abi::emit_load_from_address(ctx.emitter, pointer_reg, base_reg, slot.offset);
    match slot.php_type.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, pointer_reg, 0);
        }
        PhpType::TaggedScalar => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            let tag_reg = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, pointer_reg, 0);
            abi::emit_load_from_address(ctx.emitter, tag_reg, pointer_reg, 8);
        }
        ty if is_pointer_sized_property_type(&ty)
            || matches!(
                ty,
                PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never
            ) =>
        {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, pointer_reg, 0);
        }
        ty => {
            return Err(CodegenIrError::unsupported(format!(
                "reference property load for PHP type {:?}",
                ty
            )))
        }
    }
    Ok(())
}

/// Emits a compact packed-field load from a pointer to the containing packed record.
///
/// A width-declared field (`i32`, `u8`, `f32`, ...) loads through the shared width emitter,
/// which extends an integer into the PHP int and widens an `f32` into the PHP float.
pub(super) fn emit_packed_field_load(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    if let Some(width) = slot.width {
        return super::super::scalar_widths::emit_width_load(
            ctx,
            width.abi(),
            base_reg,
            slot.offset,
        );
    }
    match slot.php_type.codegen_repr() {
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool
        | PhpType::False
        | PhpType::Int
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        PhpType::Packed(_) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            if slot.offset == 0 {
                ctx.emitter
                    .instruction(&format!("mov {}, {}", int_reg, base_reg)); // return the nested packed field address directly
            } else {
                match ctx.emitter.target.arch {
                    // An AArch64 `add` immediate encodes only 0..=4095 unshifted, so a nested
                    // record placed further into its container adds the offset from a register.
                    Arch::AArch64 if slot.offset <= 4095 => {
                        ctx.emitter.instruction(&format!(
                            "add {}, {}, #{}",
                            int_reg, base_reg, slot.offset
                        ));                                                     // compute the nested packed field address
                    }
                    Arch::AArch64 => {
                        let offset_reg = if base_reg == abi::tertiary_scratch_reg(ctx.emitter) {
                            abi::secondary_scratch_reg(ctx.emitter)
                        } else {
                            abi::tertiary_scratch_reg(ctx.emitter)
                        };
                        // -- compute a nested field address past the add immediate range --
                        abi::emit_load_int_immediate(ctx.emitter, offset_reg, slot.offset as i64);
                        ctx.emitter.instruction(&format!(
                            "add {}, {}, {}",
                            int_reg, base_reg, offset_reg
                        ));                                                     // compute a nested packed field address beyond the add immediate range
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction(&format!(
                            "lea {}, [{} + {}]",
                            int_reg, base_reg, slot.offset
                        ));                                                     // compute the nested packed field address
                    }
                }
            }
        }
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "packed field load for PHP type {:?}",
                slot.php_type
            )))
        }
    }
    Ok(())
}
