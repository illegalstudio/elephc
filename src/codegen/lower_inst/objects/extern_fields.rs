//! Purpose:
//! Lowers field reads and writes through `ptr<ExternClass>` receivers, the C structs declared
//! with `extern class`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::known_property_reads::lower_prop_get()`.
//! - `crate::codegen::lower_inst::objects::runtime_property_writes::lower_prop_set()`.
//!
//! Key details:
//! - Each field is accessed at its natural C offset with its declared width through the shared
//!   `scalar_widths` emitter: integer reads sign or zero extend into the 64-bit PHP int, integer
//!   writes store only the low bytes, `f32` rounds through an IEEE 754 single, and `bool` is one
//!   byte read back as 0 or 1.
//! - A `string` (`char*`) field reads as a borrowed view (pointer plus scanned length) that
//!   lowering immediately copies with `StrPersist`; a write stores a fresh null-terminated copy
//!   on the elephc heap that the struct keeps, like an `extern global string` store.
//! - This is the extern sibling of the packed `ptr<T>` receiver route
//!   (`property_resolution::packed_receiver_class`): the receiver is the struct's raw address, and
//!   every access first aborts on null through `__rt_ptr_check_nonnull`, exactly like
//!   `emit_typed_pointer_receiver_null_check` does for packed receivers.

use super::super::scalar_widths::{emit_width_load, emit_width_store};
use super::*;
use crate::types::{ExternAbiType, ExternFieldInfo};

/// Integer scratch register that carries a store payload while the struct pointer is restored.
fn store_scratch_reg(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "rcx",
    }
}

/// Looks up the declared C field a `ptr<Class>` access names.
fn resolve_extern_field(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<ExternFieldInfo> {
    let class_info = ctx.module.extern_class_infos.get(class_name).ok_or_else(|| {
        CodegenIrError::unsupported(format!("unknown extern class {}", class_name))
    })?;
    class_info
        .fields
        .iter()
        .find(|field| field.name == property)
        .cloned()
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!(
                "{} for missing extern field {}::${}",
                inst.op.name(),
                class_name,
                property
            ))
        })
}

/// Lowers `$ptr->field` for a `ptr<ExternClass>` receiver into a width-aware load.
pub(super) fn lower_extern_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let field = resolve_extern_field(ctx, class_name, property, inst)?;
    // -- null-check the struct pointer --
    ctx.load_value_to_result(object)?;
    abi::emit_call_label(ctx.emitter, "__rt_ptr_check_nonnull");
    // -- load the field at its C width --
    let base_reg = abi::int_result_reg(ctx.emitter);
    emit_width_load(ctx, field.abi, base_reg, field.offset)?;
    store_if_result(ctx, inst)
}

/// Lowers `$ptr->field = value` for a `ptr<ExternClass>` receiver into a width-aware store.
pub(super) fn lower_extern_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    value: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let field = resolve_extern_field(ctx, class_name, property, inst)?;
    // -- null-check the struct pointer and keep it while the value loads --
    ctx.load_value_to_result(object)?;
    abi::emit_call_label(ctx.emitter, "__rt_ptr_check_nonnull");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let in_float_reg = materialize_extern_field_value(ctx, &field, value)?;
    // -- park the payload and restore the struct pointer --
    let arch = ctx.emitter.target.arch;
    if !in_float_reg {
        let scratch = store_scratch_reg(arch);
        let result = abi::int_result_reg(ctx.emitter);
        ctx.emitter.instruction(&format!("mov {}, {}", scratch, result));       // keep the field payload while the struct pointer is restored
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    // -- store the field at its C width --
    let base_reg = abi::int_result_reg(ctx.emitter);
    emit_width_store(ctx, field.abi, base_reg, field.offset, store_scratch_reg(arch))?;
    store_if_result(ctx, inst)
}

/// Loads the value to store into the register its field width is written from.
///
/// Returns `true` when the payload is a double in the float result register, `false` when it
/// is a word in the integer result register.
fn materialize_extern_field_value(
    ctx: &mut FunctionContext<'_>,
    field: &ExternFieldInfo,
    value: ValueId,
) -> Result<bool> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    match (field.abi, value_ty) {
        (ExternAbiType::Float64 | ExternAbiType::Float32, PhpType::Float) => {
            ctx.load_value_to_result(value)?;
            Ok(true)
        }
        (ExternAbiType::Float64 | ExternAbiType::Float32, PhpType::Int) => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
            Ok(true)
        }
        (ExternAbiType::Int { .. }, PhpType::Int)
        | (ExternAbiType::Bool, PhpType::Bool)
        | (ExternAbiType::Ptr, PhpType::Pointer(_)) => {
            ctx.load_value_to_result(value)?;
            Ok(false)
        }
        (ExternAbiType::Ptr, PhpType::Void) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(false)
        }
        (ExternAbiType::CStr, PhpType::Str) => {
            ctx.load_value_to_result(value)?;
            // The struct keeps the copy: C owns a `char*` field, so it outlives this statement.
            abi::emit_call_label(ctx.emitter, "__rt_str_to_cstr");
            Ok(false)
        }
        (abi_ty, other) => Err(CodegenIrError::unsupported(format!(
            "extern field {} store of PHP type {:?} into C shape {:?}",
            field.name, other, abi_ty
        ))),
    }
}
