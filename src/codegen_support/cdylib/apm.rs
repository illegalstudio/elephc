//! Purpose:
//! Library exports as transactions: under `--with-apm`, each call a host
//! makes into an exported PHP function is counted as `CALL <name>`, failed
//! when the call reports a non-zero status.
//!
//! Called from:
//! - `super::emit_cdylib_exports()`, for every export of a library built
//!   with transaction monitoring.
//!
//! Key details:
//! - The export's symbol becomes a trampoline in front of its ordinary
//!   recoverable wrapper, renamed `INNER_PREFIX` + its C name, so the
//!   wrapper — every exit of it, scalar or string, on both architectures —
//!   is left as it is.
//! - The trampoline saves every argument register, opens the transaction,
//!   copies the caller's stack arguments down to where the wrapper expects
//!   them, restores the registers and calls the wrapper. It then keeps the
//!   return registers while it closes the transaction with the boundary's
//!   status, and returns them.
//! - A call the host makes from inside another (a callback) opens nothing
//!   of its own: the probe returns a zero token, which closes nothing.

use super::*;
use crate::codegen_support::data_section::DataSection;

/// What the wrapper behind a trampoline is renamed to, before its C name.
pub(crate) const INNER_PREFIX: &str = "__elephc_apm_inner_";

/// The name an export's calls are counted under.
pub(crate) fn transaction_name(c_name: &str) -> String {
    format!("CALL {c_name}")
}

/// Emits the trampoline at `exported` (the export's mangled public symbol):
/// a transaction around a call to `inner`, the wrapper's mangled symbol.
/// `stack_words` is how many 8-byte arguments the C caller passed on the stack.
pub(super) fn emit_trampoline(
    emitter: &mut Emitter,
    data: &mut DataSection,
    exported: &str,
    inner: &str,
    c_name: &str,
    stack_words: usize,
) {
    let mut name = transaction_name(c_name).into_bytes();
    name.push(0);
    let (name_label, _) = data.add_string(&name);
    let begin = emitter.target.extern_symbol("elephc_probe_txn_begin");
    let end = emitter.target.extern_symbol("elephc_probe_txn_end");
    emitter.blank();
    emitter.comment(&format!("--with-apm: calls of {c_name} are transactions"));
    emitter.label_global(exported);
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter, &name_label, &begin, &end, inner, stack_words),
        Arch::X86_64 => emit_x86_64(emitter, &name_label, &begin, &end, inner, stack_words),
    }
}

/// Bytes below the frame record: 8 integer and 8 float argument registers,
/// the token, and the two integer and one float return registers.
const SAVE_BYTES: usize = 8 * 8 + 8 * 8 + 8 + 3 * 8;

/// The frame below the frame record: the saves, and the outgoing stack
/// arguments at its bottom, 16-byte aligned.
fn frame_bytes(stack_words: usize) -> usize {
    (SAVE_BYTES + stack_words * 8 + 15) & !15
}

/// The AArch64 trampoline (AAPCS64 and Apple arm64).
fn emit_aarch64(emitter: &mut Emitter, name: &str, begin: &str, end: &str, inner: &str, stack_words: usize) {
    let frame = frame_bytes(stack_words);
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // keep the host's frame record
    emitter.instruction("mov x29, sp");                                         // anchor this frame for the saves and the caller's stack arguments
    emitter.instruction(&format!("sub sp, sp, #{frame}"));                      // reserve the saves and the outgoing stack arguments
    for pair in 0..4 {
        emitter.instruction(&format!(                                           // save two integer arguments across the transaction call
            "stp x{}, x{}, [x29, #-{}]", 2 * pair, 2 * pair + 1, 16 * (pair + 1)
        ));
        emitter.instruction(&format!(                                           // save two float arguments across the transaction call
            "stp d{}, d{}, [x29, #-{}]", 2 * pair, 2 * pair + 1, 64 + 16 * (pair + 1)
        ));
    }
    abi::emit_symbol_address(emitter, "x0", name);
    emitter.instruction(&format!("bl {begin}"));                                // open the transaction (token 0 when nothing is counted)
    emitter.instruction("stur x0, [x29, #-136]");                               // keep the token for the close
    for word in 0..stack_words {
        emitter.instruction(&format!("ldr x9, [x29, #{}]", 16 + 8 * word));     // read one of the host's stack arguments
        emitter.instruction(&format!("str x9, [sp, #{}]", 8 * word));           // hand it on where the wrapper reads its caller's stack
    }
    for pair in 0..4 {
        emitter.instruction(&format!(                                           // restore two integer arguments for the wrapper
            "ldp x{}, x{}, [x29, #-{}]", 2 * pair, 2 * pair + 1, 16 * (pair + 1)
        ));
        emitter.instruction(&format!(                                           // restore two float arguments for the wrapper
            "ldp d{}, d{}, [x29, #-{}]", 2 * pair, 2 * pair + 1, 64 + 16 * (pair + 1)
        ));
    }
    emitter.instruction(&format!("bl {inner}"));                                // run the export's recoverable wrapper
    emitter.instruction("stp x0, x1, [x29, #-152]");                            // keep the integer return registers across the close
    emitter.instruction("stur d0, [x29, #-160]");                               // keep the float return register across the close
    emitter.instruction("ldur x0, [x29, #-136]");                               // pass the token
    abi::emit_load_symbol_to_reg(emitter, "x1", BOUNDARY_STATUS, 0);
    emitter.instruction("cmp x1, #0");                                          // a non-zero boundary status is a failed call
    emitter.instruction("cset w1, ne");                                         // pass whether the call failed
    emitter.instruction(&format!("bl {end}"));                                  // count the call
    emitter.instruction("ldp x0, x1, [x29, #-152]");                            // restore the integer return registers
    emitter.instruction("ldur d0, [x29, #-160]");                               // restore the float return register
    emitter.instruction("mov sp, x29");                                         // drop the saves and the outgoing arguments
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the host's frame record
    emitter.instruction("ret");                                                 // return the wrapper's result to the host
}

/// The x86_64 System V trampoline.
fn emit_x86_64(emitter: &mut Emitter, name: &str, begin: &str, end: &str, inner: &str, stack_words: usize) {
    let frame = frame_bytes(stack_words);
    const INTEGER: [&str; 6] = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
    emitter.instruction("push rbp");                                            // keep the host's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor this frame for the saves and the caller's stack arguments
    emitter.instruction(&format!("sub rsp, {frame}"));                          // reserve the saves and the outgoing stack arguments
    for (index, reg) in INTEGER.iter().enumerate() {
        emitter.instruction(&format!(                                           // save an integer argument across the transaction call
            "mov QWORD PTR [rbp - {}], {reg}", 8 * (index + 1)
        ));
    }
    for index in 0..8 {
        emitter.instruction(&format!(                                           // save a float argument across the transaction call
            "movsd QWORD PTR [rbp - {}], xmm{index}", 64 + 8 * (index + 1)
        ));
    }
    abi::emit_symbol_address(emitter, "rdi", name);
    emitter.instruction(&format!("call {begin}"));                              // open the transaction (token 0 when nothing is counted)
    emitter.instruction("mov QWORD PTR [rbp - 136], rax");                      // keep the token for the close
    for word in 0..stack_words {
        emitter.instruction(&format!(                                           // read one of the host's stack arguments
            "mov r10, QWORD PTR [rbp + {}]", 16 + 8 * word
        ));
        emitter.instruction(&format!(                                           // hand it on where the wrapper reads its caller's stack
            "mov QWORD PTR [rsp + {}], r10", 8 * word
        ));
    }
    for (index, reg) in INTEGER.iter().enumerate() {
        emitter.instruction(&format!(                                           // restore an integer argument for the wrapper
            "mov {reg}, QWORD PTR [rbp - {}]", 8 * (index + 1)
        ));
    }
    for index in 0..8 {
        emitter.instruction(&format!(                                           // restore a float argument for the wrapper
            "movsd xmm{index}, QWORD PTR [rbp - {}]", 64 + 8 * (index + 1)
        ));
    }
    emitter.instruction(&format!("call {inner}"));                              // run the export's recoverable wrapper
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // keep the integer return registers across the close
    emitter.instruction("mov QWORD PTR [rbp - 152], rdx");                      // keep the second integer return register
    emitter.instruction("movsd QWORD PTR [rbp - 160], xmm0");                   // keep the float return register across the close
    emitter.instruction("mov rdi, QWORD PTR [rbp - 136]");                      // pass the token
    abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_STATUS, 0);
    emitter.instruction("xor esi, esi");                                        // assume the call succeeded
    emitter.instruction("test r10, r10");                                       // a non-zero boundary status is a failed call
    emitter.instruction("setne sil");                                           // pass whether the call failed
    emitter.instruction(&format!("call {end}"));                                // count the call
    emitter.instruction("mov rax, QWORD PTR [rbp - 144]");                      // restore the integer return register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 152]");                      // restore the second integer return register
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 160]");                   // restore the float return register
    emitter.instruction("leave");                                               // drop the frame and restore the host's frame pointer
    emitter.instruction("ret");                                                 // return the wrapper's result to the host
}

/// How many 8-byte words of an export's C arguments arrive on the stack.
pub(super) fn stack_words(emitter: &Emitter, export: &crate::exports::ExportedFunction) -> usize {
    let mut types = boundary::flattened_c_param_types(export);
    // A string-returning export takes its output pointer and length after
    // the PHP arguments (`owned_string::emit_save_public_arguments`); missing
    // them left a host's stack-passed outputs behind (a SIGBUS in the e2e).
    if crate::exports::is_string_return_signature(&export.source_sig) {
        types.extend([crate::types::PhpType::Int, crate::types::PhpType::Int]);
    }
    abi::build_outgoing_arg_assignments_for_target(emitter.target, &types, 0)
        .iter()
        .filter(|assignment| !assignment.in_register())
        .count()
}
