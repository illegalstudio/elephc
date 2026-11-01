//! Purpose:
//! Lowers include-variant and include-once activation markers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a concrete include-loaded function variant activation marker.
pub(super) fn lower_function_variant_mark(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_data(inst)?;
    let label = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let parsed = function_variants::parse_variant_label(label).ok_or_else(|| {
        CodegenIrError::invalid_module(format!("invalid function variant label '{}'", label))
    })?;
    function_variants::emit_variant_mark(ctx.emitter, ctx.data, &parsed)
}

/// Lowers an include-once marker by setting its module-global guard symbol.
pub(super) fn lower_include_once_mark(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let label = include_once_label(ctx, inst)?;
    ctx.data.add_comm(label.clone(), 8);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &label, 0);
    Ok(())
}

/// Lowers an include-once guard to a boolean branch condition and marks first entry.
pub(super) fn lower_include_once_guard(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let label = include_once_label(ctx, inst)?;
    ctx.data.add_comm(label.clone(), 8);
    let already_label = ctx.next_label("include_once_already");
    let done_label = ctx.next_label("include_once_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &label, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &already_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, &label, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&already_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Resolves a typed source identity to its native request-state symbol.
pub(super) fn include_once_label(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<String> {
    let Some(Immediate::Source(id)) = &inst.immediate else {
        return Err(CodegenIrError::invalid_module("include marker requires a source id"));
    };
    let source = ctx.module.source_catalog().and_then(|catalog| catalog.get(*id))
        .ok_or_else(|| CodegenIrError::missing_entry("source", id.as_raw()))?;
    Ok(crate::codegen::source_units::include_guard_symbol(&source.canonical_path))
}


/// Links one synthetic `require_once`/`include_once` activation into the trace chain.
///
/// The record lives in the function's own frame (one 48-byte slot per include site) so nested
/// includes and recursive calls cannot alias it. `[16]` carries the site descriptor instead of a
/// suspended frame pointer: the trace walkers only compare that word with the record address to
/// recognise an ownership guard, and the unwinder reads it only when a cleanup callback exists
/// (these records install none). `[24]` is the shared reader, the same contract a compiled PHP
/// frame publishes.
pub(super) fn lower_include_frame_enter(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if !ctx.backtrace_enabled {
        return Ok(());
    }
    let Some(frame) = include_frame_immediate(inst)? else {
        return Ok(());
    };
    let Some(base) = ctx.include_frame_base else {
        return Ok(());
    };
    let record_offset = base + frame.slot as usize * 48;
    let source = ctx
        .module
        .source_catalog()
        .and_then(|catalog| catalog.get(frame.source))
        .ok_or_else(|| CodegenIrError::missing_entry("include frame source", frame.source.as_raw()))?;
    let path = source.canonical_path.to_string_lossy();
    let file = ctx
        .span_source_file(inst.span)
        .map(str::to_string)
        .or_else(|| ctx.module.source_path.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let line = u64::from(
        inst.span
            .filter(|span| span.is_from_source())
            .map_or(0, |span| span.line),
    );
    let construct: &[u8] = if frame.required {
        b"require_once"
    } else {
        b"include_once"
    };
    let (file_label, file_len) = ctx.data.add_string(file.as_bytes());
    let (path_label, path_len) = ctx.data.add_string(path.as_bytes());
    let (construct_label, construct_len) = ctx.data.add_string(construct);
    use crate::codegen_support::data_section::DataWord;
    let info = ctx.data.add_words(vec![
        DataWord::Symbol(construct_label),
        DataWord::U64(construct_len as u64),
        DataWord::U64(0),
        DataWord::U64(0),
        DataWord::U64(0),
        DataWord::U64(0),
    ]);
    // The descriptor is also the frame's `{file pointer, file length}` record, so the first two
    // words must stay in that order.
    let descriptor = ctx.data.add_words(vec![
        DataWord::Symbol(file_label),
        DataWord::U64(file_len as u64),
        DataWord::U64(line),
        DataWord::Symbol(info),
        DataWord::Symbol(path_label),
        DataWord::U64(path_len as u64),
    ]);
    let (record_reg, scratch) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x11", "x10"),
        Arch::X86_64 => ("r11", "rax"),
    };
    abi::emit_frame_slot_address(ctx.emitter, record_reg, record_offset);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_exc_call_frame_top", 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x10, [x11]");                          // [0] the previous activation
            ctx.emitter.instruction("stp xzr, xzr, [x11, #8]");                 // [8] no cleanup, [16] descriptor next
            abi::emit_symbol_address(ctx.emitter, "x10", &descriptor);
            ctx.emitter.instruction("str x10, [x11, #16]");                     // [16] this site's descriptor
            abi::emit_symbol_address(ctx.emitter, "x10", "__rt_include_frame_reader");
            ctx.emitter.instruction("str x10, [x11, #24]");                     // [24] the shared frame reader
            ctx.emitter.instruction("stp xzr, xzr, [x11, #32]");                // [32]/[40] no caller site
            abi::emit_store_reg_to_symbol(ctx.emitter, "x11", "_exc_call_frame_top", 0);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [r11], rax");                // [0] the previous activation
            ctx.emitter.instruction("mov QWORD PTR [r11 + 8], 0");              // [8] no cleanup callback
            abi::emit_symbol_address(ctx.emitter, "rax", &descriptor);
            ctx.emitter.instruction("mov QWORD PTR [r11 + 16], rax");           // [16] this site's descriptor
            abi::emit_symbol_address(ctx.emitter, "rax", "__rt_include_frame_reader");
            ctx.emitter.instruction("mov QWORD PTR [r11 + 24], rax");           // [24] the shared frame reader
            ctx.emitter.instruction("mov QWORD PTR [r11 + 32], 0");             // [32] no caller line
            ctx.emitter.instruction("mov QWORD PTR [r11 + 40], 0");             // [40] no caller file
            abi::emit_store_reg_to_symbol(ctx.emitter, "r11", "_exc_call_frame_top", 0);
        }
    }
    Ok(())
}

/// Unlinks the site's synthetic include activation when the inlined body falls through.
pub(super) fn lower_include_frame_exit(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if !ctx.backtrace_enabled {
        return Ok(());
    }
    let Some(frame) = include_frame_immediate(inst)? else {
        return Ok(());
    };
    let Some(base) = ctx.include_frame_base else {
        return Ok(());
    };
    let record_offset = base + frame.slot as usize * 48;
    let (record_reg, scratch) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x11", "x10"),
        Arch::X86_64 => ("r11", "rax"),
    };
    abi::emit_frame_slot_address(ctx.emitter, record_reg, record_offset);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("ldr x10, [x11]"),             // reload the enclosing activation
        Arch::X86_64 => ctx.emitter.instruction("mov rax, QWORD PTR [r11]"),    // reload the enclosing activation
    }
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_call_frame_top", 0);
    Ok(())
}

/// Extracts the include-frame immediate, or reports a malformed module.
fn include_frame_immediate(inst: &Instruction) -> Result<Option<&crate::ir::IncludeFrameImmediate>> {
    match inst.immediate.as_ref() {
        Some(Immediate::IncludeFrame(frame)) => Ok(Some(frame)),
        None => Ok(None),
        Some(_) => Err(CodegenIrError::invalid_module(
            "include frame op requires an include frame immediate",
        )),
    }
}

/// Lowers a void EIR opcode that maps directly to one runtime helper call.
pub(super) fn lower_runtime_void_call(ctx: &mut FunctionContext<'_>, label: &str) -> Result<()> {
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}
