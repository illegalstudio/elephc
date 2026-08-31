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
    ("_pcntl_siginfo_utime", 5, 48, 6),
    ("_pcntl_siginfo_stime", 5, 56, 7),
    ("_pcntl_siginfo_pid", 3, 32, 4),
    ("_pcntl_siginfo_uid", 3, 40, 5),
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
            emit_pcntl_abort_dispatch_aarch64(emitter);
            emit_pcntl_async_dispatch_preserving_aarch64(emitter);
            emit_pcntl_release_handlers_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_pcntl_invoke_descriptor_x86_64(emitter);
            emit_pcntl_dispatch_pending_x86_64(emitter);
            emit_pcntl_abort_dispatch_x86_64(emitter);
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
    emitter.instruction("sub sp, sp, #32");                                     // reserve saved frame, record, and hash slots
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame
    emitter.instruction("str x0, [sp]");                                        // preserve the stable usage-record pointer
    emitter.instruction("mov x0, #32");                                         // hash capacity
    emitter.instruction("mov x1, #0");                                          // hash value type = Int
    emitter.instruction("bl __rt_hash_new");                                    // allocate the PHP associative array
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the current hash pointer
    for (symbol, length, offset) in RUSAGE_FIELDS {
        emitter.instruction("ldr x0, [sp, #8]");                                // C arg0 = current hash
        abi::emit_symbol_address(emitter, "x1", symbol);                        // C arg1 = string key bytes
        emitter.instruction(&format!("mov x2, #{}", length));                   // C arg2 = key length
        emitter.instruction("ldr x9, [sp]");                                    // reload usage-record pointer
        emitter.instruction(&format!("ldr x3, [x9, #{}]", offset));             // C arg3 = integer value
        emitter.instruction("mov x4, #0");                                      // unused high value word
        emitter.instruction("mov x5, #0");                                      // runtime value tag = Int
        emitter.instruction("bl __rt_hash_set");                                // insert the field
        emitter.instruction("str x0, [sp, #8]");                                // retain a potentially reallocated hash
    }
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the completed hash
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release helper frame
    emitter.instruction("ret");                                                 // return the completed resource-usage array
}

/// Emits the x86_64 SysV resource-usage array builder.
fn emit_pcntl_rusage_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl resource usage array ---");
    emitter.label_global("__rt_pcntl_rusage_array");
    emitter.instruction("push rbp");                                            // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable local offsets
    emitter.instruction("sub rsp, 16");                                         // reserve record and hash slots, keeping call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the stable usage-record pointer
    emitter.instruction("mov rdi, 32");                                         // hash capacity
    emitter.instruction("mov rsi, 0");                                          // hash value type = Int
    emitter.instruction("call __rt_hash_new");                                  // allocate the PHP associative array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the current hash pointer
    for (symbol, length, offset) in RUSAGE_FIELDS {
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // C arg0 = current hash
        abi::emit_symbol_address(emitter, "rsi", symbol);                       // C arg1 = string key bytes
        emitter.instruction(&format!("mov rdx, {}", length));                   // C arg2 = key length
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                    // reload usage-record pointer
        emitter.instruction(&format!("mov rcx, QWORD PTR [rax + {}]", offset)); // C arg3 = integer value
        emitter.instruction("mov r8, 0");                                       // unused high value word
        emitter.instruction("mov r9, 0");                                       // runtime value tag = Int
        emitter.instruction("call __rt_hash_set");                              // insert the field
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                   // retain a potentially reallocated hash
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the completed hash
    emitter.instruction("leave");                                               // release helper frame and restore rbp
    emitter.instruction("ret");                                                 // return the completed resource-usage array
}

/// Emits the AArch64 stable-siginfo associative-array builder.
fn emit_pcntl_siginfo_array_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl signal information array ---");
    emitter.label_global("__rt_pcntl_siginfo_array");
    emitter.instruction("sub sp, sp, #32");                                     // reserve saved frame, record, and hash slots
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame
    emitter.instruction("str x0, [sp]");                                        // preserve the stable siginfo pointer
    emitter.instruction("mov x0, #32");                                         // hash capacity
    emitter.instruction("mov x1, #7");                                          // hash value type = boxed Mixed
    emitter.instruction("bl __rt_hash_new");                                    // allocate the PHP associative array
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the current hash pointer
    for (symbol, length, offset, bit) in SIGINFO_FIELDS {
        let skip = format!("__rt_pcntl_siginfo_skip_{bit}");
        emitter.instruction("ldr x9, [sp]");                                    // reload stable siginfo pointer
        emitter.instruction("ldr x10, [x9, #88]");                              // load the field-presence bitset
        emitter.instruction(&format!("tbz x10, #{bit}, {skip}"));               // omit fields not supplied by this signal/target
        emitter.instruction("ldr x0, [sp, #8]");                                // C arg0 = current hash
        abi::emit_symbol_address(emitter, "x1", symbol);                        // C arg1 = string key bytes
        emitter.instruction(&format!("mov x2, #{}", length));                   // C arg2 = key length
        emitter.instruction("ldr x9, [sp]");                                    // symbol loading may use scratch registers
        emitter.instruction(&format!("ldr x3, [x9, #{}]", offset));             // C arg3 = stable scalar value
        emitter.instruction("mov x4, #0");                                      // unused high value word
        if matches!(*bit, 6 | 7 | 8) {
            emitter.instruction("scvtf d0, x3");                                // PHP exposes clock ticks and addresses as float
            emitter.instruction("fmov x3, d0");                                 // pass the double payload bits
            emitter.instruction("mov x5, #2");                                  // runtime value tag = Float
        } else {
            emitter.instruction("mov x5, #0");                                  // runtime value tag = Int
        }
        emitter.instruction("bl __rt_hash_set");                                // insert the present field
        emitter.instruction("str x0, [sp, #8]");                                // retain a potentially reallocated hash
        emitter.label(&skip);
    }
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the completed hash
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release helper frame
    emitter.instruction("ret");                                                 // return the completed signal-information array
}

/// Emits the x86_64 SysV stable-siginfo associative-array builder.
fn emit_pcntl_siginfo_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl signal information array ---");
    emitter.label_global("__rt_pcntl_siginfo_array");
    emitter.instruction("push rbp");                                            // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable local offsets
    emitter.instruction("sub rsp, 16");                                         // reserve record and hash slots, keeping call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the stable siginfo pointer
    emitter.instruction("mov rdi, 32");                                         // hash capacity
    emitter.instruction("mov rsi, 7");                                          // hash value type = boxed Mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the PHP associative array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the current hash pointer
    for (symbol, length, offset, bit) in SIGINFO_FIELDS {
        let skip = format!("__rt_pcntl_siginfo_skip_x_{bit}");
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                    // reload stable siginfo pointer
        emitter.instruction(&format!("test QWORD PTR [rax + 88], {}", 1_u64 << bit)); // test this field's presence bit
        emitter.instruction(&format!("jz {skip}"));                             // omit fields not supplied by this signal/target
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // C arg0 = current hash
        abi::emit_symbol_address(emitter, "rsi", symbol);                       // C arg1 = string key bytes
        emitter.instruction(&format!("mov rdx, {}", length));                   // C arg2 = key length
        emitter.instruction(&format!("mov rcx, QWORD PTR [rax + {}]", offset)); // C arg3 = stable scalar value
        emitter.instruction("mov r8, 0");                                       // unused high value word
        if matches!(*bit, 6 | 7 | 8) {
            emitter.instruction("cvtsi2sd xmm0, rcx");                          // PHP exposes clock ticks and addresses as float
            emitter.instruction("movq rcx, xmm0");                              // pass the double payload bits
            emitter.instruction("mov r9, 2");                                   // runtime value tag = Float
        } else {
            emitter.instruction("mov r9, 0");                                   // runtime value tag = Int
        }
        emitter.instruction("call __rt_hash_set");                              // insert the present field
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                   // retain a potentially reallocated hash
        emitter.label(&skip);
    }
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the completed hash
    emitter.instruction("leave");                                               // release helper frame and restore rbp
    emitter.instruction("ret");                                                 // return the completed signal-information array
}

/// Emits the AArch64 callback adapter for `(int $signal, array $info)`.
fn emit_pcntl_invoke_descriptor_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl invoke signal descriptor ---");
    emitter.label_global("__rt_pcntl_invoke_descriptor");
    emitter.instruction("sub sp, sp, #80");                                     // reserve descriptor, arguments, temporaries, and saved frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the callback adapter frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // preserve descriptor and signal number
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the stable siginfo record pointer
    emitter.instruction("mov x2, #0");                                          // clear the unused high payload word
    emitter.instruction("mov x0, #0");                                          // Mixed source tag = Int
    emitter.instruction("bl __rt_mixed_from_value");                            // box the signal number
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the boxed signal owner
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the stable siginfo record
    emitter.instruction("bl __rt_pcntl_siginfo_array");                         // materialize the PHP siginfo array
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the raw siginfo array owner
    emitter.instruction("mov x1, x0");                                          // Mixed payload = siginfo array pointer
    emitter.instruction("mov x2, #0");                                          // clear the unused high payload word
    emitter.instruction("mov x0, #5");                                          // Mixed source tag = associative array
    emitter.instruction("bl __rt_mixed_from_value");                            // box the siginfo array
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the boxed siginfo owner
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the raw siginfo array owner
    emitter.instruction("bl __rt_decref_any");                                  // transfer ownership to its Mixed box
    emitter.instruction("mov x0, #2");                                          // allocate two callback arguments
    emitter.instruction("mov x1, #8");                                          // argument array element type = refcounted Mixed
    emitter.instruction("bl __rt_array_new");                                   // allocate the callback argument array
    emitter.instruction("ldr x1, [sp, #24]");                                   // append the boxed signal
    emitter.instruction("bl __rt_array_push_refcounted");                       // retain signal in the argument array
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the possibly relocated array
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the local boxed-signal owner
    emitter.instruction("bl __rt_decref_any");                                  // leave the array as its sole owner
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the callback argument array
    emitter.instruction("ldr x1, [sp, #40]");                                   // append the boxed siginfo value
    emitter.instruction("bl __rt_array_push_refcounted");                       // retain siginfo in the argument array
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the possibly relocated array
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the local boxed-siginfo owner
    emitter.instruction("bl __rt_decref_any");                                  // leave the array as its sole owner
    emitter.instruction("ldr x1, [sp, #32]");                                   // Mixed payload = callback argument array
    emitter.instruction("mov x2, #0");                                          // clear the unused high payload word
    emitter.instruction("mov x0, #4");                                          // Mixed source tag = indexed array
    emitter.instruction("bl __rt_mixed_from_value");                            // box the callback argument array
    emitter.instruction("str x0, [sp, #48]");                                   // preserve the boxed argument owner
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the raw argument array owner
    emitter.instruction("bl __rt_decref_any");                                  // transfer ownership to its Mixed box
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the callable descriptor
    emitter.instruction("ldr x10, [x9, #56]");                                  // load its uniform invocation entry point
    emitter.instruction("cbz x10, __rt_pcntl_invoke_cleanup");                  // tolerate descriptors without an invoker
    emitter.instruction("mov x0, x9");                                          // invocation arg0 = callable descriptor
    emitter.instruction("ldr x1, [sp, #48]");                                   // invocation arg1 = boxed arguments
    emitter.instruction("blr x10");                                             // invoke the PHP signal handler
    emitter.instruction("cbz x0, __rt_pcntl_invoke_cleanup");                   // a void/null result needs no release
    emitter.instruction("bl __rt_decref_any");                                  // release the ignored callback result
    emitter.label("__rt_pcntl_invoke_cleanup");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the boxed argument owner
    emitter.instruction("bl __rt_decref_any");                                  // release both callback arguments deeply
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release callback adapter storage
    emitter.instruction("ret");                                                 // resume the masked dispatcher
}

/// Emits the x86_64 callback adapter for `(int $signal, array $info)`.
fn emit_pcntl_invoke_descriptor_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl invoke signal descriptor ---");
    emitter.label_global("__rt_pcntl_invoke_descriptor");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable callback adapter offsets
    emitter.instruction("sub rsp, 64");                                         // reserve arguments and temporary owners
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the callable descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the signal number
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the stable siginfo record pointer
    emitter.instruction("mov rdi, rsi");                                        // Mixed payload = signal number
    emitter.instruction("xor esi, esi");                                        // clear the unused high payload word
    emitter.instruction("xor eax, eax");                                        // Mixed source tag = Int
    emitter.instruction("call __rt_mixed_from_value");                          // box the signal number
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the boxed signal owner
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the stable siginfo record
    emitter.instruction("call __rt_pcntl_siginfo_array");                       // materialize the PHP siginfo array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the raw siginfo array owner
    emitter.instruction("mov rdi, rax");                                        // Mixed payload = siginfo array pointer
    emitter.instruction("xor esi, esi");                                        // clear the unused high payload word
    emitter.instruction("mov eax, 5");                                          // Mixed source tag = associative array
    emitter.instruction("call __rt_mixed_from_value");                          // box the siginfo array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the boxed siginfo owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the raw siginfo array owner
    emitter.instruction("call __rt_decref_any");                                // transfer ownership to its Mixed box
    emitter.instruction("mov edi, 2");                                          // allocate two callback arguments
    emitter.instruction("mov esi, 8");                                          // argument array element type = refcounted Mixed
    emitter.instruction("call __rt_array_new");                                 // allocate the callback argument array
    emitter.instruction("mov rdi, rax");                                        // mutation arg0 = argument array
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // mutation arg1 = boxed signal
    emitter.instruction("call __rt_array_push_refcounted");                     // retain signal in the argument array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the possibly relocated array
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the local boxed-signal owner
    emitter.instruction("call __rt_decref_any");                                // leave the array as its sole owner
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the callback argument array
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // mutation arg1 = boxed siginfo
    emitter.instruction("call __rt_array_push_refcounted");                     // retain siginfo in the argument array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the possibly relocated array
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the local boxed-siginfo owner
    emitter.instruction("call __rt_decref_any");                                // leave the array as its sole owner
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // Mixed payload = callback argument array
    emitter.instruction("xor esi, esi");                                        // clear the unused high payload word
    emitter.instruction("mov eax, 4");                                          // Mixed source tag = indexed array
    emitter.instruction("call __rt_mixed_from_value");                          // box the callback argument array
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the boxed argument owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the raw argument array owner
    emitter.instruction("call __rt_decref_any");                                // transfer ownership to its Mixed box
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the callable descriptor
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");                       // load its uniform invocation entry point
    emitter.instruction("test r11, r11");                                       // does the descriptor provide an invoker?
    emitter.instruction("jz __rt_pcntl_invoke_cleanup_x86");                    // tolerate an absent invoker defensively
    emitter.instruction("mov rdi, r10");                                        // invocation arg0 = callable descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // invocation arg1 = boxed arguments
    emitter.instruction("call r11");                                            // invoke the PHP signal handler
    emitter.instruction("test rax, rax");                                       // did the handler return an owned value?
    emitter.instruction("jz __rt_pcntl_invoke_cleanup_x86");                    // a void/null result needs no release
    emitter.instruction("call __rt_decref_any");                                // release the ignored callback result
    emitter.label("__rt_pcntl_invoke_cleanup_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the boxed argument owner
    emitter.instruction("call __rt_decref_any");                                // release both callback arguments deeply
    emitter.instruction("leave");                                               // release adapter storage and restore rbp
    emitter.instruction("ret");                                                 // resume the masked dispatcher
}

/// Emits the AArch64 pending-record drain with a reentrancy guard.
fn emit_pcntl_dispatch_pending_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl dispatch pending signals ---");
    emitter.label_global("__rt_pcntl_dispatch_pending");
    emitter.instruction("sub sp, sp, #144");                                    // reserve siginfo, result, and saved frame storage
    emitter.instruction("stp x29, x30, [sp, #128]");                            // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #128");                                   // establish the dispatcher frame
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("ldr x10, [x9]");                                       // load the process-wide reentrancy guard
    emitter.instruction("cbnz x10, __rt_pcntl_dispatch_reentrant");             // nested dispatch is a successful no-op
    emitter.instruction("mov x10, #1");                                         // mark callback dispatch active
    emitter.instruction("str x10, [x9]");                                       // publish the guard before masking signals
    emitter.instruction("str x10, [sp, #96]");                                  // default the returned success value to true
    abi::emit_symbol_address(emitter, "x0", "__rt_pcntl_dispatch_mask");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatch_begin_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the bridge mask-begin callback
    emitter.instruction("cbz x9, __rt_pcntl_dispatch_failed");                  // an uninitialized bridge cannot dispatch safely
    emitter.instruction("blr x9");                                              // block signals and save the prior thread mask
    emitter.instruction("cbz x0, __rt_pcntl_dispatch_failed");                  // propagate bridge mask failures as false
    emitter.label("__rt_pcntl_dispatch_loop");
    emitter.instruction("mov x0, sp");                                          // C arg0 = stable siginfo output storage
    emitter.instruction("mov x1, #1");                                          // C arg1 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the bridge queue-pop callback
    emitter.instruction("cbz x9, __rt_pcntl_dispatch_failed_masked");           // missing callback is a dispatch failure
    emitter.instruction("blr x9");                                              // pop one record while delivery remains blocked
    emitter.instruction("cmp x0, #0");                                          // zero means the masked snapshot is exhausted
    emitter.instruction("b.eq __rt_pcntl_dispatch_finish");                     // finish after consuming the masked snapshot
    emitter.instruction("b.lt __rt_pcntl_dispatch_failed_masked");              // a bridge read error returns false
    emitter.instruction("ldr x1, [sp]");                                        // x1 = signal number from the stable record
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_handler_kind");
    emitter.instruction("ldr x10, [x9, x1, lsl #3]");                           // load the registered handler kind
    emitter.instruction("cmp x10, #2");                                         // kind two owns a callable descriptor
    emitter.instruction("b.ne __rt_pcntl_dispatch_loop");                       // ignored/default records need no PHP call
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_handler_descriptor");
    emitter.instruction("ldr x0, [x9, x1, lsl #3]");                            // x0 = retained callable descriptor
    emitter.instruction("cbz x0, __rt_pcntl_dispatch_loop");                    // tolerate an empty descriptor slot defensively
    emitter.instruction("mov x2, sp");                                          // x2 = stable siginfo record
    emitter.instruction("bl __rt_pcntl_invoke_descriptor");                     // invoke handler(signal, info)
    emitter.instruction("b __rt_pcntl_dispatch_loop");                          // continue only through the original masked snapshot
    emitter.label("__rt_pcntl_dispatch_failed_masked");
    emitter.instruction("str xzr, [sp, #96]");                                  // remember the queue-pop failure
    emitter.label("__rt_pcntl_dispatch_finish");
    abi::emit_symbol_address(emitter, "x0", "__rt_pcntl_dispatch_mask");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatch_end_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the bridge mask-restore callback
    emitter.instruction("cbz x9, __rt_pcntl_dispatch_clear");                   // preserve the current failure result when unavailable
    emitter.instruction("blr x9");                                              // restore the pre-dispatch signal mask
    emitter.instruction("cbnz x0, __rt_pcntl_dispatch_clear");                  // a successful restore keeps the prior result
    emitter.instruction("str xzr, [sp, #96]");                                  // mask restoration failure makes dispatch false
    emitter.instruction("b __rt_pcntl_dispatch_clear");                         // clear the guard after restoring the mask
    emitter.label("__rt_pcntl_dispatch_failed");
    emitter.instruction("str xzr, [sp, #96]");                                  // record failure before clearing the guard
    emitter.label("__rt_pcntl_dispatch_clear");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("str xzr, [x9]");                                       // leave the non-reentrant dispatch region
    emitter.instruction("ldr x0, [sp, #96]");                                   // return the accumulated boolean result
    emitter.instruction("b __rt_pcntl_dispatch_done");                          // join the common dispatcher epilogue
    emitter.label("__rt_pcntl_dispatch_reentrant");
    emitter.instruction("mov x0, #1");                                          // nested dispatch reports success without consuming records
    emitter.label("__rt_pcntl_dispatch_done");
    emitter.instruction("ldp x29, x30, [sp, #128]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #144");                                    // release dispatcher storage
    emitter.instruction("ret");                                                 // return the boolean result
}

/// Emits the x86_64 pending-record drain with a reentrancy guard.
fn emit_pcntl_dispatch_pending_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl dispatch pending signals ---");
    emitter.label_global("__rt_pcntl_dispatch_pending");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable dispatcher offsets
    emitter.instruction("sub rsp, 112");                                        // reserve siginfo plus result storage with SysV alignment
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("cmp QWORD PTR [r9], 0");                               // test the process-wide reentrancy guard
    emitter.instruction("jne __rt_pcntl_dispatch_reentrant_x86");               // nested dispatch is a successful no-op
    emitter.instruction("mov QWORD PTR [r9], 1");                               // publish active dispatch state
    emitter.instruction("mov QWORD PTR [rsp + 96], 1");                         // default the returned success value to true
    abi::emit_symbol_address(emitter, "rdi", "__rt_pcntl_dispatch_mask");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatch_begin_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the bridge mask-begin callback
    emitter.instruction("test r9, r9");                                         // is the PCNTL bridge initialized?
    emitter.instruction("jz __rt_pcntl_dispatch_failed_x86");                   // an uninitialized bridge cannot dispatch safely
    emitter.instruction("call r9");                                             // block signals and save the prior mask
    emitter.instruction("test eax, eax");                                       // did the bridge save the mask?
    emitter.instruction("jz __rt_pcntl_dispatch_failed_x86");                   // propagate bridge mask failures as false
    emitter.label("__rt_pcntl_dispatch_loop_x86");
    emitter.instruction("mov rdi, rsp");                                        // C arg0 = stable siginfo output storage
    emitter.instruction("mov esi, 1");                                          // C arg1 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the bridge queue-pop callback
    emitter.instruction("test r9, r9");                                         // reject an uninitialized bridge slot
    emitter.instruction("jz __rt_pcntl_dispatch_failed_masked_x86");            // missing callback is a dispatch failure
    emitter.instruction("call r9");                                             // pop one record while delivery remains blocked
    emitter.instruction("test rax, rax");                                       // zero means the snapshot is exhausted
    emitter.instruction("jz __rt_pcntl_dispatch_finish_x86");                   // finish after consuming the masked snapshot
    emitter.instruction("js __rt_pcntl_dispatch_failed_masked_x86");            // a bridge read error returns false
    emitter.instruction("mov rsi, QWORD PTR [rsp]");                            // rsi = signal number from the stable record
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_handler_kind");
    emitter.instruction("cmp QWORD PTR [r9 + rsi*8], 2");                       // kind two owns a callable descriptor
    emitter.instruction("jne __rt_pcntl_dispatch_loop_x86");                    // ignored/default records need no PHP call
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_handler_descriptor");
    emitter.instruction("mov rdi, QWORD PTR [r9 + rsi*8]");                     // rdi = retained callable descriptor
    emitter.instruction("test rdi, rdi");                                       // tolerate an empty descriptor slot defensively
    emitter.instruction("jz __rt_pcntl_dispatch_loop_x86");                     // tolerate an empty descriptor slot defensively
    emitter.instruction("mov rdx, rsp");                                        // rdx = stable siginfo record
    emitter.instruction("call __rt_pcntl_invoke_descriptor");                   // invoke handler(signal, info)
    emitter.instruction("jmp __rt_pcntl_dispatch_loop_x86");                    // continue only through the masked snapshot
    emitter.label("__rt_pcntl_dispatch_failed_masked_x86");
    emitter.instruction("mov QWORD PTR [rsp + 96], 0");                         // remember the queue-pop failure
    emitter.label("__rt_pcntl_dispatch_finish_x86");
    abi::emit_symbol_address(emitter, "rdi", "__rt_pcntl_dispatch_mask");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatch_end_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the bridge mask-restore callback
    emitter.instruction("test r9, r9");                                         // preserve failure when the callback is absent
    emitter.instruction("jz __rt_pcntl_dispatch_clear_x86");                    // preserve failure when the callback is absent
    emitter.instruction("call r9");                                             // restore the pre-dispatch signal mask
    emitter.instruction("test eax, eax");                                       // did restoration succeed?
    emitter.instruction("jnz __rt_pcntl_dispatch_clear_x86");                   // a successful restore keeps the prior result
    emitter.instruction("mov QWORD PTR [rsp + 96], 0");                         // mask restoration failure makes dispatch false
    emitter.instruction("jmp __rt_pcntl_dispatch_clear_x86");                   // clear the guard after restoring the mask
    emitter.label("__rt_pcntl_dispatch_failed_x86");
    emitter.instruction("mov QWORD PTR [rsp + 96], 0");                         // record failure before clearing the guard
    emitter.label("__rt_pcntl_dispatch_clear_x86");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // leave the non-reentrant dispatch region
    emitter.instruction("mov rax, QWORD PTR [rsp + 96]");                       // return the accumulated boolean result
    emitter.instruction("jmp __rt_pcntl_dispatch_done_x86");                    // join the common dispatcher epilogue
    emitter.label("__rt_pcntl_dispatch_reentrant_x86");
    emitter.instruction("mov eax, 1");                                          // nested dispatch reports success without consuming records
    emitter.label("__rt_pcntl_dispatch_done_x86");
    emitter.instruction("leave");                                               // release dispatcher storage and restore rbp
    emitter.instruction("ret");                                                 // return the boolean result
}

/// Emits exception-unwind cleanup for an active AArch64 signal dispatch.
fn emit_pcntl_abort_dispatch_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl abort signal dispatch ---");
    emitter.label_global("__rt_pcntl_abort_dispatch");
    emitter.instruction("sub sp, sp, #112");                                    // reserve one siginfo record and saved frame state
    emitter.instruction("stp x29, x30, [sp, #96]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the cleanup frame
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("ldr x10, [x9]");                                       // test whether an exception escaped a signal handler
    emitter.instruction("cbz x10, __rt_pcntl_abort_dispatch_done");             // ordinary throws require no PCNTL cleanup
    emitter.label("__rt_pcntl_abort_dispatch_drain");
    emitter.instruction("mov x0, sp");                                          // C arg0 = discard record storage
    emitter.instruction("mov x1, #1");                                          // C arg1 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the bridge queue-pop callback
    emitter.instruction("cbz x9, __rt_pcntl_abort_dispatch_restore");           // restore state if no queue callback is available
    emitter.instruction("blr x9");                                              // discard one remaining snapshot record
    emitter.instruction("cmp x0, #1");                                          // did the pipe provide another complete record?
    emitter.instruction("b.eq __rt_pcntl_abort_dispatch_drain");                // keep discarding the original masked snapshot
    emitter.label("__rt_pcntl_abort_dispatch_restore");
    abi::emit_symbol_address(emitter, "x0", "__rt_pcntl_dispatch_mask");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatch_end_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the bridge mask-restore callback
    emitter.instruction("cbz x9, __rt_pcntl_abort_dispatch_clear");             // clear state if no restore callback is available
    emitter.instruction("blr x9");                                              // restore delivery before propagating the Throwable
    emitter.label("__rt_pcntl_abort_dispatch_clear");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("str xzr, [x9]");                                       // guarantee future dispatch calls can run
    emitter.label("__rt_pcntl_abort_dispatch_done");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release cleanup storage
    emitter.instruction("ret");                                                 // resume the exception unwinder
}

/// Emits exception-unwind cleanup for an active x86_64 signal dispatch.
fn emit_pcntl_abort_dispatch_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl abort signal dispatch ---");
    emitter.label_global("__rt_pcntl_abort_dispatch");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable cleanup offsets
    emitter.instruction("sub rsp, 96");                                         // reserve one stable siginfo discard record
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("cmp QWORD PTR [r9], 0");                               // test whether an exception escaped a signal handler
    emitter.instruction("je __rt_pcntl_abort_dispatch_done_x86");               // ordinary throws require no PCNTL cleanup
    emitter.label("__rt_pcntl_abort_dispatch_drain_x86");
    emitter.instruction("mov rdi, rsp");                                        // C arg0 = discard record storage
    emitter.instruction("mov esi, 1");                                          // C arg1 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the bridge queue-pop callback
    emitter.instruction("test r9, r9");                                         // tolerate an unavailable bridge defensively
    emitter.instruction("jz __rt_pcntl_abort_dispatch_restore_x86");            // restore state if no queue callback is available
    emitter.instruction("call r9");                                             // discard one remaining snapshot record
    emitter.instruction("cmp rax, 1");                                          // did the pipe provide another complete record?
    emitter.instruction("je __rt_pcntl_abort_dispatch_drain_x86");              // keep discarding the original masked snapshot
    emitter.label("__rt_pcntl_abort_dispatch_restore_x86");
    abi::emit_symbol_address(emitter, "rdi", "__rt_pcntl_dispatch_mask");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatch_end_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the bridge mask-restore callback
    emitter.instruction("test r9, r9");                                         // skip restoration when no bridge was initialized
    emitter.instruction("jz __rt_pcntl_abort_dispatch_clear_x86");              // clear state if no restore callback is available
    emitter.instruction("call r9");                                             // restore delivery before propagating the Throwable
    emitter.label("__rt_pcntl_abort_dispatch_clear_x86");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // guarantee future dispatch calls can run
    emitter.label("__rt_pcntl_abort_dispatch_done_x86");
    emitter.instruction("leave");                                               // release cleanup storage and restore rbp
    emitter.instruction("ret");                                                 // resume the exception unwinder
}

/// Emits an AArch64 safe-point wrapper that preserves every allocatable register and FP state.
fn emit_pcntl_async_dispatch_preserving_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl async dispatch preserving registers ---");
    emitter.label_global("__rt_pcntl_async_dispatch_preserving");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a minimal flag-test frame
    emitter.instruction("stp x9, x10, [sp]");                                   // preserve both scratch registers used by the fast path
    emitter.instruction("mrs x9, nzcv");                                        // preserve condition flags before testing async state
    emitter.instruction("str x9, [sp, #16]");                                   // park the original condition flags
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_async_enabled");
    emitter.instruction("ldr x10, [x9]");                                       // read async state before spilling the full register file
    emitter.instruction("cbnz x10, __rt_pcntl_async_slow");                     // enter the expensive path only while async dispatch is enabled
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the caller's condition flags
    emitter.instruction("msr nzcv, x9");                                        // restore flags on the disabled fast path
    emitter.instruction("ldp x9, x10, [sp]");                                   // restore scratch registers on the disabled fast path
    emitter.instruction("add sp, sp, #32");                                     // release the minimal flag-test frame
    emitter.instruction("ret");                                                 // return without the 800-byte spill when async mode is off
    emitter.label("__rt_pcntl_async_slow");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload flags before the complete preserving wrapper
    emitter.instruction("msr nzcv, x9");                                        // restore the caller's condition flags
    emitter.instruction("ldp x9, x10, [sp]");                                   // restore scratch registers before the complete spill
    emitter.instruction("add sp, sp, #32");                                     // release the fast-path frame before the regular frame
    emitter.instruction("sub sp, sp, #800");                                    // reserve the complete register and FP-state spill
    for register in (0..32).step_by(2) {
        emitter.instruction(&format!(                                           // preserve one pair of SIMD registers
            "stp q{register}, q{}, [sp, #{}]",
            register + 1,
            register * 16
        ));
    }
    emitter.instruction("str x28, [sp, #736]");                                 // preserve the scratch base register
    emitter.instruction("str x29, [sp, #768]");                                 // preserve the caller frame pointer
    emitter.instruction("str x30, [sp, #776]");                                 // preserve the caller return address
    emitter.instruction("add x28, sp, #512");                                   // address the general-register spill area
    for register in (0..28).step_by(2) {
        emitter.instruction(&format!(                                           // preserve one pair of general registers
            "stp x{register}, x{}, [x28, #{}]",
            register + 1,
            register * 8
        ));
    }
    emitter.instruction("mrs x9, nzcv");                                        // capture the caller's condition flags
    emitter.instruction("str x9, [sp, #744]");                                  // preserve the condition flags across dispatch
    emitter.instruction("mrs x9, fpcr");                                        // capture the caller's FP control state
    emitter.instruction("str x9, [sp, #752]");                                  // preserve the FP control state across dispatch
    emitter.instruction("mrs x9, fpsr");                                        // capture the caller's FP status state
    emitter.instruction("str x9, [sp, #760]");                                  // preserve the FP status state across dispatch
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_async_enabled");
    emitter.instruction("ldr x10, [x9]");                                       // recheck asynchronous dispatch after the complete spill
    emitter.instruction("cbz x10, __rt_pcntl_async_restore");                   // skip work if the mode changed while entering
    emitter.instruction("bl __rt_pcntl_dispatch_pending");                      // run pending handlers with all caller state protected
    emitter.label("__rt_pcntl_async_restore");
    emitter.instruction("ldr x9, [sp, #744]");                                  // reload the caller's condition flags
    emitter.instruction("msr nzcv, x9");                                        // restore condition flags after dispatch
    emitter.instruction("ldr x9, [sp, #752]");                                  // reload the caller's FP control state
    emitter.instruction("msr fpcr, x9");                                        // restore FP control state after dispatch
    emitter.instruction("ldr x9, [sp, #760]");                                  // reload the caller's FP status state
    emitter.instruction("msr fpsr, x9");                                        // restore FP status state after dispatch
    emitter.instruction("add x28, sp, #512");                                   // address the general-register restore area
    for register in (0..28).step_by(2).rev() {
        emitter.instruction(&format!(                                           // restore one pair of general registers
            "ldp x{register}, x{}, [x28, #{}]",
            register + 1,
            register * 8
        ));
    }
    emitter.instruction("ldr x29, [sp, #768]");                                 // restore the caller frame pointer
    emitter.instruction("ldr x30, [sp, #776]");                                 // restore the caller return address
    for register in (0..32).step_by(2).rev() {
        emitter.instruction(&format!(                                           // restore one pair of SIMD registers
            "ldp q{register}, q{}, [sp, #{}]",
            register + 1,
            register * 16
        ));
    }
    emitter.instruction("ldr x28, [sp, #736]");                                 // restore the scratch base register last
    emitter.instruction("add sp, sp, #800");                                    // release the complete preserving spill
    emitter.instruction("ret");                                                 // resume the interrupted generated-code path
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
    emitter.instruction("pushfq");                                              // preserve caller flags around the direct state comparison
    emitter.instruction("cmp QWORD PTR [rip + __rt_pcntl_async_enabled], 0");   // test async state before spilling registers and vectors
    emitter.instruction("jne __rt_pcntl_async_slow_x86");                       // enter the expensive path only while async dispatch is enabled
    emitter.instruction("popfq");                                               // restore flags on the disabled fast path
    emitter.instruction("ret");                                                 // return without the complete spill when async mode is off
    emitter.label("__rt_pcntl_async_slow_x86");
    emitter.instruction("popfq");                                               // restore flags before the complete preserving wrapper
    emitter.instruction("pushfq");                                              // preserve caller flags across asynchronous dispatch
    for register in REGISTERS {
        emitter.instruction(&format!("push {register}"));                       // preserve one general register
    }
    emitter.instruction("sub rsp, 264");                                        // reserve SIMD and floating-point control spill slots
    for register in 0..16 {
        emitter.instruction(&format!("movdqu XMMWORD PTR [rsp + {}], xmm{register}", register * 16)); // preserve one SIMD register
    }
    emitter.instruction("stmxcsr DWORD PTR [rsp + 256]");                       // preserve SSE control and status state
    emitter.instruction("fnstcw WORD PTR [rsp + 260]");                         // preserve x87 control state
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_async_enabled");
    emitter.instruction("cmp QWORD PTR [r9], 0");                               // recheck asynchronous dispatch after the complete spill
    emitter.instruction("je __rt_pcntl_async_restore_x86");                     // skip work if the mode changed while entering
    emitter.instruction("call __rt_pcntl_dispatch_pending");                    // run pending handlers with all caller state protected
    emitter.label("__rt_pcntl_async_restore_x86");
    emitter.instruction("ldmxcsr DWORD PTR [rsp + 256]");                       // restore SSE control and status state
    emitter.instruction("fldcw WORD PTR [rsp + 260]");                          // restore x87 control state
    for register in (0..16).rev() {
        emitter.instruction(&format!("movdqu xmm{register}, XMMWORD PTR [rsp + {}]", register * 16)); // restore one SIMD register
    }
    emitter.instruction("add rsp, 264");                                        // release the SIMD and control-state spill
    for register in REGISTERS.iter().rev() {
        emitter.instruction(&format!("pop {register}"));                        // restore one general register
    }
    emitter.instruction("popfq");                                               // restore caller flags after dispatch
    emitter.instruction("ret");                                                 // resume the interrupted generated-code path
}

/// Emits AArch64 teardown for registered descriptors, OS dispositions, and queued records.
fn emit_pcntl_release_handlers_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");
    emitter.comment("--- runtime: pcntl release signal handlers ---");
    emitter.label_global("__rt_pcntl_release_handlers");
    emitter.instruction("sub sp, sp, #128");                                    // reserve siginfo scratch storage and the saved frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the teardown helper frame
    emitter.instruction("mov x9, #1");                                          // begin with the first valid signal number
    emitter.instruction("str x9, [sp, #96]");                                   // preserve the current table index across calls
    emitter.label("__rt_pcntl_release_handlers_loop");
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the current signal-table index
    emitter.instruction("cmp x9, #128");                                        // detect the end of the fixed handler table
    emitter.instruction("b.hs __rt_pcntl_release_handlers_state");              // finish with global dispatch state
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_kind");
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");                          // inspect the registered handler kind
    emitter.instruction("cbz x11, __rt_pcntl_release_handlers_zero");           // a default disposition needs only PHP-value cleanup
    emitter.instruction("mov x0, x9");                                          // bridge arg0 = signal number
    emitter.instruction("mov x1, #0");                                          // bridge arg1 = default disposition
    emitter.instruction("mov x2, #1");                                          // bridge arg2 = restart-syscalls flag
    emitter.instruction("mov x3, #1");                                          // bridge arg3 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_signal_fn");
    emitter.instruction("ldr x10, [x10]");                                      // load the bridge signal-registration callback
    emitter.instruction("cbz x10, __rt_pcntl_release_handlers_zero");           // clear metadata if no bridge is available
    emitter.instruction("blr x10");                                             // restore the OS default disposition
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the table index after the bridge call
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_kind");
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");                          // reload the registered handler kind
    emitter.instruction("cmp x11, #2");                                         // detect an owned callable descriptor
    emitter.instruction("b.ne __rt_pcntl_release_handlers_zero");               // skip release for SIG_IGN
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load the descriptor owned by the table
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the handler-table ownership
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the table index after release
    emitter.label("__rt_pcntl_release_handlers_zero");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_value");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load the preserved PHP handler value
    emitter.instruction("cbz x0, __rt_pcntl_release_handlers_clear");           // untouched signals own no boxed value
    emitter.instruction("bl __rt_decref_any");                                  // release the table's PHP-value ownership
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the table index after value release
    emitter.label("__rt_pcntl_release_handlers_clear");
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_kind");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");                          // clear the handler-kind entry
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");                          // clear the descriptor entry
    abi::emit_symbol_address(emitter, "x10", "__rt_pcntl_handler_value");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");                          // clear the PHP handler-value entry
    emitter.label("__rt_pcntl_release_handlers_next");
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the completed table index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next signal
    emitter.instruction("str x9, [sp, #96]");                                   // preserve the next table index
    emitter.instruction("b __rt_pcntl_release_handlers_loop");                  // continue scanning registered handlers
    emitter.label("__rt_pcntl_release_handlers_state");
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_async_enabled");
    emitter.instruction("str xzr, [x9]");                                       // disable asynchronous dispatch during shutdown
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_dispatching");
    emitter.instruction("str xzr, [x9]");                                       // clear any interrupted dispatch marker
    emitter.label("__rt_pcntl_release_handlers_drain");
    emitter.instruction("mov x0, sp");                                          // bridge arg0 = discard-record scratch storage
    emitter.instruction("mov x1, #1");                                          // bridge arg1 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "x9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the queued-signal reader callback
    emitter.instruction("cbz x9, __rt_pcntl_release_handlers_done");            // finish if no queue was initialized
    emitter.instruction("blr x9");                                              // discard one queued signal record
    emitter.instruction("cmp x0, #1");                                          // test whether a complete record was consumed
    emitter.instruction("b.eq __rt_pcntl_release_handlers_drain");              // drain every remaining queued record
    emitter.label("__rt_pcntl_release_handlers_done");
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release teardown scratch storage
    emitter.instruction("ret");                                                 // return after all handler ownership is released
}

/// Emits x86_64 teardown for registered descriptors, OS dispositions, and queued records.
fn emit_pcntl_release_handlers_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 4");
    emitter.comment("--- runtime: pcntl release signal handlers ---");
    emitter.label_global("__rt_pcntl_release_handlers");
    emitter.instruction("push rbp");                                            // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish stable teardown local offsets
    emitter.instruction("sub rsp, 112");                                        // reserve siginfo scratch storage and the table index
    emitter.instruction("mov QWORD PTR [rbp - 104], 1");                        // begin with the first valid signal number
    emitter.label("__rt_pcntl_release_handlers_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // reload the current signal-table index
    emitter.instruction("cmp r9, 128");                                         // detect the end of the fixed handler table
    emitter.instruction("jae __rt_pcntl_release_handlers_state_x86");           // finish with global dispatch state
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_kind");
    emitter.instruction("cmp QWORD PTR [r10 + r9*8], 0");                       // inspect the registered handler kind
    emitter.instruction("je __rt_pcntl_release_handlers_zero_x86");             // a default disposition needs only PHP-value cleanup
    emitter.instruction("mov rdi, r9");                                         // bridge arg0 = signal number
    emitter.instruction("xor esi, esi");                                        // bridge arg1 = default disposition
    emitter.instruction("mov edx, 1");                                          // bridge arg2 = restart-syscalls flag
    emitter.instruction("mov ecx, 1");                                          // bridge arg3 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_signal_fn");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the bridge signal-registration callback
    emitter.instruction("test r10, r10");                                       // tolerate an unavailable bridge during teardown
    emitter.instruction("jz __rt_pcntl_release_handlers_zero_x86");             // clear metadata if no bridge is available
    emitter.instruction("call r10");                                            // restore the OS default disposition
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // reload the table index after the bridge call
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_kind");
    emitter.instruction("cmp QWORD PTR [r10 + r9*8], 2");                       // detect an owned callable descriptor
    emitter.instruction("jne __rt_pcntl_release_handlers_zero_x86");            // skip release for SIG_IGN
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");                     // load the descriptor owned by the table
    emitter.instruction("call __rt_callable_descriptor_release");               // release the handler-table ownership
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // reload the table index after release
    emitter.label("__rt_pcntl_release_handlers_zero_x86");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_value");
    emitter.instruction("mov rax, QWORD PTR [r10 + r9*8]");                     // load the preserved PHP handler value
    emitter.instruction("test rax, rax");                                       // untouched signals own no boxed value
    emitter.instruction("jz __rt_pcntl_release_handlers_clear_x86");            // skip release for an empty value slot
    emitter.instruction("call __rt_decref_any");                                // release the table's PHP-value ownership
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // reload the table index after value release
    emitter.label("__rt_pcntl_release_handlers_clear_x86");
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_kind");
    emitter.instruction("mov QWORD PTR [r10 + r9*8], 0");                       // clear the handler-kind entry
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_descriptor");
    emitter.instruction("mov QWORD PTR [r10 + r9*8], 0");                       // clear the descriptor entry
    abi::emit_symbol_address(emitter, "r10", "__rt_pcntl_handler_value");
    emitter.instruction("mov QWORD PTR [r10 + r9*8], 0");                       // clear the PHP handler-value entry
    emitter.label("__rt_pcntl_release_handlers_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 104], 1");                        // advance and preserve the next signal index
    emitter.instruction("jmp __rt_pcntl_release_handlers_loop_x86");            // continue scanning registered handlers
    emitter.label("__rt_pcntl_release_handlers_state_x86");
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_async_enabled");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // disable asynchronous dispatch during shutdown
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_dispatching");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // clear any interrupted dispatch marker
    emitter.label("__rt_pcntl_release_handlers_drain_x86");
    emitter.instruction("lea rdi, [rbp - 96]");                                 // bridge arg0 = discard-record scratch storage
    emitter.instruction("mov esi, 1");                                          // bridge arg1 = generated AOT queue owner
    abi::emit_symbol_address(emitter, "r9", "__rt_pcntl_signal_next_fn");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the queued-signal reader callback
    emitter.instruction("test r9, r9");                                         // tolerate a queue that was never initialized
    emitter.instruction("jz __rt_pcntl_release_handlers_done_x86");             // finish when no reader is available
    emitter.instruction("call r9");                                             // discard one queued signal record
    emitter.instruction("cmp rax, 1");                                          // test whether a complete record was consumed
    emitter.instruction("je __rt_pcntl_release_handlers_drain_x86");            // drain every remaining queued record
    emitter.label("__rt_pcntl_release_handlers_done_x86");
    emitter.instruction("leave");                                               // release teardown storage and restore rbp
    emitter.instruction("ret");                                                 // return after all handler ownership is released
}
