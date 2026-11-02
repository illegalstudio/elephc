//! Purpose:
//! Copies fixed-layout packed records into buffer elements and embedded packed fields.
//!
//! Called from:
//! - Buffer assignment and packed property-store lowering.
//!
//! Key details:
//! - Sources and destinations have the same packed class and are identical or disjoint.
//! - Copies the complete stride, including padding and narrow-field tails, without allocation.
//! - Typed pointer sources are null-checked; their remaining lifetime is caller-managed.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::types::PhpType;

use super::super::context::FunctionContext;

/// Copies a whole packed record into a checked buffer element or embedded field.
///
/// `value` holds the address of a record of the same packed class (a buffer element, another
/// nested field, or a `ptr<T>`); its `total_size` bytes are copied into the field at
/// `base_reg + field_offset`, one 8-byte word at a time and then the tail a record of
/// width-declared fields can leave (4, 2 and 1 bytes, in that order). Like C struct assignment,
/// the copy assumes the source and the destination are the same record or do not overlap. A
/// `ptr<T>` source is null-checked first. The copy advances `base_reg`, the source register and
/// two scratch registers; the store is the last thing the write emits, so none of them
/// is read afterwards.
pub(super) fn emit_packed_record_copy(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    record: &str,
    field_offset: usize,
    base_reg: &str,
) -> Result<()> {
    let size = ctx
        .module
        .packed_class_infos
        .get(record.trim_start_matches('\\'))
        .map(|info| info.total_size)
        .ok_or_else(|| {
            CodegenIrError::unsupported(format!("unknown packed record type {}", record))
        })?;
    // -- load the source record address and null-check a ptr<T> source --
    let source_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, base_reg);
    ctx.load_value_to_reg(value, source_reg)?;
    abi::emit_pop_reg(ctx.emitter, base_reg);
    if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Pointer(_)) {
        abi::emit_call_label(ctx.emitter, "__rt_ptr_check_nonnull");
    }
    if size == 0 {
        return Ok(());
    }
    // Resolved buffer addresses can occupy either scratch register. Keep both the copied
    // payload and the loop count separate from the destination pointer on every target.
    let data_reg = if base_reg == abi::secondary_scratch_reg(ctx.emitter) {
        abi::symbol_scratch_reg(ctx.emitter)
    } else {
        abi::secondary_scratch_reg(ctx.emitter)
    };
    let count_reg = if base_reg == abi::tertiary_scratch_reg(ctx.emitter) {
        abi::symbol_scratch_reg(ctx.emitter)
    } else {
        abi::tertiary_scratch_reg(ctx.emitter)
    };
    let loop_label = ctx.next_label("packed_record_copy");
    if field_offset != 0 {
        // -- point the destination at the embedded record --
        abi::emit_load_int_immediate(ctx.emitter, count_reg, field_offset as i64);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("add {}, {}, {}", base_reg, base_reg, count_reg)); // point the destination at the embedded record
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("add {}, {}", base_reg, count_reg)); // point the destination at the embedded record
            }
        }
    }
    let words = size / 8;
    if words > 0 {
        // -- copy the record one 8-byte word at a time --
        abi::emit_load_int_immediate(ctx.emitter, count_reg, words as i64);
        ctx.emitter.label(&loop_label);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("ldr {}, [{}], #8", data_reg, source_reg)); // load the next source record word and advance the source
                ctx.emitter.instruction(&format!("str {}, [{}], #8", data_reg, base_reg)); // store it into the destination record and advance the destination
                ctx.emitter.instruction(&format!("subs {}, {}, #1", count_reg, count_reg)); // count down the record words still to copy
                ctx.emitter.instruction(&format!("b.ne {}", loop_label));       // keep copying until the whole record is written
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{}]", data_reg, source_reg)); // load the next source record word
                ctx.emitter.instruction(&format!("mov QWORD PTR [{}], {}", base_reg, data_reg)); // store it into the destination record
                ctx.emitter.instruction(&format!("add {}, 8", source_reg));     // advance the source to the next record word
                ctx.emitter.instruction(&format!("add {}, 8", base_reg));       // advance the destination to the next record word
                ctx.emitter.instruction(&format!("sub {}, 1", count_reg));      // count down the record words still to copy
                ctx.emitter.instruction(&format!("jne {}", loop_label));        // keep copying until the whole record is written
            }
        }
    }
    // -- copy the tail left by width-declared fields, widest piece first --
    let arch = ctx.emitter.target.arch;
    let tail = size % 8;
    for piece in [4usize, 2, 1] {
        if tail & piece == 0 {
            continue;
        }
        let piece_reg = super::scalar_widths::sub_register(arch, data_reg, piece);
        match arch {
            Arch::AArch64 => {
                let (load, store) = match piece {
                    4 => ("ldr", "str"),
                    2 => ("ldrh", "strh"),
                    _ => ("ldrb", "strb"),
                };
                ctx.emitter.instruction(&format!("{} {}, [{}], #{}", load, piece_reg, source_reg, piece)); // load the next tail piece and advance the source
                ctx.emitter.instruction(&format!("{} {}, [{}], #{}", store, piece_reg, base_reg, piece)); // store it into the destination record and advance the destination
            }
            Arch::X86_64 => {
                let ptr = match piece {
                    4 => "DWORD PTR",
                    2 => "WORD PTR",
                    _ => "BYTE PTR",
                };
                ctx.emitter.instruction(&format!("mov {}, {} [{}]", piece_reg, ptr, source_reg)); // load the next tail piece of the source record
                ctx.emitter.instruction(&format!("mov {} [{}], {}", ptr, base_reg, piece_reg)); // store it into the destination record
                ctx.emitter.instruction(&format!("add {}, {}", source_reg, piece)); // advance the source past the tail piece
                ctx.emitter.instruction(&format!("add {}, {}", base_reg, piece)); // advance the destination past the tail piece
            }
        }
    }
    Ok(())
}
