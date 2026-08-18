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
