//! Purpose:
//! Emits width-aware loads and stores for scalars that live in memory narrower than a machine
//! word: the C-width fields of `extern class` structs, the width-declared fields of packed
//! classes, and the elements of `buffer<i8>`..`buffer<f32>`. The 8-byte shapes are covered too,
//! so one table serves every field of a struct.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::extern_fields` (extern struct fields).
//! - `crate::codegen::lower_inst::objects::property_loads` and `property_store_values` (packed
//!   fields and the tail of a nested record copy).
//! - `crate::codegen::lower_inst::buffers` (width-declared buffer elements).
//!
//! Key details:
//! - One `ExternAbiType` shape selects the access: integer widths sign or zero extend into the
//!   64-bit PHP int on a load and store only their low bytes, `f32` widens from and rounds to an
//!   IEEE 754 single through the float result register, `bool` is one byte read as 0 or 1, and a
//!   `char*` loads as a borrowed (pointer, length) view in the string result registers.
//! - Loads land in the canonical result registers; stores take an integer payload from a
//!   caller-chosen register and a float payload from the float result register.
//! - AArch64 scaled immediates reach 4095 elements of the access size; an offset past that is
//!   first added into a scratch register that is neither the base nor the payload register.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::types::ExternAbiType;

use super::super::context::FunctionContext;

/// Returns the byte size of a C shape stored in memory.
pub(super) fn abi_shape_size(abi_ty: ExternAbiType) -> usize {
    match abi_ty {
        ExternAbiType::Int { bits, .. } => usize::from(bits / 8),
        ExternAbiType::Float32 => 4,
        ExternAbiType::Bool => 1,
        ExternAbiType::Float64
        | ExternAbiType::CStr
        | ExternAbiType::NullableCStr
        | ExternAbiType::Ptr
        | ExternAbiType::Callable
        | ExternAbiType::Void => 8,
    }
}

/// Returns the name of an integer register at an access of `size` bytes: the `w` form of an
/// AArch64 register for every narrow access, and the 32-, 16- or 8-bit alias of an x86_64
/// register. An 8-byte access uses the full register.
pub(super) fn sub_register(arch: Arch, reg: &str, size: usize) -> String {
    if size == 8 {
        return reg.to_string();
    }
    match arch {
        Arch::AArch64 => format!("w{}", reg.trim_start_matches('x')),
        Arch::X86_64 => x86_sub_register(reg, size),
    }
}

/// Returns the 32-, 16- or 8-bit alias of an x86_64 general-purpose register.
fn x86_sub_register(reg: &str, size: usize) -> String {
    if let Some(number) = reg.strip_prefix('r').and_then(|rest| rest.parse::<u8>().ok()) {
        let suffix = match size {
            4 => "d",
            2 => "w",
            _ => "b",
        };
        return format!("r{}{}", number, suffix);
    }
    let (dword, word, byte) = match reg {
        "rax" => ("eax", "ax", "al"),
        "rbx" => ("ebx", "bx", "bl"),
        "rcx" => ("ecx", "cx", "cl"),
        "rdx" => ("edx", "dx", "dl"),
        "rsi" => ("esi", "si", "sil"),
        "rdi" => ("edi", "di", "dil"),
        "rbp" => ("ebp", "bp", "bpl"),
        "rsp" => ("esp", "sp", "spl"),
        other => return other.to_string(),
    };
    match size {
        4 => dword.to_string(),
        2 => word.to_string(),
        _ => byte.to_string(),
    }
}

/// Returns the memory operand of `base_reg + offset` for an access of `size` bytes.
///
/// x86_64 encodes any 32-bit displacement. AArch64 encodes an unsigned offset of up to 4095
/// elements of the access size; an offset past that (only in records larger than 4 KiB) is
/// first added into a scratch register other than the base and the payload register.
fn width_operand(
    ctx: &mut FunctionContext<'_>,
    base_reg: &str,
    offset: usize,
    size: usize,
    payload_reg: &str,
) -> String {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                format!("[{}]", base_reg)
            } else if offset % size == 0 && offset / size <= 4095 {
                format!("[{}, #{}]", base_reg, offset)
            } else {
                // Three candidates minus at most two excluded registers always leaves one.
                let scratch = ["x9", "x10", "x11"]
                    .into_iter()
                    .find(|candidate| *candidate != base_reg && *candidate != payload_reg)
                    .unwrap_or("x11");
                // -- add a field offset beyond the scaled-immediate range --
                abi::emit_load_int_immediate(ctx.emitter, scratch, offset as i64);
                ctx.emitter.instruction(&format!(
                    "add {}, {}, {}",
                    scratch, base_reg, scratch
                ));                                                             // point the scratch register at a field beyond the scaled-immediate range
                format!("[{}]", scratch)
            }
        }
        Arch::X86_64 => {
            if offset == 0 {
                format!("[{}]", base_reg)
            } else {
                format!("[{} + {}]", base_reg, offset)
            }
        }
    }
}

/// Loads the C shape at `base_reg + offset` into the canonical result register(s).
///
/// Integer shapes land in the integer result register widened to the PHP int, floats in the
/// float result register as a double, and a `char*` as a borrowed (pointer, length) string view.
pub(super) fn emit_width_load(
    ctx: &mut FunctionContext<'_>,
    abi_ty: ExternAbiType,
    base_reg: &str,
    offset: usize,
) -> Result<()> {
    let operand = width_operand(ctx, base_reg, offset, abi_shape_size(abi_ty), "");
    let arch = ctx.emitter.target.arch;
    // -- load the C shape into the PHP result register --
    match (arch, abi_ty) {
        (Arch::AArch64, ExternAbiType::Int { bits: 64, .. } | ExternAbiType::Ptr) => {
            ctx.emitter.instruction(&format!("ldr x0, {}", operand));           // load the 64-bit C value as the PHP value
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 32, signed: true }) => {
            ctx.emitter.instruction(&format!("ldrsw x0, {}", operand));         // load the C int32 and sign-extend it into the PHP int
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 32, signed: false }) => {
            ctx.emitter.instruction(&format!("ldr w0, {}", operand));           // load the C uint32 and zero-extend it into the PHP int
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 16, signed: true }) => {
            ctx.emitter.instruction(&format!("ldrsh x0, {}", operand));         // load the C int16 and sign-extend it into the PHP int
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 16, signed: false }) => {
            ctx.emitter.instruction(&format!("ldrh w0, {}", operand));          // load the C uint16 and zero-extend it into the PHP int
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 8, signed: true }) => {
            ctx.emitter.instruction(&format!("ldrsb x0, {}", operand));         // load the C int8 and sign-extend it into the PHP int
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 8, signed: false }) => {
            ctx.emitter.instruction(&format!("ldrb w0, {}", operand));          // load the C uint8 and zero-extend it into the PHP int
        }
        (Arch::AArch64, ExternAbiType::Bool) => {
            // -- read the C bool byte as a PHP bool --
            ctx.emitter.instruction(&format!("ldrb w0, {}", operand));          // load the one-byte C bool
            ctx.emitter.instruction("cmp w0, #0");                              // test whether the C bool byte is set
            ctx.emitter.instruction("cset x0, ne");                             // read any nonzero C bool byte as PHP true
        }
        (Arch::AArch64, ExternAbiType::Float64) => {
            ctx.emitter.instruction(&format!("ldr d0, {}", operand));           // load the C double as the PHP float
        }
        (Arch::AArch64, ExternAbiType::Float32) => {
            // -- widen the C float into the PHP double --
            ctx.emitter.instruction(&format!("ldr s0, {}", operand));           // load the C float as an IEEE 754 single
            abi::emit_float_result_single_to_double(ctx.emitter);
        }
        (Arch::AArch64, ExternAbiType::CStr) => {
            let scan = ctx.next_label("extern_cstr_scan");
            let done = ctx.next_label("extern_cstr_done");
            // -- borrow the C string and measure it up to its terminator --
            ctx.emitter.instruction(&format!("ldr x1, {}", operand));           // load the C char* as the borrowed string pointer
            ctx.emitter.instruction("mov x2, #0");                              // start the borrowed view at length zero
            ctx.emitter.instruction(&format!("cbz x1, {}", done));              // read a NULL char* as the empty string
            ctx.emitter.label(&scan);
            ctx.emitter.instruction("ldrb w9, [x1, x2]");                       // load the next C string byte
            ctx.emitter.instruction(&format!("cbz w9, {}", done));              // stop at the C string terminator
            ctx.emitter.instruction("add x2, x2, #1");                          // count one more byte of the borrowed view
            ctx.emitter.instruction(&format!("b {}", scan));                    // keep scanning for the terminator
            ctx.emitter.label(&done);
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 64, .. } | ExternAbiType::Ptr) => {
            ctx.emitter.instruction(&format!(
                "mov rax, QWORD PTR {}",
                operand
            ));                                                                 // load the 64-bit C value as the PHP value
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 32, signed: true }) => {
            ctx.emitter.instruction(&format!(
                "movsxd rax, DWORD PTR {}",
                operand
            ));                                                                 // load the C int32 and sign-extend it into the PHP int
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 32, signed: false }) => {
            ctx.emitter.instruction(&format!(
                "mov eax, DWORD PTR {}",
                operand
            ));                                                                 // load the C uint32 and zero-extend it into the PHP int
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 16, signed: true }) => {
            ctx.emitter.instruction(&format!(
                "movsx rax, WORD PTR {}",
                operand
            ));                                                                 // load the C int16 and sign-extend it into the PHP int
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 16, signed: false }) => {
            ctx.emitter.instruction(&format!(
                "movzx eax, WORD PTR {}",
                operand
            ));                                                                 // load the C uint16 and zero-extend it into the PHP int
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 8, signed: true }) => {
            ctx.emitter.instruction(&format!(
                "movsx rax, BYTE PTR {}",
                operand
            ));                                                                 // load the C int8 and sign-extend it into the PHP int
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 8, signed: false }) => {
            ctx.emitter.instruction(&format!(
                "movzx eax, BYTE PTR {}",
                operand
            ));                                                                 // load the C uint8 and zero-extend it into the PHP int
        }
        (Arch::X86_64, ExternAbiType::Bool) => {
            // -- read the C bool byte as a PHP bool --
            ctx.emitter.instruction(&format!(
                "movzx eax, BYTE PTR {}",
                operand
            ));                                                                 // load the one-byte C bool
            ctx.emitter.instruction("test eax, eax");                           // test whether the C bool byte is set
            ctx.emitter.instruction("setne al");                                // read any nonzero C bool byte as PHP true
            ctx.emitter.instruction("movzx eax, al");                           // widen the 0/1 flag into the PHP bool payload
        }
        (Arch::X86_64, ExternAbiType::Float64) => {
            ctx.emitter.instruction(&format!(
                "movsd xmm0, QWORD PTR {}",
                operand
            ));                                                                 // load the C double as the PHP float
        }
        (Arch::X86_64, ExternAbiType::Float32) => {
            // -- widen the C float into the PHP double --
            ctx.emitter.instruction(&format!(
                "movss xmm0, DWORD PTR {}",
                operand
            ));                                                                 // load the C float as an IEEE 754 single
            abi::emit_float_result_single_to_double(ctx.emitter);
        }
        (Arch::X86_64, ExternAbiType::CStr) => {
            let scan = ctx.next_label("extern_cstr_scan");
            let done = ctx.next_label("extern_cstr_done");
            // -- borrow the C string and measure it up to its terminator --
            ctx.emitter.instruction(&format!(
                "mov rax, QWORD PTR {}",
                operand
            ));                                                                 // load the C char* as the borrowed string pointer
            ctx.emitter.instruction("xor edx, edx");                            // start the borrowed view at length zero
            ctx.emitter.instruction("test rax, rax");                           // check for a NULL char*
            ctx.emitter.instruction(&format!("jz {}", done));                   // read a NULL char* as the empty string
            ctx.emitter.label(&scan);
            ctx.emitter.instruction("cmp BYTE PTR [rax + rdx], 0");             // test the next C string byte for the terminator
            ctx.emitter.instruction(&format!("je {}", done));                   // stop at the C string terminator
            ctx.emitter.instruction("add rdx, 1");                              // count one more byte of the borrowed view
            ctx.emitter.instruction(&format!("jmp {}", scan));                  // keep scanning for the terminator
            ctx.emitter.label(&done);
        }
        (_, other) => {
            return Err(CodegenIrError::unsupported(format!(
                "load of C shape {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Stores a payload into the C shape at `base_reg + offset`.
///
/// An integer, pointer or `char*` payload is read from `payload_reg` and truncated to the
/// shape's width; a float payload is the double in the float result register, rounded to a
/// single for `f32`. `payload_reg` must differ from `base_reg`.
pub(super) fn emit_width_store(
    ctx: &mut FunctionContext<'_>,
    abi_ty: ExternAbiType,
    base_reg: &str,
    offset: usize,
    payload_reg: &str,
) -> Result<()> {
    let size = abi_shape_size(abi_ty);
    let operand = width_operand(ctx, base_reg, offset, size, payload_reg);
    let arch = ctx.emitter.target.arch;
    let narrow = sub_register(arch, payload_reg, size);
    // -- store the payload into the C shape --
    match (arch, abi_ty) {
        (Arch::AArch64, ExternAbiType::Int { bits: 64, .. } | ExternAbiType::Ptr | ExternAbiType::CStr) => {
            ctx.emitter.instruction(&format!("str {}, {}", narrow, operand));   // store the 64-bit payload into the C value
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 32, .. }) => {
            ctx.emitter.instruction(&format!("str {}, {}", narrow, operand));   // store the low 32 bits into the C 32-bit value
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 16, .. }) => {
            ctx.emitter.instruction(&format!("strh {}, {}", narrow, operand));  // store the low 16 bits into the C 16-bit value
        }
        (Arch::AArch64, ExternAbiType::Int { bits: 8, .. } | ExternAbiType::Bool) => {
            ctx.emitter.instruction(&format!("strb {}, {}", narrow, operand));  // store the low byte into the C 8-bit or bool value
        }
        (Arch::AArch64, ExternAbiType::Float64) => {
            ctx.emitter.instruction(&format!("str d0, {}", operand));           // store the PHP float into the C double
        }
        (Arch::AArch64, ExternAbiType::Float32) => {
            // -- round the PHP double into the C float --
            abi::emit_float_result_double_to_single(ctx.emitter);
            ctx.emitter.instruction(&format!("str s0, {}", operand));           // store the rounded IEEE 754 single into the C float
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 64, .. } | ExternAbiType::Ptr | ExternAbiType::CStr) => {
            ctx.emitter.instruction(&format!(
                "mov QWORD PTR {}, {}",
                operand, narrow
            ));                                                                 // store the 64-bit payload into the C value
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 32, .. }) => {
            ctx.emitter.instruction(&format!(
                "mov DWORD PTR {}, {}",
                operand, narrow
            ));                                                                 // store the low 32 bits into the C 32-bit value
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 16, .. }) => {
            ctx.emitter.instruction(&format!(
                "mov WORD PTR {}, {}",
                operand, narrow
            ));                                                                 // store the low 16 bits into the C 16-bit value
        }
        (Arch::X86_64, ExternAbiType::Int { bits: 8, .. } | ExternAbiType::Bool) => {
            ctx.emitter.instruction(&format!(
                "mov BYTE PTR {}, {}",
                operand, narrow
            ));                                                                 // store the low byte into the C 8-bit or bool value
        }
        (Arch::X86_64, ExternAbiType::Float64) => {
            ctx.emitter.instruction(&format!(
                "movsd QWORD PTR {}, xmm0",
                operand
            ));                                                                 // store the PHP float into the C double
        }
        (Arch::X86_64, ExternAbiType::Float32) => {
            // -- round the PHP double into the C float --
            abi::emit_float_result_double_to_single(ctx.emitter);
            ctx.emitter.instruction(&format!(
                "movss DWORD PTR {}, xmm0",
                operand
            ));                                                                 // store the rounded IEEE 754 single into the C float
        }
        (_, other) => {
            return Err(CodegenIrError::unsupported(format!(
                "store of C shape {:?}",
                other
            )))
        }
    }
    Ok(())
}
