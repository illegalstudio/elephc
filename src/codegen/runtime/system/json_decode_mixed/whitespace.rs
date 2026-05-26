//! Purpose:
//! Emits the shared JSON whitespace skipper used by the checked Mixed decoder.
//! Centralizes the RFC 8259 whitespace byte set for recursive decode helpers.
//!
//! Called from:
//! - `crate::codegen::runtime::system::json_decode_mixed` decoder emitters.
//!
//! Key details:
//! - The helper advances a caller-owned cursor up to an exclusive byte limit.
//! - Callers decide whether reaching the limit means EOF, an empty container, or a local syntax error.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Generates the `__rt_json_skip_ws` runtime helper that skips JSON whitespace.
///
/// Reads bytes from a caller-owned slice starting at `cursor` up to an exclusive
/// `limit`, consuming RFC 8259 whitespace (space, tab, LF, CR) until a non-whitespace
/// byte or the limit is reached. The advanced cursor is returned in the same register.
///
/// Input/output ABI:
///   ARM64:  x1 = slice ptr, x2 = exclusive limit, x9 = cursor → x9 = first non-WS / limit
///   x86_64: rax = slice ptr, rdx = exclusive limit, rcx = cursor → rcx = first non-WS / limit
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_skip_ws ---");
    emitter.label_global("__rt_json_skip_ws");
    emitter.label("__rt_json_skip_ws_loop");
    emitter.instruction("cmp x9, x2");                                          // stop once the cursor reaches the caller's exclusive limit
    emitter.instruction("b.ge __rt_json_skip_ws_done");                         // return the limit unchanged when only whitespace remains
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the candidate JSON whitespace byte
    emitter.instruction("cmp w10, #32");                                        // space?
    emitter.instruction("b.eq __rt_json_skip_ws_step");                         // consume JSON space
    emitter.instruction("cmp w10, #9");                                         // tab?
    emitter.instruction("b.eq __rt_json_skip_ws_step");                         // consume JSON tab
    emitter.instruction("cmp w10, #10");                                        // LF?
    emitter.instruction("b.eq __rt_json_skip_ws_step");                         // consume JSON line feed
    emitter.instruction("cmp w10, #13");                                        // CR?
    emitter.instruction("b.ne __rt_json_skip_ws_done");                         // any other byte begins the next token
    emitter.label("__rt_json_skip_ws_step");
    emitter.instruction("add x9, x9, #1");                                      // advance over one JSON whitespace byte
    emitter.instruction("b __rt_json_skip_ws_loop");                            // continue until token or limit
    emitter.label("__rt_json_skip_ws_done");
    emitter.instruction("ret");                                                 // return with x9 holding the advanced cursor
}

/// Generates the x86_64 SysV ABI variant of the JSON whitespace skipper.
///
/// Identical behavior to the ARM64 variant: reads bytes from a caller-owned slice
/// starting at `cursor` up to an exclusive `limit`, consuming RFC 8259 whitespace
/// (space, tab, LF, CR) until a non-whitespace byte or the limit is reached.
///
/// Input/output (System V ABI):
///   rax = slice ptr, rdx = exclusive limit, rcx = cursor → rcx = first non-WS / limit
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_skip_ws ---");
    emitter.label_global("__rt_json_skip_ws");
    emitter.label("__rt_json_skip_ws_loop_x");
    emitter.instruction("cmp rcx, rdx");                                        // stop once the cursor reaches the caller's exclusive limit
    emitter.instruction("jge __rt_json_skip_ws_done_x");                        // return the limit unchanged when only whitespace remains
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load the candidate JSON whitespace byte
    emitter.instruction("cmp r8, 32");                                          // space?
    emitter.instruction("je __rt_json_skip_ws_step_x");                         // consume JSON space
    emitter.instruction("cmp r8, 9");                                           // tab?
    emitter.instruction("je __rt_json_skip_ws_step_x");                         // consume JSON tab
    emitter.instruction("cmp r8, 10");                                          // LF?
    emitter.instruction("je __rt_json_skip_ws_step_x");                         // consume JSON line feed
    emitter.instruction("cmp r8, 13");                                          // CR?
    emitter.instruction("jne __rt_json_skip_ws_done_x");                        // any other byte begins the next token
    emitter.label("__rt_json_skip_ws_step_x");
    emitter.instruction("add rcx, 1");                                          // advance over one JSON whitespace byte
    emitter.instruction("jmp __rt_json_skip_ws_loop_x");                        // continue until token or limit
    emitter.label("__rt_json_skip_ws_done_x");
    emitter.instruction("ret");                                                 // return with rcx holding the advanced cursor
}
