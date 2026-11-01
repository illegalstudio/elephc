//! Purpose:
//! Out-of-line publication of a PHP frame's activation record.
//!
//! Called from:
//! - `crate::codegen::frame` (the function prologue) through `__rt_php_frame_publish[_bt]`.
//!
//! Key details:
//! - Every PHP frame that can unwind or be backtraced links a 48-byte record into the
//!   `_exc_call_frame_top` chain: `[0]` previous, `[8]` cleanup callback, `[16]` frame pointer,
//!   `[24]` frame reader, `[32]` caller line, `[40]` caller-file descriptor, at ascending addresses
//!   from the record's address. The inline sequence
//!   was twenty-two instructions per function; the two helpers take the record's address and the
//!   cleanup callback in two registers and do the same stores once.
//! - AArch64: `x11` = record address, `x10` = cleanup callback (0 for none). x29 is the new
//!   frame's pointer, exactly as it was inline. Clobbers x9, x12, x13 only.
//! - x86_64: `r11` = record address, `r10` = cleanup callback. rbp is the new frame's pointer.
//!   Clobbers rax and rcx only.
//! - `_bt` additionally copies the caller's published `_php_site` record `{file, line}`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_php_frame_publish` and `__rt_php_frame_publish_bt`.
pub fn emit_php_frame_publish(emitter: &mut Emitter) {
    for backtrace in [false, true] {
        let name = if backtrace {
            "__rt_php_frame_publish_bt"
        } else {
            "__rt_php_frame_publish"
        };
        emitter.blank();
        emitter.label_global(name);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_symbol_to_reg(emitter, "x12", "_exc_call_frame_top", 0);
                emitter.instruction("str x12, [x11]");                          // [0] the previous activation
                emitter.instruction("stp x10, x29, [x11, #8]");                  // [8] cleanup callback, [16] frame pointer
                emitter.instruction("str xzr, [x11, #24]");                     // [24] frame reader, none yet
                if backtrace {
                    abi::emit_load_symbol_to_reg(emitter, "x12", "_php_site", 0);
                    emitter.instruction("ldp x12, x13, [x12]");                 // the caller's published {file descriptor, line}
                    emitter.instruction("stp x13, x12, [x11, #32]");             // [32] caller line, [40] caller file
                } else {
                    emitter.instruction("stp xzr, xzr, [x11, #32]");              // no caller site recorded for a hidden frame
                }
                abi::emit_store_reg_to_symbol(emitter, "x11", "_exc_call_frame_top", 0);
                emitter.instruction("ret");                                     // back to the prologue
            }
            Arch::X86_64 => {
                abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_call_frame_top", 0);
                emitter.instruction("mov QWORD PTR [r11], rax");                // [0] the previous activation
                emitter.instruction("mov QWORD PTR [r11 + 8], r10");            // [8] cleanup callback
                emitter.instruction("mov QWORD PTR [r11 + 16], rbp");           // [16] frame pointer
                emitter.instruction("mov QWORD PTR [r11 + 24], 0");             // [24] frame reader, none yet
                if backtrace {
                    abi::emit_load_symbol_to_reg(emitter, "rax", "_php_site", 0);
                    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");        // the caller's published line
                    emitter.instruction("mov QWORD PTR [r11 + 32], rcx");       // [32] caller line
                    emitter.instruction("mov rcx, QWORD PTR [rax]");            // the caller's published file descriptor
                    emitter.instruction("mov QWORD PTR [r11 + 40], rcx");       // [40] caller file
                } else {
                    emitter.instruction("mov QWORD PTR [r11 + 32], 0");         // no caller line for a hidden frame
                    emitter.instruction("mov QWORD PTR [r11 + 40], 0");         // no caller file for a hidden frame
                }
                abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_call_frame_top", 0);
                emitter.instruction("ret");                                     // back to the prologue
            }
        }
    }
}
