//! Purpose:
//! Lowers compiler-extension pointer builtins for the EIR backend.
//! Covers raw null materialization, null tests, address arithmetic, raw memory loads/stores,
//! and byte-exact string copies through runtime helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Pointer values are raw machine addresses in the integer result register.
//! - Numeric pointer builtins do not allocate, box, retain, or release PHP runtime values.
//! - String pointer builtins delegate allocation/copy semantics to the existing runtime helpers.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::ir_global_symbol;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `ptr(value)` by materializing the address of addressable local/global storage.
pub(crate) fn lower_ptr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr", 1)?;
    let value = expect_operand(inst, 0)?;
    match pointer_source(ctx, value)? {
        PointerSource::Local { slot, is_ref_cell } => {
            let offset = ctx.local_offset(slot)?;
            if is_ref_cell || ctx.local_stores_ref_cell_pointer(slot) {
                abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
            } else {
                abi::emit_frame_slot_address(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
            }
        }
        PointerSource::Global { symbol, bytes } => {
            ctx.data.add_comm(symbol.clone(), bytes);
            abi::emit_symbol_address(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol);
        }
        PointerSource::Null => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `ptr_null()` by materializing the raw null pointer sentinel.
pub(crate) fn lower_ptr_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_null", 0)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    store_if_result(ctx, inst)
}

/// Lowers `ptr_is_null(pointer)` by comparing the raw pointer address to zero.
pub(crate) fn lower_ptr_is_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_is_null", 1)?;
    let pointer = expect_operand(inst, 0)?;
    load_pointer_payload(ctx, pointer, "ptr_is_null")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // compare the raw pointer payload against the null address
            ctx.emitter.instruction("cset x0, eq");                             // return true only when the pointer payload is null
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // compare the raw pointer payload against the null address
            ctx.emitter.instruction("sete al");                                 // materialize the null test result in the low byte
            ctx.emitter.instruction("movzx rax, al");                           // widen the null test result to the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `ptr_sizeof("type")` by materializing the checked static byte size.
pub(crate) fn lower_ptr_sizeof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_sizeof", 1)?;
    let type_name = const_string_operand(ctx, expect_operand(inst, 0)?)?;
    let size = pointer_target_size(ctx, &type_name).ok_or_else(|| {
        CodegenIrError::unsupported(format!("ptr_sizeof type {:?}", type_name))
    })?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), size as i64);
    store_if_result(ctx, inst)
}

/// Lowers `ptr_offset(pointer, offset)` by adding a byte offset to a raw address.
pub(crate) fn lower_ptr_offset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_offset", 2)?;
    let pointer = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    load_pointer_payload(ctx, pointer, "ptr_offset")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    super::super::resolve_int_operand_to_result(ctx, offset, "ptr_offset offset")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x10, x0");                             // preserve the byte offset while restoring the base pointer
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("add x0, x0, x10");                         // compute the derived raw pointer address
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, rax");                            // preserve the byte offset while restoring the base pointer
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("add rax, r10");                            // compute the derived raw pointer address
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `ptr_get(pointer)` by reading one machine word through a checked pointer.
pub(crate) fn lower_ptr_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_read(ctx, inst, "ptr_get", PointerWidth::Word64)
}

/// Lowers `ptr_set(pointer, value)` by writing one machine word through a checked pointer.
pub(crate) fn lower_ptr_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_write(ctx, inst, "ptr_set", PointerWidth::Word64, WordValuePolicy::Word)
}

/// Lowers `ptr_read8(pointer)` by reading one unsigned byte through a checked pointer.
pub(crate) fn lower_ptr_read8(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_read(ctx, inst, "ptr_read8", PointerWidth::Byte)
}

/// Lowers `ptr_read16(pointer)` by reading one unsigned 16-bit word through a checked pointer.
pub(crate) fn lower_ptr_read16(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_read(ctx, inst, "ptr_read16", PointerWidth::Half)
}

/// Lowers `ptr_read32(pointer)` by reading one unsigned 32-bit word through a checked pointer.
pub(crate) fn lower_ptr_read32(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_read(ctx, inst, "ptr_read32", PointerWidth::Word32)
}

/// Lowers `ptr_read_string(pointer, length)` by copying raw bytes into an owned PHP string.
pub(crate) fn lower_ptr_read_string(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_read_string", 2)?;
    let pointer = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_checked_pointer(ctx, pointer, "ptr_read_string")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int_value(ctx.load_value_to_result(length)?, "ptr_read_string length")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the requested byte length to the runtime string-copy helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // pass the requested byte length to the runtime string-copy helper
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ptr_read_string");
    store_if_result(ctx, inst)
}

/// Lowers `ptr_write8(pointer, value)` by writing one byte through a checked pointer.
pub(crate) fn lower_ptr_write8(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_write(ctx, inst, "ptr_write8", PointerWidth::Byte, WordValuePolicy::IntOnly)
}

/// Lowers `ptr_write16(pointer, value)` by writing one 16-bit word through a checked pointer.
pub(crate) fn lower_ptr_write16(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_write(ctx, inst, "ptr_write16", PointerWidth::Half, WordValuePolicy::IntOnly)
}

/// Lowers `ptr_write32(pointer, value)` by writing one 32-bit word through a checked pointer.
pub(crate) fn lower_ptr_write32(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_pointer_write(ctx, inst, "ptr_write32", PointerWidth::Word32, WordValuePolicy::IntOnly)
}

/// Lowers `ptr_write_string(pointer, string)` by copying PHP string bytes into raw memory.
pub(crate) fn lower_ptr_write_string(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "ptr_write_string", 2)?;
    let pointer = expect_operand(inst, 0)?;
    let string = expect_operand(inst, 1)?;
    load_checked_pointer(ctx, pointer, "ptr_write_string")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(string, ptr_reg, len_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ptr_write_string");
    store_if_result(ctx, inst)
}

/// Returns the literal string payload for a `ConstStr` operand.
fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "ptr_sizeof with non-literal type name",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Err(CodegenIrError::unsupported(
            "ptr_sizeof with non-literal type name",
        ));
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "ptr_sizeof string literal has no data id",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Addressable storage source accepted by the `ptr()` builtin.
enum PointerSource {
    Local { slot: LocalSlotId, is_ref_cell: bool },
    Global { symbol: String, bytes: usize },
    Null,
}

/// Resolves the lowered `ptr()` operand back to addressable storage metadata.
fn pointer_source(ctx: &FunctionContext<'_>, value: ValueId) -> Result<PointerSource> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(PointerSource::Null);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    match inst_ref.op {
        Op::LoadLocal | Op::LoadRefCell => {
            let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
                return Err(CodegenIrError::invalid_module(
                    "ptr() local load has no local slot",
                ));
            };
            Ok(PointerSource::Local {
                slot,
                is_ref_cell: inst_ref.op == Op::LoadRefCell,
            })
        }
        Op::LoadGlobal => {
            let Some(Immediate::GlobalName(data)) = inst_ref.immediate else {
                return Err(CodegenIrError::invalid_module(
                    "ptr() global load has no global name",
                ));
            };
            let name = ctx.global_name_data(data)?;
            let symbol = ir_global_symbol(name);
            let bytes = ctx.value_php_type(value)?.codegen_repr().stack_size().max(8);
            Ok(PointerSource::Global { symbol, bytes })
        }
        _ => Ok(PointerSource::Null),
    }
}

/// Computes the byte size for a checked pointer target type name.
fn pointer_target_size(ctx: &FunctionContext<'_>, type_name: &str) -> Option<usize> {
    match type_name {
        "int" | "integer" => Some(8),
        "float" | "double" | "real" => Some(8),
        "bool" | "boolean" => Some(8),
        "string" => Some(16),
        "ptr" | "pointer" => Some(8),
        class_name => ctx
            .module
            .class_infos
            .get(class_name)
            .map(|info| {
                let dynamic_slot = if info.allow_dynamic_properties { 8 } else { 0 };
                8 + info.properties.len() * 16 + dynamic_slot
            })
            .or_else(|| {
                ctx.module
                    .extern_class_infos
                    .get(class_name)
                    .map(|info| info.total_size)
            })
            .or_else(|| {
                ctx.module
                    .packed_class_infos
                    .get(class_name)
                    .map(|info| info.total_size)
            }),
    }
}

/// Native integer width for raw pointer memory reads and writes.
#[derive(Clone, Copy)]
enum PointerWidth {
    Byte,
    Half,
    Word32,
    Word64,
}

/// Controls which PHP value types a pointer write builtin can materialize as a raw word.
#[derive(Clone, Copy)]
enum WordValuePolicy {
    IntOnly,
    Word,
}

/// Lowers a raw pointer memory read after validating the pointer against null.
fn lower_pointer_read(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    width: PointerWidth,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    let pointer = expect_operand(inst, 0)?;
    load_checked_pointer(ctx, pointer, name)?;
    emit_width_load(ctx, width);
    store_if_result(ctx, inst)
}

/// Lowers a raw pointer memory write after validating the destination pointer against null.
fn lower_pointer_write(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    width: PointerWidth,
    policy: WordValuePolicy,
) -> Result<()> {
    ensure_arg_count(inst, name, 2)?;
    let pointer = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    load_checked_pointer(ctx, pointer, name)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_word_value(ctx, value, name, policy)?;
    emit_width_store(ctx, width);
    emit_void_result(ctx);
    store_if_result(ctx, inst)
}

/// Loads a pointer operand, validates its type, and aborts at runtime if it is null.
fn load_checked_pointer(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    load_pointer_payload(ctx, value, name)?;
    abi::emit_call_label(ctx.emitter, "__rt_ptr_check_nonnull");
    Ok(())
}

/// Loads a pointer operand into the canonical integer result register.
fn load_pointer_payload(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Pointer(_) => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            emit_mixed_payload_to_result(ctx);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for pointer PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Moves the low payload word returned by `__rt_mixed_unbox` into the pointer result register.
fn emit_mixed_payload_to_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // use the unboxed Mixed low word as the raw pointer payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // use the unboxed Mixed low word as the raw pointer payload
        }
    }
}

/// Materializes an EIR value as the raw word payload for a pointer store.
fn materialize_word_value(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
    policy: WordValuePolicy,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            Ok(())
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never if matches!(policy, WordValuePolicy::Word) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Pointer(_) if matches!(policy, WordValuePolicy::Word) => {
            ctx.load_value_to_result(value)?;
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) if matches!(policy, WordValuePolicy::Word) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            emit_mixed_payload_to_result(ctx);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} value PHP type {:?}",
            name,
            other
        ))),
    }
}

/// Emits the target-specific load for a raw pointer memory width.
fn emit_width_load(ctx: &mut FunctionContext<'_>, width: PointerWidth) {
    match (ctx.emitter.target.arch, width) {
        (Arch::AArch64, PointerWidth::Byte) => {
            ctx.emitter.instruction("ldrb w0, [x0]");                           // load one unsigned byte and zero-extend it as a PHP integer
        }
        (Arch::AArch64, PointerWidth::Half) => {
            ctx.emitter.instruction("ldrh w0, [x0]");                           // load one unsigned 16-bit word and zero-extend it as a PHP integer
        }
        (Arch::AArch64, PointerWidth::Word32) => {
            ctx.emitter.instruction("ldr w0, [x0]");                            // load one unsigned 32-bit word and zero-extend it as a PHP integer
        }
        (Arch::AArch64, PointerWidth::Word64) => {
            ctx.emitter.instruction("ldr x0, [x0]");                            // load one machine word as a PHP integer
        }
        (Arch::X86_64, PointerWidth::Byte) => {
            ctx.emitter.instruction("movzx eax, BYTE PTR [rax]");               // load one unsigned byte and zero-extend it as a PHP integer
        }
        (Arch::X86_64, PointerWidth::Half) => {
            ctx.emitter.instruction("movzx eax, WORD PTR [rax]");               // load one unsigned 16-bit word and zero-extend it as a PHP integer
        }
        (Arch::X86_64, PointerWidth::Word32) => {
            ctx.emitter.instruction("mov eax, DWORD PTR [rax]");                // load one unsigned 32-bit word and zero-extend it as a PHP integer
        }
        (Arch::X86_64, PointerWidth::Word64) => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load one machine word as a PHP integer
        }
    }
}

/// Emits the target-specific store for a raw pointer memory width.
fn emit_width_store(ctx: &mut FunctionContext<'_>, width: PointerWidth) {
    match (ctx.emitter.target.arch, width) {
        (Arch::AArch64, PointerWidth::Byte) => {
            ctx.emitter.instruction("mov w10, w0");                             // preserve the low byte payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("strb w10, [x0]");                          // store one byte through the checked pointer
        }
        (Arch::AArch64, PointerWidth::Half) => {
            ctx.emitter.instruction("mov w10, w0");                             // preserve the low 16-bit payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("strh w10, [x0]");                          // store one 16-bit word through the checked pointer
        }
        (Arch::AArch64, PointerWidth::Word32) => {
            ctx.emitter.instruction("mov w10, w0");                             // preserve the low 32-bit payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("str w10, [x0]");                           // store one 32-bit word through the checked pointer
        }
        (Arch::AArch64, PointerWidth::Word64) => {
            ctx.emitter.instruction("mov x10, x0");                             // preserve the machine-word payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("str x10, [x0]");                           // store one machine word through the checked pointer
        }
        (Arch::X86_64, PointerWidth::Byte) => {
            ctx.emitter.instruction("mov cl, al");                              // preserve the low byte payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov BYTE PTR [rax], cl");                  // store one byte through the checked pointer
        }
        (Arch::X86_64, PointerWidth::Half) => {
            ctx.emitter.instruction("mov cx, ax");                              // preserve the low 16-bit payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov WORD PTR [rax], cx");                  // store one 16-bit word through the checked pointer
        }
        (Arch::X86_64, PointerWidth::Word32) => {
            ctx.emitter.instruction("mov ecx, eax");                            // preserve the low 32-bit payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov DWORD PTR [rax], ecx");                // store one 32-bit word through the checked pointer
        }
        (Arch::X86_64, PointerWidth::Word64) => {
            ctx.emitter.instruction("mov rcx, rax");                            // preserve the machine-word payload while restoring the pointer
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov QWORD PTR [rax], rcx");                // store one machine word through the checked pointer
        }
    }
}

/// Materializes the EIR void/null sentinel for storing a void write result.
fn emit_void_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
}

/// Verifies a pointer builtin received the expected number of operands.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Lowers `zval_pack(value)` by boxing the operand as a Mixed cell and invoking
/// `__rt_zval_pack`, which returns a pointer to a freshly allocated 16-byte zval.
///
/// `__rt_zval_pack` only reads the `(tag, lo, hi)` triple out of the boxed Mixed
/// cell; it never retains or frees that cell. When the operand was not already
/// Mixed/Union, `emit_box_current_value_as_mixed` allocated a fresh owned box
/// (persisting strings, increfing array/object/mixed children), so that box is a
/// throwaway temporary that must be deep-released after the pack call or it leaks
/// one Mixed cell per call. When the operand is already Mixed/Union no box was
/// created, so the operand's own live cell must not be freed here.
pub(crate) fn lower_zval_pack(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "zval_pack", 1)?;
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    let boxed_a_temporary = !matches!(value_ty, PhpType::Mixed | PhpType::Union(_));
    ctx.load_value_to_result(value)?;
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &value_ty);
    if boxed_a_temporary {
        let box_reg = abi::int_result_reg(ctx.emitter);
        abi::emit_push_reg(ctx.emitter, box_reg);
        abi::emit_call_label(ctx.emitter, "__rt_zval_pack");
        abi::emit_push_reg(ctx.emitter, box_reg);                              // stash the returned zval pointer across the box release
        emit_load_temp_box_for_release(ctx);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_free_deep");
        abi::emit_pop_reg(ctx.emitter, box_reg);                               // recover the zval pointer as the builtin result
        emit_drop_temp_box_slot(ctx);
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_zval_pack");
    }
    store_if_result(ctx, inst)
}

/// Loads the throwaway Mixed box pointer so it can be released after `zval_pack`.
fn emit_load_temp_box_for_release(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the throwaway box pointer from the deeper temporary slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the throwaway box pointer from the deeper temporary slot
        }
    }
}

/// Drops the temporary stack slot that held the throwaway Mixed box pointer.
fn emit_drop_temp_box_slot(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add sp, sp, #16");                         // discard the freed box slot without clobbering the zval result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 16");                             // discard the freed box slot without clobbering the zval result
        }
    }
}

/// Lowers `zval_unpack(zval_ptr)` by invoking `__rt_zval_unpack`.
pub(crate) fn lower_zval_unpack(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "zval_unpack", 1)?;
    let pointer = expect_operand(inst, 0)?;
    load_pointer_payload(ctx, pointer, "zval_unpack")?;
    abi::emit_call_label(ctx.emitter, "__rt_zval_unpack");
    store_if_result(ctx, inst)
}

/// Lowers `zval_type(zval_ptr)` by returning the PHP `IS_*` type byte.
pub(crate) fn lower_zval_type(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "zval_type", 1)?;
    let pointer = expect_operand(inst, 0)?;
    load_pointer_payload(ctx, pointer, "zval_type")?;
    abi::emit_call_label(ctx.emitter, "__rt_zval_type");
    store_if_result(ctx, inst)
}

/// Lowers `zval_free(zval_ptr)` by releasing the zval block and owned children.
pub(crate) fn lower_zval_free(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "zval_free", 1)?;
    let pointer = expect_operand(inst, 0)?;
    load_pointer_payload(ctx, pointer, "zval_free")?;
    abi::emit_call_label(ctx.emitter, "__rt_zval_free");
    store_if_result(ctx, inst)
}

/// Verifies a pointer string-copy length operand is a concrete PHP integer.
fn require_int_value(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} PHP type {:?}",
            name,
            other
        ))),
    }
}
