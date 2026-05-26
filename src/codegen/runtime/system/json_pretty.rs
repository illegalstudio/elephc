//! Purpose:
//! Emits shared JSON pretty-print whitespace helpers used by container encoders.
//! The helpers append indentation during encoding instead of scanning a completed buffer.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//! - JSON container encoders when `JSON_PRETTY_PRINT` is active.
//!
//! Key details:
//! - `_json_indent_depth` tracks formatting depth separately from JSON depth-limit enforcement.
//! - Helpers are no-ops when the active flag set does not include `JSON_PRETTY_PRINT`.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the four JSON pretty-print runtime helpers: `__rt_json_pretty_push`,
/// `__rt_json_pretty_pop`, `__rt_json_pretty_line`, and `__rt_json_pretty_colon_space`.
///
/// `push` increments `_json_indent_depth` when `JSON_PRETTY_PRINT` is active.
/// `pop` decrements it, guarded against underflow.
/// `line` appends a newline followed by `depth × 4` spaces to the output buffer,
/// returning the updated write pointer in `x11` (ARM64) or `r11` (x86_64).
/// `colon_space` appends a single space after an object key colon when pretty-printing.
///
/// All four helpers are no-ops when the active flags do not include `JSON_PRETTY_PRINT`
/// (flag bit 128), leaving buffer state and depth unchanged.
pub(crate) fn emit_json_pretty_helpers(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_pretty_helpers_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_pretty_helpers ---");

    emitter.label_global("__rt_json_pretty_push");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [x9]");                                       // load the active JSON flag bitmask
    emitter.instruction("tst x10, #128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("b.eq __rt_json_pretty_push_done");                     // leave formatting depth unchanged for compact output
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_indent_depth");
    emitter.instruction("ldr x10, [x9]");                                       // load the current pretty-print indentation depth
    emitter.instruction("add x10, x10, #1");                                    // enter one emitted JSON container level
    emitter.instruction("str x10, [x9]");                                       // publish the updated indentation depth
    emitter.label("__rt_json_pretty_push_done");
    emitter.instruction("ret");                                                 // return to the container encoder

    emitter.label_global("__rt_json_pretty_pop");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [x9]");                                       // load the active JSON flag bitmask
    emitter.instruction("tst x10, #128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("b.eq __rt_json_pretty_pop_done");                      // compact output does not maintain pretty indentation depth
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_indent_depth");
    emitter.instruction("ldr x10, [x9]");                                       // load the current pretty-print indentation depth
    emitter.instruction("cbz x10, __rt_json_pretty_pop_done");                  // guard against underflow after exceptional control flow
    emitter.instruction("sub x10, x10, #1");                                    // leave one emitted JSON container level
    emitter.instruction("str x10, [x9]");                                       // publish the updated indentation depth
    emitter.label("__rt_json_pretty_pop_done");
    emitter.instruction("ret");                                                 // return to the container encoder

    emitter.label_global("__rt_json_pretty_line");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [x9]");                                       // load the active JSON flag bitmask
    emitter.instruction("tst x10, #128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("b.eq __rt_json_pretty_line_done");                     // compact output keeps the write pointer unchanged
    emitter.instruction("mov w12, #10");                                        // ASCII newline
    emitter.instruction("strb w12, [x11]");                                     // append the pretty-print line break
    emitter.instruction("add x11, x11, #1");                                    // advance past the newline byte
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_indent_depth");
    emitter.instruction("ldr x10, [x9]");                                       // load the current pretty-print indentation depth
    emitter.instruction("lsl x10, x10, #2");                                    // convert depth to PHP's four-space indent width
    emitter.instruction("mov x13, #0");                                         // initialize the emitted-space counter
    emitter.label("__rt_json_pretty_line_loop");
    emitter.instruction("cmp x13, x10");                                        // have all indent spaces been appended?
    emitter.instruction("b.ge __rt_json_pretty_line_done");                     // return once this line is fully indented
    emitter.instruction("mov w12, #32");                                        // ASCII space
    emitter.instruction("strb w12, [x11]");                                     // append one indentation space
    emitter.instruction("add x11, x11, #1");                                    // advance past the space byte
    emitter.instruction("add x13, x13, #1");                                    // count the emitted indentation space
    emitter.instruction("b __rt_json_pretty_line_loop");                        // continue emitting indentation spaces
    emitter.label("__rt_json_pretty_line_done");
    emitter.instruction("ret");                                                 // return x11 as the updated write pointer

    emitter.label_global("__rt_json_pretty_colon_space");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [x9]");                                       // load the active JSON flag bitmask
    emitter.instruction("tst x10, #128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("b.eq __rt_json_pretty_colon_space_done");              // compact output does not add a key/value space
    emitter.instruction("mov w12, #32");                                        // ASCII space after the object colon
    emitter.instruction("strb w12, [x11]");                                     // append PHP's pretty-print key/value separator space
    emitter.instruction("add x11, x11, #1");                                    // advance past the separator space
    emitter.label("__rt_json_pretty_colon_space_done");
    emitter.instruction("ret");                                                 // return x11 as the updated write pointer
}

/// x86_64-specific implementation of the four JSON pretty-print helpers.
/// Mirrors the ARM64 helpers but uses x86_64 registers (`r10`, `r11`, `rcx`) and
/// RIP-relative symbol addresses. Underflow guards and flag checks are identical
/// to the ARM64 path; labels use `_x` suffix to avoid collisions.
fn emit_json_pretty_helpers_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_pretty_helpers ---");

    emitter.label_global("__rt_json_pretty_push");
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // load the active JSON flag bitmask
    emitter.instruction("test r10, 128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("je __rt_json_pretty_push_done_x");                     // leave formatting depth unchanged for compact output
    emitter.instruction("mov r10, QWORD PTR [rip + _json_indent_depth]");       // load the current pretty-print indentation depth
    emitter.instruction("add r10, 1");                                          // enter one emitted JSON container level
    emitter.instruction("mov QWORD PTR [rip + _json_indent_depth], r10");       // publish the updated indentation depth
    emitter.label("__rt_json_pretty_push_done_x");
    emitter.instruction("ret");                                                 // return to the container encoder

    emitter.label_global("__rt_json_pretty_pop");
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // load the active JSON flag bitmask
    emitter.instruction("test r10, 128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("je __rt_json_pretty_pop_done_x");                      // compact output does not maintain pretty indentation depth
    emitter.instruction("mov r10, QWORD PTR [rip + _json_indent_depth]");       // load the current pretty-print indentation depth
    emitter.instruction("test r10, r10");                                       // is the formatting depth already at the root?
    emitter.instruction("je __rt_json_pretty_pop_done_x");                      // avoid underflow after exceptional control flow
    emitter.instruction("sub r10, 1");                                          // leave one emitted JSON container level
    emitter.instruction("mov QWORD PTR [rip + _json_indent_depth], r10");       // publish the updated indentation depth
    emitter.label("__rt_json_pretty_pop_done_x");
    emitter.instruction("ret");                                                 // return to the container encoder

    emitter.label_global("__rt_json_pretty_line");
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // load the active JSON flag bitmask
    emitter.instruction("test r10, 128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("je __rt_json_pretty_line_done_x");                     // compact output keeps the write pointer unchanged
    emitter.instruction("mov BYTE PTR [r11], 10");                              // append the pretty-print line break
    emitter.instruction("add r11, 1");                                          // advance past the newline byte
    emitter.instruction("mov r10, QWORD PTR [rip + _json_indent_depth]");       // load the current pretty-print indentation depth
    emitter.instruction("shl r10, 2");                                          // convert depth to PHP's four-space indent width
    emitter.instruction("xor rcx, rcx");                                        // initialize the emitted-space counter
    emitter.label("__rt_json_pretty_line_loop_x");
    emitter.instruction("cmp rcx, r10");                                        // have all indent spaces been appended?
    emitter.instruction("jae __rt_json_pretty_line_done_x");                    // return once this line is fully indented
    emitter.instruction("mov BYTE PTR [r11], 32");                              // append one indentation space
    emitter.instruction("add r11, 1");                                          // advance past the space byte
    emitter.instruction("add rcx, 1");                                          // count the emitted indentation space
    emitter.instruction("jmp __rt_json_pretty_line_loop_x");                    // continue emitting indentation spaces
    emitter.label("__rt_json_pretty_line_done_x");
    emitter.instruction("ret");                                                 // return r11 as the updated write pointer

    emitter.label_global("__rt_json_pretty_colon_space");
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // load the active JSON flag bitmask
    emitter.instruction("test r10, 128");                                       // is JSON_PRETTY_PRINT active?
    emitter.instruction("je __rt_json_pretty_colon_space_done_x");              // compact output does not add a key/value space
    emitter.instruction("mov BYTE PTR [r11], 32");                              // append PHP's pretty-print key/value separator space
    emitter.instruction("add r11, 1");                                          // advance past the separator space
    emitter.label("__rt_json_pretty_colon_space_done_x");
    emitter.instruction("ret");                                                 // return r11 as the updated write pointer
}
