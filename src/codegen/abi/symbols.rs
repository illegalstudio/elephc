//! Purpose:
//! Emits target-aware loads, stores, and addresses for assembly symbols and external globals.
//! Bridges local slot values, result registers, and static storage symbols.
//!
//! Called from:
//! - `crate::codegen::stmt::storage`, `crate::codegen::functions`, and global emitters
//!
//! Key details:
//! - Symbol relocations differ by platform and refcounted stores must preserve ownership cleanup.

use crate::codegen::{emit::Emitter, platform::Arch};
use crate::types::PhpType;

use super::calls::emit_call_label;
use super::frame::{
    emit_load_from_address, emit_store_to_address, load_at_offset_scratch, store_at_offset_scratch,
};
use super::registers::{
    float_result_reg, int_result_reg, is_float_register, secondary_scratch_reg,
    string_result_regs, symbol_scratch_reg, tertiary_scratch_reg,
};
use super::values::{emit_decref_if_refcounted, emit_load_int_immediate};

const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// Stores a local variable from its frame slot into a static/global symbol.
/// Loads the value from `offset` relative to the frame pointer, then writes it
/// to `symbol` at `byte_offset`.  Handles Float (single register), Str (pointer
/// + length pair), Void (null sentinel), and scalar/pointer types differently.
pub fn emit_store_local_slot_to_symbol(
    emitter: &mut Emitter,
    symbol: &str,
    ty: &PhpType,
    offset: usize,
) {
    let symbol_reg = symbol_scratch_reg(emitter);
    let local_reg = secondary_scratch_reg(emitter);
    let local_hi_reg = tertiary_scratch_reg(emitter);
    match ty.codegen_repr() {
        PhpType::Float => {
            load_at_offset_scratch(emitter, float_result_reg(emitter), offset, local_reg); // load the local float value from its frame slot
            emit_store_reg_to_symbol(emitter, float_result_reg(emitter), symbol, 0);       // store the local float value into symbol storage
        }
        PhpType::Str => {
            load_at_offset_scratch(emitter, local_reg, offset, symbol_reg);            // load the local string pointer from its frame slot
            load_at_offset_scratch(emitter, local_hi_reg, offset - 8, symbol_reg);     // load the local string length from its paired frame slot
            emit_store_reg_to_symbol(emitter, local_reg, symbol, 0);                    // store the local string pointer into symbol storage
            emit_store_reg_to_symbol(emitter, local_hi_reg, symbol, 8);                 // store the local string length into symbol storage
        }
        PhpType::Void => {
            load_at_offset_scratch(emitter, local_reg, offset, symbol_reg);             // load the local null sentinel from its frame slot
            emit_store_reg_to_symbol(emitter, local_reg, symbol, 0);                    // store the local null sentinel into symbol storage
        }
        _ => {
            load_at_offset_scratch(emitter, local_reg, offset, symbol_reg);             // load the local scalar or pointer-like value from its frame slot
            emit_store_reg_to_symbol(emitter, local_reg, symbol, 0);                    // store the local scalar or pointer-like value into symbol storage
        }
    }
}

/// Loads a value from a static/global symbol into a local frame slot.
/// Reads from `symbol` at `byte_offset` and writes it to `offset` relative to
/// the frame pointer.  Dispatches on `ty` to handle Float, Str (pointer +
/// length), Void (null sentinel), and scalar/pointer types.
pub fn emit_load_symbol_to_local_slot(
    emitter: &mut Emitter,
    symbol: &str,
    ty: &PhpType,
    offset: usize,
) {
    let local_reg = secondary_scratch_reg(emitter);
    let local_hi_reg = tertiary_scratch_reg(emitter);
    match ty.codegen_repr() {
        PhpType::Float => {
            emit_load_symbol_to_reg(emitter, float_result_reg(emitter), symbol, 0);        // load the float value from symbol storage
            store_at_offset_scratch(emitter, float_result_reg(emitter), offset, local_reg); // write the loaded float value into the local frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_load_symbol_to_reg(emitter, ptr_reg, symbol, 0);                       // load the string pointer from symbol storage
            emit_load_symbol_to_reg(emitter, len_reg, symbol, 8);                       // load the string length from symbol storage
            store_at_offset_scratch(emitter, ptr_reg, offset, local_reg);               // write the loaded string pointer into the local frame slot
            store_at_offset_scratch(emitter, len_reg, offset - 8, local_hi_reg);        // write the loaded string length into the paired local frame slot
        }
        PhpType::Void => {
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);       // load the null sentinel from symbol storage
            store_at_offset_scratch(emitter, int_result_reg(emitter), offset, local_reg); // write the loaded null sentinel into the local frame slot
        }
        _ => {
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);       // load the scalar or pointer-like value from symbol storage
            store_at_offset_scratch(emitter, int_result_reg(emitter), offset, local_reg); // write the loaded scalar or pointer-like value into the local frame slot
        }
    }
}

/// Materializes the address of a local/internal symbol into `dest`.
/// Uses ADRP+ADD on AArch64 (page-relative) and LEA with RIP-relative
/// addressing on x86_64.  The symbol must be defined in the current module's
/// data section.
pub fn emit_symbol_address(emitter: &mut Emitter, dest: &str, symbol: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp(dest, &format!("{}", symbol));                                  // load the page of the requested symbol storage
            emitter.add_lo12(dest, dest, &format!("{}", symbol));                       // resolve the exact address of the requested symbol storage
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea {}, [rip + {}]", dest, symbol));  // materialize the symbol address through a RIP-relative LEA
        }
    }
}

/// Materializes the address of an external/global symbol into `dest`.
/// Resolves the symbol through the GOT on both targets: ADRP+GOT on AArch64
/// and MOVQ with GOTPCREL on x86_64.  Used for symbols defined outside the
/// current translation unit.
pub fn emit_extern_symbol_address(emitter: &mut Emitter, dest: &str, symbol: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp_got(dest, symbol);                                          // load the GOT page that points at the requested extern symbol
            emitter.ldr_got_lo12(dest, dest, symbol);                                // resolve the GOT entry into the actual extern symbol address
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR {}@GOTPCREL[rip]", dest, symbol)); // materialize the extern symbol address through the ELF GOTPCREL slot
        }
    }
}

/// Loads a value from an external symbol into `reg`.
/// First resolves the extern symbol address via the GOT, then performs a
/// load from `byte_offset` into `reg`.  Used for reading global variables
/// defined in external libraries or other compilation units.
pub fn emit_load_extern_symbol_to_reg(
    emitter: &mut Emitter,
    reg: &str,
    symbol: &str,
    byte_offset: usize,
) {
    let scratch = symbol_scratch_reg(emitter);
    emit_extern_symbol_address(emitter, scratch, symbol);
    emit_load_from_address(emitter, reg, scratch, byte_offset);
}

/// Stores the contents of `reg` into an external symbol at a byte offset.
/// First resolves the extern symbol address via the GOT, then performs a
/// store from `reg` to `byte_offset`.  Used for writing to global variables
/// defined in external libraries or other compilation units.
pub fn emit_store_reg_to_extern_symbol(
    emitter: &mut Emitter,
    reg: &str,
    symbol: &str,
    byte_offset: usize,
) {
    let scratch = symbol_scratch_reg(emitter);
    emit_extern_symbol_address(emitter, scratch, symbol);
    emit_store_to_address(emitter, reg, scratch, byte_offset);
}

/// Loads a value from a local/internal symbol into `reg`.
/// Uses a temporary scratch register (x9 on AArch64) to compute the symbol
/// address, then loads from `byte_offset`.  On x86_64 uses RIP-relative
/// addressing directly when offset is zero.  Dispatches on register type
/// for float vs. integer moves on x86_64.
pub fn emit_load_symbol_to_reg(
    emitter: &mut Emitter,
    reg: &str,
    symbol: &str,
    byte_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_symbol_address(emitter, "x9", symbol);
            if byte_offset == 0 {
                emitter.instruction(&format!("ldr {}, [x9]", reg));             // load the symbol payload directly from its base address
            } else {
                emitter.instruction(&format!("ldr {}, [x9, #{}]", reg, byte_offset)); // load the symbol payload from the requested byte offset
            }
        }
        Arch::X86_64 => {
            let scratch = symbol_scratch_reg(emitter);
            if byte_offset == 0 {
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd {}, QWORD PTR [rip + {}]", reg, symbol)); // load the floating-point symbol payload through RIP-relative addressing
                } else {
                    emitter.instruction(&format!("mov {}, QWORD PTR [rip + {}]", reg, symbol)); // load the integer or pointer symbol payload through RIP-relative addressing
                }
            } else {
                emit_symbol_address(emitter, scratch, symbol);
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd {}, QWORD PTR [{} + {}]", reg, scratch, byte_offset)); // load the floating-point symbol payload from a non-zero byte offset
                } else {
                    emitter.instruction(&format!("mov {}, QWORD PTR [{} + {}]", reg, scratch, byte_offset)); // load the integer or pointer symbol payload from a non-zero byte offset
                }
            }
        }
    }
}

/// Stores the contents of `reg` into a local/internal symbol at a byte offset.
/// Uses a temporary scratch register (x9 on AArch64) to compute the symbol
/// address, then stores at `byte_offset`.  On x86_64 uses RIP-relative
/// addressing directly when offset is zero.  Dispatches on register type
/// for float vs. integer moves on x86_64.
pub fn emit_store_reg_to_symbol(
    emitter: &mut Emitter,
    reg: &str,
    symbol: &str,
    byte_offset: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_symbol_address(emitter, "x9", symbol);
            if byte_offset == 0 {
                emitter.instruction(&format!("str {}, [x9]", reg));             // store the register payload directly into the symbol base slot
            } else {
                emitter.instruction(&format!("str {}, [x9, #{}]", reg, byte_offset)); // store the register payload into the requested symbol byte offset
            }
        }
        Arch::X86_64 => {
            let scratch = symbol_scratch_reg(emitter);
            if byte_offset == 0 {
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd QWORD PTR [rip + {}], {}", symbol, reg)); // store the floating-point payload directly into RIP-relative symbol storage
                } else {
                    emitter.instruction(&format!("mov QWORD PTR [rip + {}], {}", symbol, reg)); // store the integer or pointer payload directly into RIP-relative symbol storage
                }
            } else {
                emit_symbol_address(emitter, scratch, symbol);
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd QWORD PTR [{} + {}], {}", scratch, byte_offset, reg)); // store the floating-point payload into a non-zero symbol byte offset
                } else {
                    emitter.instruction(&format!("mov QWORD PTR [{} + {}], {}", scratch, byte_offset, reg)); // store the integer or pointer payload into a non-zero symbol byte offset
                }
            }
        }
    }
}

/// Stores the architectural zero register (xzr / zero) into a symbol slot.
/// On AArch64 this is a single STR using xzr; on x86_64 it emits a MOV
/// immediate zero.  Used to initialize symbol storage to null/zero without
/// a separate load-from-register step.
pub fn emit_store_zero_to_symbol(emitter: &mut Emitter, symbol: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_store_reg_to_symbol(emitter, "xzr", symbol, byte_offset);              // store architectural zero directly into symbol-backed storage
        }
        Arch::X86_64 => {
            let scratch = symbol_scratch_reg(emitter);
            if byte_offset == 0 {
                emitter.instruction(&format!("mov QWORD PTR [rip + {}], 0", symbol)); // zero the symbol base slot through RIP-relative addressing
            } else {
                emit_symbol_address(emitter, scratch, symbol);
                emitter.instruction(&format!("mov QWORD PTR [{} + {}], 0", scratch, byte_offset)); // zero the requested symbol byte offset through the computed address
            }
        }
    }
}

/// Loads a symbol's value and places it into the appropriate result registers.
/// Reads from `symbol` at offset 0 (and 8 for strings) and places the value
/// into the target's canonical result registers: float_result_reg for Float,
/// string_result_regs for Str, int_result_reg otherwise.  Used to return
/// a symbol's value as a call result.
pub fn emit_load_symbol_to_result(emitter: &mut Emitter, symbol: &str, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            emit_load_symbol_to_reg(emitter, float_result_reg(emitter), symbol, 0);     // load the float payload from symbol storage into the result register
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_load_symbol_to_reg(emitter, ptr_reg, symbol, 0);                       // load the string pointer from symbol storage into the result register pair
            emit_load_symbol_to_reg(emitter, len_reg, symbol, 8);                       // load the string length from symbol storage into the result register pair
        }
        PhpType::Void => {
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);       // load the null sentinel from symbol storage into the result register
        }
        _ => {
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);       // load the scalar or pointer-like payload from symbol storage into the result register
        }
    }
}

/// Stores the current result registers into a static/global symbol.
/// If `release_previous` is true, first loads the old symbol value and
/// releases it: strings call `__rt_heap_free_safe`, refcounted types call
/// `emit_decref_if_refcounted`.  Incoming results are preserved on the stack
/// during the release call.  Handles Float, Str (pointer + length pair),
/// Void (null sentinel), and scalar/pointer types.
pub fn emit_store_result_to_symbol(
    emitter: &mut Emitter,
    symbol: &str,
    ty: &PhpType,
    release_previous: bool,
) {
    let ty = ty.codegen_repr();
    if release_previous {
        if matches!(ty, PhpType::Str) {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // preserve the incoming string result while releasing the previous symbol payload
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("push {}", ptr_reg));          // preserve the incoming string pointer result while releasing the previous symbol payload
                    emitter.instruction(&format!("push {}", len_reg));          // preserve the incoming string length result while releasing the previous symbol payload
                }
            }
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);
            emit_call_label(emitter, "__rt_heap_free_safe");                             // release the previous string allocation before overwriting the symbol slot
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldp x1, x2, [sp], #16");               // restore the incoming string result after the release helper call
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("pop {}", len_reg));           // restore the incoming string length result after the release helper call
                    emitter.instruction(&format!("pop {}", ptr_reg));           // restore the incoming string pointer result after the release helper call
                }
            }
        } else if ty.is_refcounted() {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // preserve the incoming heap pointer while decreffing the previous symbol payload
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("push {}", int_result_reg(emitter))); // preserve the incoming heap pointer while decreffing the previous symbol payload
                }
            }
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);
            emit_decref_if_refcounted(emitter, &ty);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x0, [sp], #16");                   // restore the incoming heap pointer after decreffing the previous payload
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("pop {}", int_result_reg(emitter))); // restore the incoming heap pointer after decreffing the previous payload
                }
            }
        }
    }

    match ty {
        PhpType::Float => {
            emit_store_reg_to_symbol(emitter, float_result_reg(emitter), symbol, 0);    // store the float result into symbol storage
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_store_reg_to_symbol(emitter, ptr_reg, symbol, 0);                      // store the string pointer result into symbol storage
            emit_store_reg_to_symbol(emitter, len_reg, symbol, 8);                      // store the string length result into symbol storage
        }
        PhpType::Void => {
            let null_reg = secondary_scratch_reg(emitter);
            emit_load_int_immediate(emitter, null_reg, NULL_SENTINEL);
            emit_store_reg_to_symbol(emitter, null_reg, symbol, 0);                     // store the null sentinel result into symbol storage
        }
        _ => {
            emit_store_reg_to_symbol(emitter, int_result_reg(emitter), symbol, 0);      // store the scalar or pointer-like result into symbol storage
        }
    }
}
