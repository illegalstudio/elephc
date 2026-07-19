//! Purpose:
//! Emits the `__rt_long2ip` runtime helper assembly for the long2ip builtin.
//! Formats a 32-bit integer as a dotted-quad IPv4 string in the concat buffer.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Each octet is written without leading zeros; the result is a borrowed
//!   `_concat_buf` slice, matching `__rt_itoa`'s string-result convention.

use crate::codegen_support::abi::emit_symbol_address;
use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::abi;

/// long2ip: format the low 32 bits of an integer as `A.B.C.D`.
/// Input:  x0 = IP integer
/// Output: x1 = string pointer (in concat_buf), x2 = length
pub fn emit_long2ip(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_long2ip_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: long2ip ---");
    emitter.label_global("__rt_long2ip");

    // -- record the result start inside the concat buffer --
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute the result start pointer
    emitter.instruction("mov x13, x12");                                        // x13 is the running write cursor

    // -- write the four dotted octets, most significant first --
    emit_octet_aarch64(emitter, 24, 0, true);
    emit_octet_aarch64(emitter, 16, 1, true);
    emit_octet_aarch64(emitter, 8, 2, true);
    emit_octet_aarch64(emitter, 0, 3, false);

    // -- return the slice and publish the new concat-buffer offset --
    emitter.instruction("sub x2, x13, x12");                                    // result length = cursor - start
    emitter.instruction("mov x1, x12");                                         // result pointer = start
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // reload the concat-buffer offset
    emitter.instruction("add x10, x10, x2");                                    // advance it past the formatted address
    emitter.instruction("str x10, [x9]");                                       // publish the updated concat-buffer offset
    emitter.instruction("ret");                                                 // return the dotted-quad string slice
}

/// Emits one octet `(n >> shift) & 0xff` as 1-3 decimal digits with no leading
/// zeros, followed by a `.` when `with_dot` is set. Cursor is x13; n stays x0.
fn emit_octet_aarch64(emitter: &mut Emitter, shift: u32, index: u32, with_dot: bool) {
    let no_h = format!("__rt_long2ip_no_h_{}", index);
    let units = format!("__rt_long2ip_units_{}", index);

    if shift > 0 {
        emitter.instruction(&format!("lsr x14, x0, #{}", shift));               // shift the wanted octet down
    } else {
        emitter.instruction("mov x14, x0");                                     // the final octet needs no shift
    }
    emitter.instruction("and x14, x14, #0xff");                                 // isolate the 8-bit octet value
    emitter.instruction("mov x2, #100");                                        // divisor for the hundreds digit
    emitter.instruction("udiv x3, x14, x2");                                    // x3 = hundreds digit
    emitter.instruction("msub x4, x3, x2, x14");                                // x4 = octet minus the hundreds
    emitter.instruction("mov x2, #10");                                         // divisor for the tens digit
    emitter.instruction("udiv x5, x4, x2");                                     // x5 = tens digit
    emitter.instruction("msub x6, x5, x2, x4");                                 // x6 = units digit
    emitter.instruction(&format!("cbz x3, {}", no_h));                          // skip the hundreds digit when zero
    emitter.instruction("add w7, w3, #48");                                     // hundreds digit to ASCII
    emitter.instruction("strb w7, [x13]");                                      // write the hundreds digit
    emitter.instruction("add x13, x13, #1");                                    // advance the cursor
    emitter.instruction("add w7, w5, #48");                                     // tens digit to ASCII (forced after hundreds)
    emitter.instruction("strb w7, [x13]");                                      // write the tens digit
    emitter.instruction("add x13, x13, #1");                                    // advance the cursor
    emitter.instruction(&format!("b {}", units));                               // hundreds path always writes the tens
    emitter.label(&no_h);
    emitter.instruction(&format!("cbz x5, {}", units));                         // skip the tens digit when zero
    emitter.instruction("add w7, w5, #48");                                     // tens digit to ASCII
    emitter.instruction("strb w7, [x13]");                                      // write the tens digit
    emitter.instruction("add x13, x13, #1");                                    // advance the cursor
    emitter.label(&units);
    emitter.instruction("add w7, w6, #48");                                     // units digit to ASCII
    emitter.instruction("strb w7, [x13]");                                      // write the units digit
    emitter.instruction("add x13, x13, #1");                                    // advance the cursor
    if with_dot {
        emitter.instruction("mov w7, #46");                                     // ASCII '.' separator
        emitter.instruction("strb w7, [x13]");                                  // write the separator
        emitter.instruction("add x13, x13, #1");                                // advance the cursor
    }
}

/// Emits the Linux x86_64 string runtime helper for long2ip.
fn emit_long2ip_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: long2ip ---");
    emitter.label_global("__rt_long2ip");

    abi::emit_symbol_address(emitter, "rax", "_concat_buf");                    // concat-buffer base address
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_concat_off", 0);             // current concat-buffer offset
    emitter.instruction("lea r11, [rax + rcx]");                                // r11 = result start pointer
    emitter.instruction("mov r10, r11");                                        // r10 is the running write cursor

    emit_octet_x86(emitter, 24, 0, true);
    emit_octet_x86(emitter, 16, 1, true);
    emit_octet_x86(emitter, 8, 2, true);
    emit_octet_x86(emitter, 0, 3, false);

    emitter.instruction("mov rdx, r10");                                        // cursor
    emitter.instruction("sub rdx, r11");                                        // result length = cursor - start
    emitter.instruction("mov rax, r11");                                        // result pointer = start
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_concat_off", 0);             // reload the concat-buffer offset
    emitter.instruction("add rcx, rdx");                                        // advance it past the formatted address
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_concat_off", 0);            // publish the updated offset
    emitter.instruction("ret");                                                 // return the dotted-quad string slice
}

/// x86_64 counterpart of `emit_octet_aarch64`. Cursor is r10; n stays rdi.
fn emit_octet_x86(emitter: &mut Emitter, shift: u32, index: u32, with_dot: bool) {
    let no_h = format!("__rt_long2ip_no_h_x86_{}", index);
    let units = format!("__rt_long2ip_units_x86_{}", index);

    emitter.instruction("mov rax, rdi");                                        // load the IP integer
    if shift > 0 {
        emitter.instruction(&format!("shr rax, {}", shift));                    // shift the wanted octet down
    }
    emitter.instruction("and rax, 0xff");                                       // isolate the 8-bit octet value
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half
    emitter.instruction("mov ecx, 100");                                        // divisor for the hundreds digit
    emitter.instruction("div rcx");                                             // rax = hundreds, rdx = remainder
    emitter.instruction("mov r8, rax");                                         // save the hundreds digit
    emitter.instruction("mov rax, rdx");                                        // remainder becomes the next dividend
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half
    emitter.instruction("mov ecx, 10");                                         // divisor for the tens digit
    emitter.instruction("div rcx");                                             // rax = tens, rdx = units
    emitter.instruction("mov r9, rax");                                         // save the tens digit
    emitter.instruction("test r8, r8");                                         // is the hundreds digit zero?
    emitter.instruction(&format!("jz {}", no_h));                               // skip the hundreds digit when zero
    emitter.instruction("add r8b, 48");                                         // hundreds digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], r8b");                             // write the hundreds digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction("mov rax, r9");                                         // tens digit (forced after hundreds)
    emitter.instruction("add al, 48");                                          // tens digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], al");                              // write the tens digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.instruction(&format!("jmp {}", units));                             // hundreds path always writes the tens
    emitter.label(&no_h);
    emitter.instruction("test r9, r9");                                         // is the tens digit zero?
    emitter.instruction(&format!("jz {}", units));                              // skip the tens digit when zero
    emitter.instruction("mov rax, r9");                                         // tens digit
    emitter.instruction("add al, 48");                                          // tens digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], al");                              // write the tens digit
    emitter.instruction("inc r10");                                             // advance the cursor
    emitter.label(&units);
    emitter.instruction("add dl, 48");                                          // units digit to ASCII
    emitter.instruction("mov BYTE PTR [r10], dl");                              // write the units digit
    emitter.instruction("inc r10");                                             // advance the cursor
    if with_dot {
        emitter.instruction("mov BYTE PTR [r10], 46");                          // ASCII '.' separator
        emitter.instruction("inc r10");                                         // advance the cursor
    }
}
