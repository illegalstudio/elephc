//! Purpose:
//! Emits the `stream_wrapper_register` runtime helper
//! `__rt_stream_wrapper_register`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_wrapper_register` is the entry point invoked by the
//!   `stream_wrapper_register` builtin.
//!
//! Key details:
//! - Stores `(protocol_ptr, protocol_len, class_ptr, class_len)` tuples in the
//!   heap-backed registration table and the matching registration flags in the
//!   parallel flag table. An empty slot has a null `protocol_ptr`.
//! - The slot comes from `__rt_user_wrappers_reserve`, which grows the table on
//!   demand, so registration is bounded only by the heap. PHP imposes no limit,
//!   and the previous fixed 64-slot array silently refused the 65th call.
//! - Both names are copied into owned heap storage via `__rt_str_persist`
//!   before they are stored: a registration outlives the caller's buffer, and a
//!   PHP-level `$scheme = "dyn" . $i;` reuses one local slot per iteration, so
//!   keeping the borrowed pointer made every entry alias the final value.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Warning PHP emits when the protocol contains a byte outside `[A-Za-z0-9+.-]`.
///
/// Reference PHP appends the class name, the protocol and the script location; the
/// runtime's other warnings (`fopen()`, `file_get_contents()`) are fixed strings
/// without a location, and this follows that convention rather than inventing a
/// half-interpolated one.
pub(crate) const BAD_PROTOCOL_WARNING: &str =
    "Warning: stream_wrapper_register(): Invalid protocol scheme specified.\n";
/// Warning PHP emits when the protocol is already registered — including the builtin
/// `file://`, which a program could otherwise shadow silently.
pub(crate) const DUPLICATE_PROTOCOL_WARNING: &str =
    "Warning: stream_wrapper_register(): Protocol is already defined.\n";

/// Emits the `__rt_stream_wrapper_register` runtime helper.
/// Input:  AArch64 x0 = proto ptr, x1 = proto len, x2 = class ptr, x3 = class len,
///         x4 = flags. x86_64 uses rdi/rsi/rdx/rcx/r8 for the same values.
/// Output: 1 when the registration was stored, 0 when the table is full.
pub fn emit_stream_wrapper_register(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_wrapper_register_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");
    // The registration outlives every caller-owned buffer, so the helper needs a
    // real frame: `__rt_str_persist` is a call and would otherwise clobber LR.
    emitter.instruction("stp x29, x30, [sp, #-80]!");                           // establish the frame and preserve the return address
    emitter.instruction("mov x29, sp");                                         // frame pointer for the persisted-argument spill area
    emitter.instruction("str x0, [sp, #16]");                                   // spill the borrowed protocol pointer
    emitter.instruction("str x1, [sp, #24]");                                   // spill the protocol length
    emitter.instruction("str x2, [sp, #32]");                                   // spill the borrowed class-name pointer
    emitter.instruction("str x3, [sp, #40]");                                   // spill the class-name length
    emitter.instruction("str x4, [sp, #48]");                                   // spill the registration flags

    // -- reserve a free slot, growing the table when every slot is taken --
    // Reserving happens BEFORE persisting so a failed reservation cannot leak
    // owned copies. PHP never refuses a registration, so the only failure mode
    // left is heap exhaustion, which the helper reports as -1.
    emitter.instruction("bl __rt_user_wrappers_reserve");                       // x0 = free slot index (-1 on heap exhaustion)
    emitter.instruction("cmp x0, #0");                                          // did the reservation fail?
    emitter.instruction("b.lt __rt_swr_full");                                  // report false when no slot could be reserved
    emitter.instruction("mov x5, x0");                                          // wrapper slot index

    // -- copy both names onto the heap so the table never aliases caller storage --
    // A PHP-level `$scheme = "dyn" . $i;` reuses one local slot per iteration, so
    // storing the borrowed pointer made every registration alias the final value.
    emitter.label("__rt_swr_found");
    emitter.instruction("str x5, [sp, #56]");                                   // remember the target slot index across the calls
    emitter.instruction("ldr x1, [sp, #16]");                                   // borrowed protocol pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // protocol length
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned protocol copy
    emitter.instruction("str x1, [sp, #16]");                                   // replace the spill with the owned pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // borrowed class-name pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // class-name length
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned class-name copy
    emitter.instruction("str x1, [sp, #32]");                                   // replace the spill with the owned pointer

    // -- store the registration into the reserved slot --
    // The slot base is re-derived: the persist calls clobber the caller-saved scratch.
    super::emit_load_table_base(emitter, "x6");
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the target slot index
    emitter.instruction("add x7, x6, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("str x9, [x7]");                                        // owned protocol pointer
    emitter.instruction("ldr x9, [sp, #24]");
    emitter.instruction("str x9, [x7, #8]");                                    // protocol length
    emitter.instruction("ldr x9, [sp, #32]");
    emitter.instruction("str x9, [x7, #16]");                                   // owned class-name pointer
    emitter.instruction("ldr x9, [sp, #40]");
    emitter.instruction("str x9, [x7, #24]");                                   // class-name length
    super::emit_load_flags_base(emitter, "x10");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the registration flags
    emitter.instruction("str x9, [x10, x5, lsl #3]");                           // store definition flags beside the registration slot
    emitter.instruction("mov x0, #1");                                          // return true for a successful registration
    emitter.instruction("ldp x29, x30, [sp], #80");                             // tear down the frame
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_full");
    emitter.instruction("mov x0, #0");                                          // return false when the table is full
    emitter.instruction("ldp x29, x30, [sp], #80");                             // tear down the frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream wrapper register.
fn emit_stream_wrapper_register_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");
    // The registration outlives every caller-owned buffer, so the helper needs a
    // real frame: `__rt_str_persist` is a call and would otherwise clobber scratch.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the spill-area frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve the persisted-argument spill slots (keeps rsp 16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // spill the borrowed protocol pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // spill the protocol length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // spill the borrowed class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // spill the class-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // spill the registration flags

    // -- reserve a free slot, growing the table when every slot is taken --
    // Reserving happens BEFORE persisting so a failed reservation cannot leak
    // owned copies. PHP never refuses a registration, so the only failure mode
    // left is heap exhaustion, which the helper reports as -1.
    emitter.instruction("call __rt_user_wrappers_reserve");                     // rax = free slot index (-1 on heap exhaustion)
    emitter.instruction("test rax, rax");                                       // did the reservation fail?
    emitter.instruction("js __rt_swr_full_x86");                                // report false when no slot could be reserved
    emitter.instruction("mov r9, rax");                                         // wrapper slot index

    // -- copy both names onto the heap so the table never aliases caller storage --
    // A PHP-level `$scheme = "dyn" . $i;` reuses one local slot per iteration, so
    // storing the borrowed pointer made every registration alias the final value.
    // NOTE: __rt_str_persist consumes the source pointer in rax (not rdi) on x86_64.
    emitter.label("__rt_swr_found_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // remember the target slot index across the calls
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // borrowed protocol pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // protocol length
    emitter.instruction("call __rt_str_persist");                               // rax = owned protocol copy
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // replace the spill with the owned pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // borrowed class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // class-name length
    emitter.instruction("call __rt_str_persist");                               // rax = owned class-name copy
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // replace the spill with the owned pointer

    // -- store the registration into the reserved slot --
    // The slot base is re-derived: the persist calls clobber the caller-saved scratch.
    super::emit_load_table_base(emitter, "r8");                                 // wrapper table base
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the target slot index
    emitter.instruction("mov r10, r9");                                         // copy the slot index for scaling
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("mov QWORD PTR [r10], rax");                            // owned protocol pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // protocol length
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // owned class-name pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // class-name length
    super::emit_load_flags_base(emitter, "r10");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the registration flags
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], rax");                   // store definition flags beside the registration slot
    emitter.instruction("mov eax, 1");                                          // return true for a successful registration
    emitter.instruction("mov rsp, rbp");                                        // discard the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_full_x86");
    emitter.instruction("xor eax, eax");                                        // return false when the table is full
    emitter.instruction("mov rsp, rbp");                                        // discard the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
