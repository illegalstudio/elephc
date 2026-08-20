//! Purpose:
//! Lowers PHP's ten iconv builtins from typed EIR into one shared runtime call.
//!
//! Called from:
//! - Typed runtime-function dispatch for the iconv family.
//!
//! Key details:
//! - Every builtin stages its arguments into the same uniform slot block, so the whole
//!   family shares one call convention and one pair of runtime entry points.
//! - Slot presence is what separates an omitted or `null` argument from an explicitly
//!   empty string, which PHP resolves to two different charsets.
//! - `iconv_mime_encode()`'s `$options` array is read at the call site, because only the
//!   backend can see the receiver's runtime storage.
//! - Argument staging is target-neutral: the shared stack-field helpers own every
//!   register and addressing decision.

use crate::codegen::{abi, CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::store_if_result;
use super::strings::{load_as_int, load_string_arg_to_regs};

/// Operation codes shared with `elephc_iconv`'s `abi::args` module.
const OP_CONVERT: i64 = 0;
const OP_STRLEN: i64 = 1;
const OP_SUBSTR: i64 = 2;
const OP_STRPOS: i64 = 3;
const OP_STRRPOS: i64 = 4;
const OP_MIME_ENCODE: i64 = 5;
const OP_MIME_DECODE: i64 = 6;
const OP_MIME_DECODE_HEADERS: i64 = 7;
const OP_GET_ENCODING: i64 = 8;
const OP_SET_ENCODING: i64 = 9;

/// Bytes reserved for the staged argument block, matching `IconvCallArgs`.
const BLOCK_SIZE: usize = 272;

/// Offset of the first argument slot inside the block.
const SLOTS_BASE: usize = 16;

/// Bytes one argument slot occupies.
const SLOT_SIZE: usize = 32;

/// Number of argument slots the block reserves.
const SLOT_COUNT: usize = 8;

/// Field offsets inside one argument slot, matching `IconvArgSlot`.
const SLOT_PRESENT: usize = 0;
const SLOT_PTR: usize = 8;
const SLOT_LEN: usize = 16;
const SLOT_INT: usize = 24;

/// Slot reserved for the resolved `iconv_mime_encode()` option table.
///
/// No operation stages an argument here, and its presence flag stays clear, so the bridge
/// ignores it while the lowering uses its integer field as call-site scratch.
const OPTIONS_SCRATCH_SLOT: usize = 7;

/// One `iconv_mime_encode()` option, its slot, and how it must be normalized.
const MIME_OPTIONS: &[(&str, usize, bool)] = &[
    ("scheme", 2, false),
    ("output-charset", 3, false),
    ("input-charset", 4, false),
    ("line-length", 5, true),
    ("line-break-chars", 6, false),
];

/// How one PHP argument is staged into its slot.
enum Staged {
    /// A required string argument.
    Text(usize),
    /// A nullable string argument; a statically `null` value stays absent.
    OptionalText(usize),
    /// An integer argument with a PHP default used when it is omitted.
    Number(usize, i64),
    /// A nullable integer argument; a statically `null` value stays absent.
    OptionalNumber(usize),
}

/// Lowers `iconv(from, to, string)`.
pub(crate) fn lower_iconv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv",
        OP_CONVERT,
        &[Staged::Text(0), Staged::Text(1), Staged::Text(2)],
        3,
        3,
    )
}

/// Lowers `iconv_strlen(string, encoding?)`.
pub(crate) fn lower_iconv_strlen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv_strlen",
        OP_STRLEN,
        &[Staged::Text(0), Staged::OptionalText(1)],
        1,
        2,
    )
}

/// Lowers `iconv_substr(string, offset, length?, encoding?)`.
pub(crate) fn lower_iconv_substr(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv_substr",
        OP_SUBSTR,
        &[
            Staged::Text(0),
            Staged::Number(1, 0),
            Staged::OptionalNumber(2),
            Staged::OptionalText(3),
        ],
        2,
        4,
    )
}

/// Lowers `iconv_strpos(haystack, needle, offset?, encoding?)`.
pub(crate) fn lower_iconv_strpos(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv_strpos",
        OP_STRPOS,
        &[
            Staged::Text(0),
            Staged::Text(1),
            Staged::Number(2, 0),
            Staged::OptionalText(3),
        ],
        2,
        4,
    )
}

/// Lowers `iconv_strrpos(haystack, needle, encoding?)`.
pub(crate) fn lower_iconv_strrpos(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv_strrpos",
        OP_STRRPOS,
        &[Staged::Text(0), Staged::Text(1), Staged::OptionalText(2)],
        2,
        3,
    )
}

/// Lowers `iconv_mime_decode(string, mode?, encoding?)`.
pub(crate) fn lower_iconv_mime_decode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv_mime_decode",
        OP_MIME_DECODE,
        &[
            Staged::Text(0),
            Staged::Number(1, 0),
            Staged::OptionalText(2),
        ],
        1,
        3,
    )
}

/// Lowers `iconv_mime_decode_headers(headers, mode?, encoding?)`.
pub(crate) fn lower_iconv_mime_decode_headers(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv_mime_decode_headers",
        OP_MIME_DECODE_HEADERS,
        &[
            Staged::Text(0),
            Staged::Number(1, 0),
            Staged::OptionalText(2),
        ],
        1,
        3,
    )
}

/// Lowers `iconv_get_encoding(type?)`.
pub(crate) fn lower_iconv_get_encoding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_family(
        ctx,
        inst,
        "iconv_get_encoding",
        OP_GET_ENCODING,
        &[Staged::OptionalText(0)],
        0,
        1,
    )
}

/// Lowers `iconv_set_encoding(type, encoding)`, whose result is a plain boolean.
pub(crate) fn lower_iconv_set_encoding(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arity(inst, "iconv_set_encoding", 2, 2)?;
    stage_block(ctx, inst, "iconv_set_encoding", OP_SET_ENCODING, &[
        Staged::Text(0),
        Staged::Text(1),
    ])?;
    finish(ctx, inst, "__rt_iconv_call_bool")
}

/// Lowers `iconv_mime_encode(field_name, field_value, options?)`.
pub(crate) fn lower_iconv_mime_encode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arity(inst, "iconv_mime_encode", 2, 3)?;
    stage_block(ctx, inst, "iconv_mime_encode", OP_MIME_ENCODE, &[
        Staged::Text(0),
        Staged::Text(1),
    ])?;
    stage_mime_options(ctx, inst)?;
    finish(ctx, inst, "__rt_iconv_call")
}

/// Stages one iconv builtin's arguments and calls the shared value-returning entry point.
fn lower_family(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    op: i64,
    staged: &[Staged],
    min_args: usize,
    max_args: usize,
) -> Result<()> {
    ensure_arity(inst, name, min_args, max_args)?;
    stage_block(ctx, inst, name, op, staged)?;
    finish(ctx, inst, "__rt_iconv_call")
}

/// Rejects an operand count the registry's declared arity cannot produce.
fn ensure_arity(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    if inst.operands.len() < min || inst.operands.len() > max {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected {} to {} args, got {}",
            name,
            min,
            max,
            inst.operands.len()
        )));
    }
    Ok(())
}

/// Reserves the argument block, clears every presence flag, and stages the operands.
fn stage_block(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    op: i64,
    staged: &[Staged],
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, BLOCK_SIZE);
    store_immediate(ctx, 0, op);
    for slot in 0..SLOT_COUNT {
        store_immediate(ctx, slot_field(slot, SLOT_PRESENT), 0);
    }
    for (index, entry) in staged.iter().enumerate() {
        match entry {
            Staged::Text(slot) => stage_text(ctx, inst, index, *slot, name, false)?,
            Staged::OptionalText(slot) => stage_text(ctx, inst, index, *slot, name, true)?,
            Staged::Number(slot, default) => {
                stage_number(ctx, inst, index, *slot, name, Some(*default))?
            }
            Staged::OptionalNumber(slot) => stage_number(ctx, inst, index, *slot, name, None)?,
        }
    }
    Ok(())
}

/// Publishes the bridge entries, calls one runtime entry point, and stores the result.
fn finish(ctx: &mut FunctionContext<'_>, inst: &Instruction, entry: &str) -> Result<()> {
    crate::codegen::iconv_bridge::publish_elephc_iconv_function_pointers(ctx.emitter);
    stage_block_pointer(ctx);
    abi::emit_call_label(ctx.emitter, entry);
    abi::emit_release_temporary_stack(ctx.emitter, BLOCK_SIZE);
    store_if_result(ctx, inst)
}

/// Stages one string operand, leaving the slot absent when PHP passed `null`.
fn stage_text(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operand: usize,
    slot: usize,
    name: &str,
    nullable: bool,
) -> Result<()> {
    let Some(value) = inst.operands.get(operand).copied() else {
        return Ok(());
    };
    if nullable && matches!(ctx.value_php_type(value)?, PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let (ptr_reg, len_reg) = (ptr_reg.to_string(), len_reg.to_string());
    load_string_arg_to_regs(ctx, inst, operand, name, &ptr_reg, &len_reg)?;
    store_register(ctx, slot_field(slot, SLOT_PTR), &ptr_reg);
    store_register(ctx, slot_field(slot, SLOT_LEN), &len_reg);
    store_immediate(ctx, slot_field(slot, SLOT_PRESENT), 1);
    Ok(())
}

/// Stages one integer operand, or its PHP default when the argument is omitted.
fn stage_number(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    operand: usize,
    slot: usize,
    name: &str,
    default: Option<i64>,
) -> Result<()> {
    let value = inst.operands.get(operand).copied();
    let supplied = match value {
        Some(value) => !matches!(ctx.value_php_type(value)?, PhpType::Void | PhpType::Never),
        None => false,
    };
    if !supplied {
        if let Some(default) = default {
            store_immediate(ctx, slot_field(slot, SLOT_INT), default);
            store_immediate(ctx, slot_field(slot, SLOT_PRESENT), 1);
        }
        return Ok(());
    }
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    load_as_int(ctx, value.expect("supplied operand"), name)?;
    store_register(ctx, slot_field(slot, SLOT_INT), &result_reg);
    store_immediate(ctx, slot_field(slot, SLOT_PRESENT), 1);
    Ok(())
}

/// Reads `iconv_mime_encode()`'s recognized option keys out of the caller's array.
///
/// The receiver is resolved to a hash pointer once and parked in the scratch slot, because
/// each lookup clobbers the argument registers. A receiver whose static type is `mixed`
/// arrives boxed and is unboxed at run time; only an associative array can carry PHP
/// string keys, so every other shape leaves the option slots absent and the bridge falls
/// back to php-src's defaults.
fn stage_mime_options(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let Some(value) = inst.operands.get(2).copied() else {
        return Ok(());
    };
    let receiver = ctx.value_php_type(value)?.codegen_repr();
    let boxed = match receiver {
        PhpType::AssocArray { .. } => false,
        PhpType::Mixed | PhpType::Union(_) => true,
        _ => return Ok(()),
    };
    ctx.load_value_to_result(value)?;
    if boxed {
        abi::emit_call_label(ctx.emitter, "__rt_iconv_option_table");
    }
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    store_register(ctx, slot_field(OPTIONS_SCRATCH_SLOT, SLOT_INT), &result_reg);
    for (key, slot, wants_int) in MIME_OPTIONS {
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            &result_reg,
            slot_field(OPTIONS_SCRATCH_SLOT, SLOT_INT),
        );
        emit_option_lookup(ctx, key, *slot, *wants_int);
    }
    Ok(())
}

/// Emits one `__rt_iconv_mime_option` call for the receiver already in the result register.
fn emit_option_lookup(
    ctx: &mut FunctionContext<'_>,
    key: &str,
    slot: usize,
    wants_int: bool,
) {
    let (label, key_len) = ctx.data.add_string(key.as_bytes());
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_temporary_stack_address(ctx.emitter, "x3", slot_field(slot, 0));
            abi::emit_load_int_immediate(ctx.emitter, "x4", i64::from(wants_int));
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // C arg0 = options table pointer
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_temporary_stack_address(ctx.emitter, "rcx", slot_field(slot, 0));
            abi::emit_load_int_immediate(ctx.emitter, "r8", i64::from(wants_int));
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_iconv_mime_option");
}

/// Returns the byte offset of one field inside one argument slot.
fn slot_field(slot: usize, field: usize) -> usize {
    SLOTS_BASE + slot * SLOT_SIZE + field
}

/// Writes one literal integer into the staged argument block.
fn store_immediate(ctx: &mut FunctionContext<'_>, offset: usize, value: i64) {
    let result_reg = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_load_int_immediate(ctx.emitter, &result_reg, value);
    store_register(ctx, offset, &result_reg);
}

/// Writes one register into the staged argument block.
fn store_register(ctx: &mut FunctionContext<'_>, offset: usize, register: &str) {
    let scratch = abi::symbol_scratch_reg(ctx.emitter).to_string();
    abi::emit_temporary_stack_address(ctx.emitter, &scratch, offset);
    abi::emit_store_to_address(ctx.emitter, register, &scratch, 0);
}

/// Places the staged argument block's address in the runtime helper's first argument.
fn stage_block_pointer(ctx: &mut FunctionContext<'_>) {
    let arg_reg = match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x0",
        crate::codegen::platform::Arch::X86_64 => "rdi",
    };
    abi::emit_temporary_stack_address(ctx.emitter, arg_reg, 0);
}
