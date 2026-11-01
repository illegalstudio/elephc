//! Purpose:
//! Emits the `__rt_sodium_box` runtime helper behind the internal `__elephc_sodium_box`
//! builtin, which `sodium_prelude` wraps as PHP's `sodium_crypto_box_*` functions.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via the string-runtime module.
//!
//! Key details:
//! - `elephc_crypto_sodium` is called through the `_elephc_crypto_sodium_fn` slot, published at
//!   the call site, so only programs that use the sealed-box surface link `-lelephc_crypto`.
//! - The output buffer is an owned heap string sized `first_len + 64`, which covers every
//!   operation (a seal adds 48 bytes, a keypair is 64). On failure the same buffer is returned
//!   with length 0, so the result is always a well-formed owned string.
//! - The bridge status is stored in `_elephc_sodium_status`, which `__elephc_sodium_status()`
//!   reads back; the prelude wrappers turn it into `SodiumException` or `false`.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Status recorded when the bridge slot was never published (`SODIUM_ERR_INTERNAL`).
const STATUS_BRIDGE_MISSING: i64 = -5;

/// Emits `__rt_sodium_box` for the active target.
///
/// Input registers:
///   AArch64: x0 = operation, x1/x2 = first string ptr/len, x3/x4 = second string ptr/len.
///   x86_64:  r10 = operation, rax/rdx = first string ptr/len, rdi/rsi = second string ptr/len.
///
/// Output registers (owned PHP string ptr/len):
///   AArch64: x1 = ptr, x2 = len.   x86_64: rax = ptr, rdx = len.
pub fn emit_sodium_box(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sodium_box_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sodium_box ---");
    emitter.label_global("__rt_sodium_box");
    // -- frame: [sp,#0]=op [#8..#40)=inputs [#40]=out ptr [#48]=out len [#64]=fp/lr --
    emitter.instruction("sub sp, sp, #80");                                     // reserve inputs, output state, and saved frame (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve caller frame and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the sodium operation code
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the first string across the allocation
    emitter.instruction("stp x3, x4, [sp, #24]");                               // save the second string across the allocation
    emitter.instruction("add x0, x2, #64");                                     // capacity = first length + 64 covers every operation
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the output buffer as an owned string
    emitter.instruction("str x0, [sp, #40]");                                   // retain the output buffer pointer
    emitter.instruction("str xzr, [sp, #48]");                                  // initialize the produced output length

    // -- elephc_crypto_sodium(op, first, first_len, second, second_len, out, cap, &out_len) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // C arg0 = operation code
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // C arg1/arg2 = first string ptr/len
    emitter.instruction("ldp x3, x4, [sp, #24]");                               // C arg3/arg4 = second string ptr/len
    emitter.instruction("ldr x5, [sp, #40]");                                   // C arg5 = output buffer
    emitter.instruction("add x6, x2, #64");                                     // C arg6 = output capacity
    emitter.instruction("add x7, sp, #48");                                     // C arg7 = produced-length destination
    abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_sodium_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published sodium bridge entry
    emitter.instruction(&format!("mov x0, #{}", STATUS_BRIDGE_MISSING));        // status when the bridge was never published
    emitter.instruction("cbz x9, __rt_sodium_box_status");                      // missing bridge reports an internal error
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore C arg0 after the status preset
    abi::emit_call_reg(emitter, "x9");
    emitter.instruction("sxtw x0, w0");                                         // widen the C int status to a PHP int
    emitter.label("__rt_sodium_box_status");
    abi::emit_symbol_address(emitter, "x9", "_elephc_sodium_status");
    emitter.instruction("str x0, [x9]");                                        // record the status for __elephc_sodium_status()
    emitter.instruction("ldr x1, [sp, #40]");                                   // return the owned output buffer
    emitter.instruction("ldr x2, [sp, #48]");                                   // produced length (0 on failure)
    emitter.instruction("cmp x0, #0");                                          // did the bridge succeed?
    emitter.instruction("csel x2, x2, xzr, eq");                                // a failed operation returns an empty string
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore caller frame and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned string in x1/x2
}

/// Emits the x86_64 Linux variant of `__rt_sodium_box`; see [`emit_sodium_box`].
fn emit_sodium_box_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sodium_box ---");
    emitter.label_global("__rt_sodium_box");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable local addressing
    emitter.instruction("sub rsp, 80");                                         // reserve two stack args plus local state (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the sodium operation code
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the first string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the first string length
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the second string pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // save the second string length
    emitter.instruction("lea rax, [rdx + 64]");                                 // capacity = first length + 64 covers every operation
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
    ));                                                                          // materialize the owned-string heap-kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the output buffer as an owned string
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // retain the output buffer pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the produced output length

    // -- elephc_crypto_sodium(op, first, first_len, second, second_len, out, cap, &out_len) --
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // first string length
    emitter.instruction("add r11, 64");                                         // output capacity
    emitter.instruction("mov QWORD PTR [rsp + 0], r11");                        // C stack arg6 = output capacity
    emitter.instruction("lea r11, [rbp - 56]");                                 // produced-length destination
    emitter.instruction("mov QWORD PTR [rsp + 8], r11");                        // C stack arg7 = produced-length pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // C arg0 = operation code
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // C arg1 = first string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // C arg2 = first string length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // C arg3 = second string pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // C arg4 = second string length
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // C arg5 = output buffer
    abi::emit_load_symbol_to_reg(emitter, "r11", "_elephc_crypto_sodium_fn", 0);
    emitter.instruction(&format!("mov rax, {}", STATUS_BRIDGE_MISSING));        // status when the bridge was never published
    emitter.instruction("test r11, r11");                                       // was the sodium bridge published?
    emitter.instruction("jz __rt_sodium_box_status_linux_x86_64");              // missing bridge reports an internal error
    abi::emit_call_reg(emitter, "r11");
    emitter.instruction("movsxd rax, eax");                                     // widen the C int status to a PHP int
    emitter.label("__rt_sodium_box_status_linux_x86_64");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_elephc_sodium_status", 0);  // record the status for __elephc_sodium_status()
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // produced length (0 on failure)
    emitter.instruction("test rax, rax");                                       // did the bridge succeed?
    emitter.instruction("mov rax, 0");                                          // zero length for a failed operation
    emitter.instruction("cmovne rdx, rax");                                     // a failed operation returns an empty string
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the owned output buffer
    emitter.instruction("add rsp, 80");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned string in rax/rdx
}
