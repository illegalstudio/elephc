//! Purpose:
//! Emits PCNTL runtime adapters that translate stable bridge records into PHP values.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - The resource-usage input is the 17-word `ElephcPcntlRUsage` C ABI, not libc's
//!   target-specific `rusage` layout.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// PHP resource-usage key plus its byte offset in the stable bridge record.
const RUSAGE_FIELDS: &[(&str, usize, usize)] = &[
    ("_pcntl_rusage_oublock", 10, 0),
    ("_pcntl_rusage_inblock", 10, 8),
    ("_pcntl_rusage_msgsnd", 9, 16),
    ("_pcntl_rusage_msgrcv", 9, 24),
    ("_pcntl_rusage_maxrss", 9, 32),
    ("_pcntl_rusage_ixrss", 8, 40),
    ("_pcntl_rusage_idrss", 8, 48),
    ("_pcntl_rusage_minflt", 9, 56),
    ("_pcntl_rusage_majflt", 9, 64),
    ("_pcntl_rusage_nsignals", 11, 72),
    ("_pcntl_rusage_nvcsw", 8, 80),
    ("_pcntl_rusage_nivcsw", 9, 88),
    ("_pcntl_rusage_nswap", 8, 96),
    ("_pcntl_rusage_utime_usec", 16, 104),
    ("_pcntl_rusage_utime_sec", 15, 112),
    ("_pcntl_rusage_stime_usec", 16, 120),
    ("_pcntl_rusage_stime_sec", 15, 128),
];

/// PHP siginfo key, key length, stable-record offset, and presence-mask bit.
const SIGINFO_FIELDS: &[(&str, usize, usize, usize)] = &[
    ("_pcntl_siginfo_signo", 5, 0, 0),
    ("_pcntl_siginfo_errno", 5, 8, 1),
    ("_pcntl_siginfo_code", 4, 16, 2),
    ("_pcntl_siginfo_status", 6, 24, 3),
    ("_pcntl_siginfo_pid", 3, 32, 4),
    ("_pcntl_siginfo_uid", 3, 40, 5),
    ("_pcntl_siginfo_utime", 5, 48, 6),
    ("_pcntl_siginfo_stime", 5, 56, 7),
    ("_pcntl_siginfo_addr", 4, 64, 8),
    ("_pcntl_siginfo_band", 4, 72, 9),
    ("_pcntl_siginfo_fd", 2, 80, 10),
];

/// Emits `__rt_pcntl_rusage_array`, which maps a stable bridge record to `array<string,int>`.
pub(crate) fn emit_pcntl_rusage_array(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_pcntl_rusage_array_aarch64(emitter),
        Arch::X86_64 => emit_pcntl_rusage_array_x86_64(emitter),
    }
}

/// Emits `__rt_pcntl_siginfo_array`, honoring the stable record's per-field presence mask.
pub(crate) fn emit_pcntl_siginfo_array(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_pcntl_siginfo_array_aarch64(emitter),
        Arch::X86_64 => emit_pcntl_siginfo_array_x86_64(emitter),
    }
}

/// Emits callable invocation, pending-queue dispatch, and register-preserving async safe points.
pub(crate) fn emit_pcntl_signal_dispatch(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_pcntl_invoke_descriptor_aarch64(emitter);
            emit_pcntl_dispatch_pending_aarch64(emitter);
            emit_pcntl_async_dispatch_preserving_aarch64(emitter);
            emit_pcntl_release_handlers_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_pcntl_invoke_descriptor_x86_64(emitter);
            emit_pcntl_dispatch_pending_x86_64(emitter);
            emit_pcntl_async_dispatch_preserving_x86_64(emitter);
            emit_pcntl_release_handlers_x86_64(emitter);
        }
    }
}

/// Emits the AArch64 resource-usage array builder.
fn emit_pcntl_rusage_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl resource usage array ---");
    emitter.label_global("__rt_pcntl_rusage_array");
    emitter.instruction("sub sp, sp, #32");                                  // reserve saved frame, record, and hash slots
    emitter.instruction("stp x29, x30, [sp, #16]");                          // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                 // establish the helper frame
    emitter.instruction("str x0, [sp]");                                     // preserve the stable usage-record pointer
    emitter.instruction("mov x0, #32");                                      // hash capacity
    emitter.instruction("mov x1, #0");                                       // hash value type = Int
    emitter.instruction("bl __rt_hash_new");                                 // allocate the PHP associative array
    emitter.instruction("str x0, [sp, #8]");                                 // preserve the current hash pointer
    for (symbol, length, offset) in RUSAGE_FIELDS {
        emitter.instruction("ldr x0, [sp, #8]");                              // C arg0 = current hash
        abi::emit_symbol_address(emitter, "x1", symbol);                      // C arg1 = string key bytes
        emitter.instruction(&format!("mov x2, #{}", length));                 // C arg2 = key length
        emitter.instruction("ldr x9, [sp]");                                 // reload usage-record pointer
        emitter.instruction(&format!("ldr x3, [x9, #{}]", offset));          // C arg3 = integer value
        emitter.instruction("mov x4, #0");                                   // unused high value word
        emitter.instruction("mov x5, #0");                                   // runtime value tag = Int
        emitter.instruction("bl __rt_hash_set");                             // insert the field
        emitter.instruction("str x0, [sp, #8]");                             // retain a potentially reallocated hash
    }
    emitter.instruction("ldr x0, [sp, #8]");                                 // return the completed hash
    emitter.instruction("ldp x29, x30, [sp, #16]");                          // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                  // release helper frame
    emitter.instruction("ret");
}

/// Emits the x86_64 SysV resource-usage array builder.
fn emit_pcntl_rusage_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl resource usage array ---");
    emitter.label_global("__rt_pcntl_rusage_array");
    emitter.instruction("push rbp");                                         // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                     // establish stable local offsets
    emitter.instruction("sub rsp, 16");                                      // reserve record and hash slots, keeping call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                     // preserve the stable usage-record pointer
    emitter.instruction("mov rdi, 32");                                      // hash capacity
    emitter.instruction("mov rsi, 0");                                       // hash value type = Int
    emitter.instruction("call __rt_hash_new");                               // allocate the PHP associative array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                    // preserve the current hash pointer
    for (symbol, length, offset) in RUSAGE_FIELDS {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                // C arg0 = current hash
        abi::emit_symbol_address(emitter, "rsi", symbol);                     // C arg1 = string key bytes
        emitter.instruction(&format!("mov rdx, {}", length));                 // C arg2 = key length
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                 // reload usage-record pointer
        emitter.instruction(&format!("mov rcx, QWORD PTR [rax + {}]", offset)); // C arg3 = integer value
        emitter.instruction("mov r8, 0");                                    // unused high value word
        emitter.instruction("mov r9, 0");                                    // runtime value tag = Int
        emitter.instruction("call __rt_hash_set");                           // insert the field
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                // retain a potentially reallocated hash
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                    // return the completed hash
    emitter.instruction("leave");                                            // release helper frame and restore rbp
    emitter.instruction("ret");
}

/// Emits the AArch64 stable-siginfo associative-array builder.
fn emit_pcntl_siginfo_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl signal information array ---");
    emitter.label_global("__rt_pcntl_siginfo_array");
    emitter.instruction("sub sp, sp, #32");                                  // reserve saved frame, record, and hash slots
    emitter.instruction("stp x29, x30, [sp, #16]");                          // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                 // establish the helper frame
    emitter.instruction("str x0, [sp]");                                     // preserve the stable siginfo pointer
    emitter.instruction("mov x0, #32");                                      // hash capacity
    emitter.instruction("mov x1, #7");                                       // hash value type = boxed Mixed
    emitter.instruction("bl __rt_hash_new");                                 // allocate the PHP associative array
    emitter.instruction("str x0, [sp, #8]");                                 // preserve the current hash pointer
    for (symbol, length, offset, bit) in SIGINFO_FIELDS {
        let skip = format!("__rt_pcntl_siginfo_skip_{bit}");
        emitter.instruction("ldr x9, [sp]");                                 // reload stable siginfo pointer
        emitter.instruction("ldr x10, [x9, #88]");                           // load the field-presence bitset
        emitter.instruction(&format!("tbz x10, #{bit}, {skip}"));             // omit fields not supplied by this signal/target
        emitter.instruction("ldr x0, [sp, #8]");                              // C arg0 = current hash
        abi::emit_symbol_address(emitter, "x1", symbol);                      // C arg1 = string key bytes
        emitter.instruction(&format!("mov x2, #{}", length));                 // C arg2 = key length
        emitter.instruction("ldr x9, [sp]");                                 // symbol loading may use scratch registers
        emitter.instruction(&format!("ldr x3, [x9, #{}]", offset));          // C arg3 = stable scalar value
        emitter.instruction("mov x4, #0");                                   // unused high value word
        if matches!(*bit, 6 | 7 | 8) {
            emitter.instruction("scvtf d0, x3");                              // PHP exposes clock ticks and addresses as float
            emitter.instruction("fmov x3, d0");                              // pass the double payload bits
            emitter.instruction("mov x5, #2");                               // runtime value tag = Float
        } else {
            emitter.instruction("mov x5, #0");                               // runtime value tag = Int
        }
        emitter.instruction("bl __rt_hash_set");                             // insert the present field
        emitter.instruction("str x0, [sp, #8]");                             // retain a potentially reallocated hash
        emitter.label(&skip);
    }
    emitter.instruction("ldr x0, [sp, #8]");                                 // return the completed hash
    emitter.instruction("ldp x29, x30, [sp, #16]");                          // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                  // release helper frame
    emitter.instruction("ret");
}

/// Emits the x86_64 SysV stable-siginfo associative-array builder.
fn emit_pcntl_siginfo_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl signal information array ---");
    emitter.label_global("__rt_pcntl_siginfo_array");
    emitter.instruction("push rbp");                                         // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                     // establish stable local offsets
    emitter.instruction("sub rsp, 16");                                      // reserve record and hash slots, keeping call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                     // preserve the stable siginfo pointer
    emitter.instruction("mov rdi, 32");                                      // hash capacity
    emitter.instruction("mov rsi, 7");                                       // hash value type = boxed Mixed
    emitter.instruction("call __rt_hash_new");                               // allocate the PHP associative array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                    // preserve the current hash pointer
    for (symbol, length, offset, bit) in SIGINFO_FIELDS {
        let skip = format!("__rt_pcntl_siginfo_skip_x_{bit}");
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                 // reload stable siginfo pointer
        emitter.instruction(&format!("test QWORD PTR [rax + 88], {}", 1_u64 << bit)); // test this field's presence bit
        emitter.instruction(&format!("jz {skip}"));                           // omit fields not supplied by this signal/target
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                // C arg0 = current hash
        abi::emit_symbol_address(emitter, "rsi", symbol);                     // C arg1 = string key bytes
        emitter.instruction(&format!("mov rdx, {}", length));                 // C arg2 = key length
        emitter.instruction(&format!("mov rcx, QWORD PTR [rax + {}]", offset)); // C arg3 = stable scalar value
        emitter.instruction("mov r8, 0");                                    // unused high value word
        if matches!(*bit, 6 | 7 | 8) {
            emitter.instruction("cvtsi2sd xmm0, rcx");                        // PHP exposes clock ticks and addresses as float
            emitter.instruction("movq rcx, xmm0");                           // pass the double payload bits
            emitter.instruction("mov r9, 2");                                // runtime value tag = Float
        } else {
            emitter.instruction("mov r9, 0");                                // runtime value tag = Int
        }
        emitter.instruction("call __rt_hash_set");                           // insert the present field
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                // retain a potentially reallocated hash
        emitter.label(&skip);
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                    // return the completed hash
    emitter.instruction("leave");                                            // release helper frame and restore rbp
    emitter.instruction("ret");
}

/// Emits the AArch64 callback adapter for `(int $signal, array $info)`.
fn emit_pcntl_invoke_descriptor_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl invoke signal descriptor ---");
    emitter.label_global("__rt_pcntl_invoke_descriptor");
    emitter.instruction("sub sp, sp, #80");
    emitter.instruction("stp x29, x30, [sp, #64]");
    emitter.instruction("add x29, sp, #64");
    emitter.instruction("stp x0, x1, [sp, #0]");
    emitter.instruction("str x2, [sp, #16]");
    emitter.instruction("mov x2, #0");
    emitter.instruction("mov x0, #0");
    emitter.instruction("bl __rt_mixed_from_value");
    emitter.instruction("str x0, [sp, #24]");
    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("bl __rt_pcntl_siginfo_array");
    emitter.instruction("str x0, [sp, #32]");
    emitter.instruction("mov x1, x0");
    emitter.instruction("mov x2, #0");
    emitter.instruction("mov x0, #5");
    emitter.instruction("bl __rt_mixed_from_value");
    emitter.instruction("str x0, [sp, #40]");
    emitter.instruction("ldr x0, [sp, #32]");
    emitter.instruction("bl __rt_decref_any");
    emitter.instruction("mov x0, #2");
    emitter.instruction("mov x1, #8");
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("ldr x1, [sp, #24]");
    emitter.instruction("bl __rt_array_push_refcounted");
    emitter.instruction("str x0, [sp, #32]");
    emitter.instruction("ldr x0, [sp, #24]");
    emitter.instruction("bl __rt_decref_any");
    emitter.instruction("ldr x0, [sp, #32]");
    emitter.instruction("ldr x1, [sp, #40]");
    emitter.instruction("bl __rt_array_push_refcounted");
    emitter.instruction("str x0, [sp, #32]");
    emitter.instruction("ldr x0, [sp, #40]");
    emitter.instruction("bl __rt_decref_any");
    emitter.instruction("ldr x1, [sp, #32]");
    emitter.instruction("mov x2, #0");
    emitter.instruction("mov x0, #4");
    emitter.instruction("bl __rt_mixed_from_value");
    emitter.instruction("str x0, [sp, #48]");
    emitter.instruction("ldr x0, [sp, #32]");
    emitter.instruction("bl __rt_decref_any");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("ldr x10, [x9, #56]");
    emitter.instruction("cbz x10, __rt_pcntl_invoke_cleanup");
    emitter.instruction("mov x0, x9");
    emitter.instruction("ldr x1, [sp, #48]");
    emitter.instruction("blr x10");
    emitter.instruction("cbz x0, __rt_pcntl_invoke_cleanup");
    emitter.instruction("bl __rt_decref_any");
    emitter.label("__rt_pcntl_invoke_cleanup");
    emitter.instruction("ldr x0, [sp, #48]");
    emitter.instruction("bl __rt_decref_any");
    emitter.instruction("ldp x29, x30, [sp, #64]");
    emitter.instruction("add sp, sp, #80");
    emitter.instruction("ret");
}

/// Emits the x86_64 callback adapter for `(int $signal, array $info)`.
fn emit_pcntl_invoke_descriptor_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl invoke signal descriptor ---");
    emitter.label_global("__rt_pcntl_invoke_descriptor");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");
    emitter.instruction("mov rdi, rsi");
    emitter.instruction("xor esi, esi");
    emitter.instruction("xor eax, eax");
    emitter.instruction("call __rt_mixed_from_value");
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_pcntl_siginfo_array");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("mov rdi, rax");
    emitter.instruction("xor esi, esi");
    emitter.instruction("mov eax, 5");
    emitter.instruction("call __rt_mixed_from_value");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction("call __rt_decref_any");
    emitter.instruction("mov edi, 2");
    emitter.instruction("mov esi, 8");
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");
    emitter.instruction("call __rt_array_push_refcounted");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");
    emitter.instruction("call __rt_decref_any");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");
    emitter.instruction("call __rt_array_push_refcounted");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");
    emitter.instruction("call __rt_decref_any");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");
    emitter.instruction("xor esi, esi");
    emitter.instruction("mov eax, 4");
    emitter.instruction("call __rt_mixed_from_value");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");
    emitter.instruction("call __rt_decref_any");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_pcntl_invoke_cleanup_x86");
    emitter.instruction("mov rdi, r10");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");
    emitter.instruction("call r11");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_pcntl_invoke_cleanup_x86");
    emitter.instruction("call __rt_decref_any");
    emitter.label("__rt_pcntl_invoke_cleanup_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");
    emitter.instruction("call __rt_decref_any");
    emitter.instruction("leave");
    emitter.instruction("ret");
}

/// Emits the AArch64 pending-record drain with a reentrancy guard.
fn emit_pcntl_dispatch_pending_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl dispatch pending signals ---");
    emitter.label_global("__rt_pcntl_dispatch_pending");
    emitter.instruction("sub sp, sp, #112");
    emitter.instruction("stp x29, x30, [sp, #96]");
    emitter.instruction("add x29, sp, #96");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("cbnz x10, __rt_pcntl_dispatch_done");
    emitter.instruction("mov x10, #1");
    emitter.instruction("str x10, [x9]");
    emitter.label("__rt_pcntl_dispatch_loop");
    emitter.instruction("mov x0, sp");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("ldr x9, [x9]");
    emitter.instruction("cbz x9, __rt_pcntl_dispatch_clear");
    emitter.instruction("blr x9");
    emitter.instruction("cmp x0, #1");
    emitter.instruction("b.ne __rt_pcntl_dispatch_clear");
    emitter.instruction("ldr x1, [sp]");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_handler_kind");
    emitter.instruction("ldr x10, [x9, x1, lsl #3]");
    emitter.instruction("cmp x10, #2");
    emitter.instruction("b.ne __rt_pcntl_dispatch_loop");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_handler_descriptor");
    emitter.instruction("ldr x0, [x9, x1, lsl #3]");
    emitter.instruction("cbz x0, __rt_pcntl_dispatch_loop");
    emitter.instruction("mov x2, sp");
    emitter.instruction("bl __rt_pcntl_invoke_descriptor");
    emitter.instruction("b __rt_pcntl_dispatch_loop");
    emitter.label("__rt_pcntl_dispatch_clear");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("str xzr, [x9]");
    emitter.label("__rt_pcntl_dispatch_done");
    emitter.instruction("mov x0, #1");
    emitter.instruction("ldp x29, x30, [sp, #96]");
    emitter.instruction("add sp, sp, #112");
    emitter.instruction("ret");
}

/// Emits the x86_64 pending-record drain with a reentrancy guard.
fn emit_pcntl_dispatch_pending_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl dispatch pending signals ---");
    emitter.label_global("__rt_pcntl_dispatch_pending");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 96");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("cmp QWORD PTR [r9], 0");
    emitter.instruction("jne __rt_pcntl_dispatch_done_x86");
    emitter.instruction("mov QWORD PTR [r9], 1");
    emitter.label("__rt_pcntl_dispatch_loop_x86");
    emitter.instruction("mov rdi, rsp");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_pcntl_dispatch_clear_x86");
    emitter.instruction("call r9");
    emitter.instruction("cmp rax, 1");
    emitter.instruction("jne __rt_pcntl_dispatch_clear_x86");
    emitter.instruction("mov rsi, QWORD PTR [rsp]");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_handler_kind");
    emitter.instruction("cmp QWORD PTR [r9 + rsi*8], 2");
    emitter.instruction("jne __rt_pcntl_dispatch_loop_x86");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_handler_descriptor");
    emitter.instruction("mov rdi, QWORD PTR [r9 + rsi*8]");
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_pcntl_dispatch_loop_x86");
    emitter.instruction("mov rdx, rsp");
    emitter.instruction("call __rt_pcntl_invoke_descriptor");
    emitter.instruction("jmp __rt_pcntl_dispatch_loop_x86");
    emitter.label("__rt_pcntl_dispatch_clear_x86");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("mov QWORD PTR [r9], 0");
    emitter.label("__rt_pcntl_dispatch_done_x86");
    emitter.instruction("mov eax, 1");
    emitter.instruction("leave");
    emitter.instruction("ret");
}

/// Emits an AArch64 safe-point wrapper that preserves every allocatable register and FP state.
fn emit_pcntl_async_dispatch_preserving_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl async dispatch preserving registers ---");
    emitter.label_global("__rt_pcntl_async_dispatch_preserving");
    emitter.instruction("sub sp, sp, #800");
    for register in (0..32).step_by(2) {
        emitter.instruction(&format!(
            "stp q{register}, q{}, [sp, #{}]",
            register + 1,
            register * 16
        ));
    }
    emitter.instruction("str x28, [sp, #736]");
    emitter.instruction("str x29, [sp, #768]");
    emitter.instruction("str x30, [sp, #776]");
    emitter.instruction("add x28, sp, #512");
    for register in (0..28).step_by(2) {
        emitter.instruction(&format!(
            "stp x{register}, x{}, [x28, #{}]",
            register + 1,
            register * 8
        ));
    }
    emitter.instruction("mrs x9, nzcv");
    emitter.instruction("str x9, [sp, #744]");
    emitter.instruction("mrs x9, fpcr");
    emitter.instruction("str x9, [sp, #752]");
    emitter.instruction("mrs x9, fpsr");
    emitter.instruction("str x9, [sp, #760]");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_async_enabled");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("cbz x10, __rt_pcntl_async_restore");
    emitter.instruction("bl __rt_pcntl_dispatch_pending");
    emitter.label("__rt_pcntl_async_restore");
    emitter.instruction("ldr x9, [sp, #744]");
    emitter.instruction("msr nzcv, x9");
    emitter.instruction("ldr x9, [sp, #752]");
    emitter.instruction("msr fpcr, x9");
    emitter.instruction("ldr x9, [sp, #760]");
    emitter.instruction("msr fpsr, x9");
    emitter.instruction("add x28, sp, #512");
    for register in (0..28).step_by(2).rev() {
        emitter.instruction(&format!(
            "ldp x{register}, x{}, [x28, #{}]",
            register + 1,
            register * 8
        ));
    }
    emitter.instruction("ldr x29, [sp, #768]");
    emitter.instruction("ldr x30, [sp, #776]");
    for register in (0..32).step_by(2).rev() {
        emitter.instruction(&format!(
            "ldp q{register}, q{}, [sp, #{}]",
            register + 1,
            register * 16
        ));
    }
    emitter.instruction("ldr x28, [sp, #736]");
    emitter.instruction("add sp, sp, #800");
    emitter.instruction("ret");
}

/// Emits an x86_64 safe-point wrapper that preserves every general/vector register and flags.
fn emit_pcntl_async_dispatch_preserving_x86_64(emitter: &mut Emitter) {
    const REGISTERS: &[&str] = &[
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "r8", "r9", "r10", "r11",
        "r12", "r13", "r14", "r15",
    ];
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl async dispatch preserving registers ---");
    emitter.label_global("__rt_pcntl_async_dispatch_preserving");
    emitter.instruction("pushfq");
    for register in REGISTERS {
        emitter.instruction(&format!("push {register}"));
    }
    emitter.instruction("sub rsp, 264");
    for register in 0..16 {
        emitter.instruction(&format!("movdqu XMMWORD PTR [rsp + {}], xmm{register}", register * 16));
    }
    emitter.instruction("stmxcsr DWORD PTR [rsp + 256]");
    emitter.instruction("fnstcw WORD PTR [rsp + 260]");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_async_enabled");
    emitter.instruction("cmp QWORD PTR [r9], 0");
    emitter.instruction("je __rt_pcntl_async_restore_x86");
    emitter.instruction("call __rt_pcntl_dispatch_pending");
    emitter.label("__rt_pcntl_async_restore_x86");
    emitter.instruction("ldmxcsr DWORD PTR [rsp + 256]");
    emitter.instruction("fldcw WORD PTR [rsp + 260]");
    for register in (0..16).rev() {
        emitter.instruction(&format!("movdqu xmm{register}, XMMWORD PTR [rsp + {}]", register * 16));
    }
    emitter.instruction("add rsp, 264");
    for register in REGISTERS.iter().rev() {
        emitter.instruction(&format!("pop {register}"));
    }
    emitter.instruction("popfq");
    emitter.instruction("ret");
}

/// Emits AArch64 teardown for registered descriptors, OS dispositions, and queued records.
fn emit_pcntl_release_handlers_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl release signal handlers ---");
    emitter.label_global("__rt_pcntl_release_handlers");
    emitter.instruction("sub sp, sp, #128");
    emitter.instruction("stp x29, x30, [sp, #112]");
    emitter.instruction("add x29, sp, #112");
    emitter.instruction("mov x9, #1");
    emitter.instruction("str x9, [sp, #96]");
    emitter.label("__rt_pcntl_release_handlers_loop");
    emitter.instruction("ldr x9, [sp, #96]");
    emitter.instruction("cmp x9, #128");
    emitter.instruction("b.hs __rt_pcntl_release_handlers_state");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_kind");
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");
    emitter.instruction("cbz x11, __rt_pcntl_release_handlers_next");
    emitter.instruction("mov x0, x9");
    emitter.instruction("mov x1, #0");
    emitter.instruction("mov x2, #1");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_signal_fn");
    emitter.instruction("ldr x10, [x10]");
    emitter.instruction("cbz x10, __rt_pcntl_release_handlers_zero");
    emitter.instruction("blr x10");
    emitter.instruction("ldr x9, [sp, #96]");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_kind");
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");
    emitter.instruction("cmp x11, #2");
    emitter.instruction("b.ne __rt_pcntl_release_handlers_zero");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");
    emitter.instruction("bl __rt_callable_descriptor_release");
    emitter.instruction("ldr x9, [sp, #96]");
    emitter.label("__rt_pcntl_release_handlers_zero");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_kind");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");
    emitter.label("__rt_pcntl_release_handlers_next");
    emitter.instruction("ldr x9, [sp, #96]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #96]");
    emitter.instruction("b __rt_pcntl_release_handlers_loop");
    emitter.label("__rt_pcntl_release_handlers_state");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_async_enabled");
    emitter.instruction("str xzr, [x9]");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("str xzr, [x9]");
    emitter.label("__rt_pcntl_release_handlers_drain");
    emitter.instruction("mov x0, sp");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("ldr x9, [x9]");
    emitter.instruction("cbz x9, __rt_pcntl_release_handlers_done");
    emitter.instruction("blr x9");
    emitter.instruction("cmp x0, #1");
    emitter.instruction("b.eq __rt_pcntl_release_handlers_drain");
    emitter.label("__rt_pcntl_release_handlers_done");
    emitter.instruction("ldp x29, x30, [sp, #112]");
    emitter.instruction("add sp, sp, #128");
    emitter.instruction("ret");
}

/// Emits x86_64 teardown for registered descriptors, OS dispositions, and queued records.
fn emit_pcntl_release_handlers_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl release signal handlers ---");
    emitter.label_global("__rt_pcntl_release_handlers");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 112");
    emitter.instruction("mov QWORD PTR [rbp - 104], 1");
    emitter.label("__rt_pcntl_release_handlers_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");
    emitter.instruction("cmp r9, 128");
    emitter.instruction("jae __rt_pcntl_release_handlers_state_x86");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_kind");
    emitter.instruction("cmp QWORD PTR [r10 + r9*8], 0");
    emitter.instruction("je __rt_pcntl_release_handlers_next_x86");
    emitter.instruction("mov rdi, r9");
    emitter.instruction("xor esi, esi");
    emitter.instruction("mov edx, 1");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_signal_fn");
    emitter.instruction("mov r10, QWORD PTR [r10]");
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_pcntl_release_handlers_zero_x86");
    emitter.instruction("call r10");
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_kind");
    emitter.instruction("cmp QWORD PTR [r10 + r9*8], 2");
    emitter.instruction("jne __rt_pcntl_release_handlers_zero_x86");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");
    emitter.instruction("call __rt_callable_descriptor_release");
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");
    emitter.label("__rt_pcntl_release_handlers_zero_x86");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_kind");
    emitter.instruction("mov QWORD PTR [r10 + r9*8], 0");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("mov QWORD PTR [r10 + r9*8], 0");
    emitter.label("__rt_pcntl_release_handlers_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 104], 1");
    emitter.instruction("jmp __rt_pcntl_release_handlers_loop_x86");
    emitter.label("__rt_pcntl_release_handlers_state_x86");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_async_enabled");
    emitter.instruction("mov QWORD PTR [r9], 0");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("mov QWORD PTR [r9], 0");
    emitter.label("__rt_pcntl_release_handlers_drain_x86");
    emitter.instruction("lea rdi, [rbp - 96]");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_pcntl_release_handlers_done_x86");
    emitter.instruction("call r9");
    emitter.instruction("cmp rax, 1");
    emitter.instruction("je __rt_pcntl_release_handlers_drain_x86");
    emitter.label("__rt_pcntl_release_handlers_done_x86");
    emitter.instruction("leave");
    emitter.instruction("ret");
}
