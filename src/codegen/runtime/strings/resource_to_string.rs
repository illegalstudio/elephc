//! Purpose:
//! Emits the `__rt_resource_to_string`, `__rt_resource_to_string_prefix_loop` runtime helper assembly for resource to string.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Resource helpers format or write runtime resource identifiers without claiming ownership of external descriptors.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Formats a native resource payload as the PHP display string "Resource id #N".
///
/// Uses the global concat buffer to build the result: copies the 13-byte prefix
/// `"Resource id #"`, then appends the decimal digits of `(payload + 1)`. Updates
/// `_concat_off` to reflect the total bytes written (prefix + digits). Returns the
/// final string pointer in x1 and length in x2.
///
/// # Inputs
/// - `x0 / rax`: native resource payload (0-based internal identifier)
///
/// # Outputs
/// - `x1 / rax`: pointer to the formatted string inside `_concat_buf`
/// - `x2 / rdx`: total byte length (13 + digit count)
///
/// # ABI details
/// - Clobbers x9–x15 on ARM64; r8–r11 on x86_64.
/// - Calls `__rt_itoa` which writes digits directly into `_concat_buf` starting at the
///   current `_concat_off`, then this function copies them to the final output position.
pub fn emit_resource_to_string(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resource_to_string_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resource_to_string ---");
    emitter.label_global("__rt_resource_to_string");

    emitter.instruction("sub sp, sp, #64");                                     // reserve locals for offsets, source pointers, and the saved return address
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address before calling itoa
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp]");                                        // preserve the native resource payload while building the output prefix
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer cursor
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the concat cursor address across the itoa call
    emitter.instruction("str x10, [sp, #16]");                                  // preserve the original concat-buffer offset
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute the final output start inside concat_buf
    emitter.instruction("str x12, [sp, #24]");                                  // preserve the final output start across the itoa call
    abi::emit_symbol_address(emitter, "x13", "_resource_id_prefix");
    emitter.instruction("mov x14, #0");                                         // initialize prefix-copy index
    emitter.label("__rt_resource_to_string_prefix_loop");
    emitter.instruction("cmp x14, #13");                                        // check whether all prefix bytes have been copied
    emitter.instruction("b.eq __rt_resource_to_string_prefix_done");            // stop after copying "Resource id #"
    emitter.instruction("ldrb w15, [x13, x14]");                                // load one prefix byte
    emitter.instruction("strb w15, [x12, x14]");                                // append the prefix byte to the final output buffer
    emitter.instruction("add x14, x14, #1");                                    // advance to the next prefix byte
    emitter.instruction("b __rt_resource_to_string_prefix_loop");               // continue copying the prefix
    emitter.label("__rt_resource_to_string_prefix_done");
    emitter.instruction("add x10, x10, #13");                                   // move the scratch cursor after the copied prefix
    emitter.instruction("str x10, [x9]");                                       // let itoa write its temporary digits after the prefix
    emitter.instruction("ldr x0, [sp]");                                        // reload the native resource payload
    emitter.instruction("add x0, x0, #1");                                      // convert the native payload into the 1-based display id
    abi::emit_call_label(emitter, "__rt_itoa");                                 // format the display id as temporary decimal digits
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the final output start
    emitter.instruction("add x12, x12, #13");                                   // compute the final digit destination after the prefix
    emitter.instruction("mov x14, #0");                                         // initialize digit-copy index
    emitter.label("__rt_resource_to_string_digit_loop");
    emitter.instruction("cmp x14, x2");                                         // check whether all formatted digits have been copied
    emitter.instruction("b.eq __rt_resource_to_string_digit_done");             // stop after copying the display id digits
    emitter.instruction("ldrb w15, [x1, x14]");                                 // load one temporary digit byte
    emitter.instruction("strb w15, [x12, x14]");                                // append the digit byte after the prefix
    emitter.instruction("add x14, x14, #1");                                    // advance to the next digit byte
    emitter.instruction("b __rt_resource_to_string_digit_loop");                // continue copying the display id digits
    emitter.label("__rt_resource_to_string_digit_done");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the concat cursor address
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the original concat-buffer offset
    emitter.instruction("add x10, x10, #13");                                   // account for the resource prefix bytes
    emitter.instruction("add x10, x10, x2");                                    // account for the formatted display id digits
    emitter.instruction("str x10, [x9]");                                       // publish the compact final concat-buffer cursor
    emitter.instruction("ldr x1, [sp, #24]");                                   // return the final resource string pointer
    emitter.instruction("add x2, x2, #13");                                     // return the final resource string length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the formatted resource string
}

/// x86_64-specific implementation of `emit_resource_to_string` for the Linux ABI.
/// Mirrors the ARM64 logic: prefix copy loop → `__rt_itoa` call → digit copy loop.
fn emit_resource_to_string_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_to_string ---");
    emitter.label_global("__rt_resource_to_string");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before using stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the helper body
    emitter.instruction("sub rsp, 48");                                         // reserve aligned locals for offsets and output pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the native resource payload while building the output prefix
    abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat-buffer cursor
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // preserve the concat cursor address across the itoa call
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");                        // preserve the original concat-buffer offset
    abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea r11, [r10 + r9]");                                 // compute the final output start inside concat_buf
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the final output start across the itoa call
    abi::emit_symbol_address(emitter, "rcx", "_resource_id_prefix");
    emitter.instruction("xor esi, esi");                                        // initialize prefix-copy index
    emitter.label("__rt_resource_to_string_prefix_loop_x86");
    emitter.instruction("cmp rsi, 13");                                         // check whether all prefix bytes have been copied
    emitter.instruction("je __rt_resource_to_string_prefix_done_x86");          // stop after copying "Resource id #"
    emitter.instruction("movzx edi, BYTE PTR [rcx + rsi]");                     // load one prefix byte
    emitter.instruction("mov BYTE PTR [r11 + rsi], dil");                       // append the prefix byte to the final output buffer
    emitter.instruction("inc rsi");                                             // advance to the next prefix byte
    emitter.instruction("jmp __rt_resource_to_string_prefix_loop_x86");         // continue copying the prefix
    emitter.label("__rt_resource_to_string_prefix_done_x86");
    emitter.instruction("add r9, 13");                                          // move the scratch cursor after the copied prefix
    emitter.instruction("mov QWORD PTR [r8], r9");                              // let itoa write its temporary digits after the prefix
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the native resource payload
    emitter.instruction("add rax, 1");                                          // convert the native payload into the 1-based display id
    abi::emit_call_label(emitter, "__rt_itoa");                                 // format the display id as temporary decimal digits
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the final output start
    emitter.instruction("lea r11, [r11 + 13]");                                 // compute the final digit destination after the prefix
    emitter.instruction("xor ecx, ecx");                                        // initialize digit-copy index
    emitter.label("__rt_resource_to_string_digit_loop_x86");
    emitter.instruction("cmp rcx, rdx");                                        // check whether all formatted digits have been copied
    emitter.instruction("je __rt_resource_to_string_digit_done_x86");           // stop after copying the display id digits
    emitter.instruction("movzx esi, BYTE PTR [rax + rcx]");                     // load one temporary digit byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], sil");                       // append the digit byte after the prefix
    emitter.instruction("inc rcx");                                             // advance to the next digit byte
    emitter.instruction("jmp __rt_resource_to_string_digit_loop_x86");          // continue copying the display id digits
    emitter.label("__rt_resource_to_string_digit_done_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the concat cursor address
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the original concat-buffer offset
    emitter.instruction("add r9, 13");                                          // account for the resource prefix bytes
    emitter.instruction("add r9, rdx");                                         // account for the formatted display id digits
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the compact final concat-buffer cursor
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the final resource string pointer
    emitter.instruction("add rdx, 13");                                         // return the final resource string length
    emitter.instruction("add rsp, 48");                                         // release the helper locals
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the formatted resource string
}
