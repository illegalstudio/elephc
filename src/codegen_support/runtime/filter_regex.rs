//! Purpose:
//! Bridges the filter bridge's PCRE2 diagnostics and matching to the managed pcre2 package.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Both symbols are emitted only when the filter prelude is linked, because the
//!   `elephc_filter` staticlib references them.
//! - The compile check uses the same locale character tables as matching, and the match
//!   entry point preserves the caller's `preg_last_error()` state.
//! - The stubs return -1; a production validation without the managed PCRE2 path reports
//!   a clear missing-package error instead of silently matching through the Rust fallback.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}, RuntimeFeatures};

/// Emits `__rt_filter_pcre2_compile_check`, a tail call or a negative stub.
pub(crate) fn emit_filter_regex(emitter: &mut Emitter, features: RuntimeFeatures) {
    emitter.blank();
    emitter.comment("--- runtime: filter regex diagnostics ---");
    // Mach-O applies the C underscore when Rust links `extern "C"`, while assembly labels
    // are literal. Spell the platform symbol the Rust bridge actually references.
    let label = match emitter.target.platform {
        Platform::MacOS => "___rt_filter_pcre2_compile_check",
        Platform::Linux => "__rt_filter_pcre2_compile_check",
        Platform::Windows => "__rt_filter_pcre2_compile_check",
    };
    emitter.label_global(label);
    if features.regex {
        match (emitter.target.platform, emitter.target.arch) {
            (Platform::MacOS, Arch::AArch64) => {
                emitter.instruction("b _elephc_pcre2_v1_php_compile_check");    // tail-call PCRE2 for an exact compile diagnostic
            }
            (Platform::Linux, Arch::AArch64) => {
                emitter.instruction("b elephc_pcre2_v1_php_compile_check");     // tail-call PCRE2 for an exact compile diagnostic
            }
            (Platform::Linux, Arch::X86_64) => {
                emitter.instruction("jmp elephc_pcre2_v1_php_compile_check");   // tail-call PCRE2 for an exact compile diagnostic
            }
            _ => match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov w0, #-1");                         // unsupported pairing; use the Rust fallback
                    emitter.instruction("ret");                                 // return the unavailable marker
                }
                Arch::X86_64 => {
                    emitter.instruction("mov eax, -1");                         // unsupported pairing; use the Rust fallback
                    emitter.instruction("ret");                                 // return the unavailable marker
                }
            },
        }
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov w0, #-1");                             // no PCRE2 package linked; use the Rust fallback
                emitter.instruction("ret");                                     // return the unavailable marker
            }
            Arch::X86_64 => {
                emitter.instruction("mov eax, -1");                             // no PCRE2 package linked; use the Rust fallback
                emitter.instruction("ret");                                     // return the unavailable marker
            }
        }
    }
    emit_filter_preg_match(emitter, features);
}

/// Emits the filter bridge's PCRE2 matching entry point, a shared-runtime call or a negative stub.
///
/// The bridge calls this with the C ABI `(pattern, pattern_len, subject, subject_len)`.
/// The shared `__rt_preg_match` helper parses PHP's delimiters and modifiers and runs
/// the match through the same managed PCRE2 path as `preg_match()`. Unlike `preg_match()`,
/// `FILTER_VALIDATE_REGEXP` must not change `preg_last_error()`, so both preg error
/// globals are saved around the shared helper and restored before returning.
fn emit_filter_preg_match(emitter: &mut Emitter, features: RuntimeFeatures) {
    emitter.blank();
    emitter.comment("--- runtime: filter regex matching ---");
    // Mach-O applies the C underscore when Rust links `extern "C"`, while assembly labels
    // are literal. Spell the platform symbol the Rust bridge actually references.
    let label = match emitter.target.platform {
        Platform::MacOS => "___rt_filter_preg_match",
        Platform::Linux => "__rt_filter_preg_match",
        Platform::Windows => "__rt_filter_preg_match",
    };
    emitter.label_global(label);
    if features.regex {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("sub sp, sp, #80");                         // reserve argument spills, saved preg state, and the result slot
                emitter.instruction("stp x29, x30, [sp, #0]");                  // save the caller frame pointer and return address
                emitter.instruction("add x29, sp, #0");                         // establish the helper frame pointer
                emitter.instruction("stp x0, x1, [sp, #16]");                   // save the pattern pointer and length
                emitter.instruction("stp x2, x3, [sp, #32]");                   // save the subject pointer and length
                abi::emit_load_symbol_to_reg(emitter, "x9", "_preg_last_error", 0);
                emitter.instruction("str x9, [sp, #48]");                       // preserve the caller's PREG_* error code
                abi::emit_load_symbol_to_reg(emitter, "x9", "_preg_exec_error", 0);
                emitter.instruction("str x9, [sp, #56]");                       // preserve the caller's last-exec outcome
                emitter.instruction("ldr x1, [sp, #16]");                       // pass the pattern pointer as the shared helper's first argument
                emitter.instruction("ldr x2, [sp, #24]");                       // pass the pattern length as the shared helper's second argument
                emitter.instruction("ldr x3, [sp, #32]");                       // pass the subject pointer as the shared helper's third argument
                emitter.instruction("ldr x4, [sp, #40]");                       // pass the subject length as the shared helper's fourth argument
                emitter.instruction("bl __rt_preg_match");                      // run the shared PCRE2 preg_match runtime
                emitter.instruction("str x0, [sp, #64]");                       // retain the match result across state restoration
                emitter.instruction("ldr x9, [sp, #48]");                       // reload the caller's PREG_* error code
                abi::emit_store_reg_to_symbol(emitter, "x9", "_preg_last_error", 0);
                emitter.instruction("ldr x9, [sp, #56]");                       // reload the caller's last-exec outcome
                abi::emit_store_reg_to_symbol(emitter, "x9", "_preg_exec_error", 0);
                emitter.instruction("ldr x0, [sp, #64]");                       // return the match result unchanged
                emitter.instruction("ldp x29, x30, [sp, #0]");                  // restore the caller frame pointer and return address
                emitter.instruction("add sp, sp, #80");                         // release the helper frame
                emitter.instruction("ret");                                     // return to the filter bridge
            }
            Arch::X86_64 => {
                emitter.instruction("push rbp");                                // keep the shared-runtime call 16-byte aligned
                emitter.instruction("mov rbp, rsp");                            // establish the helper frame pointer
                emitter.instruction("sub rsp, 48");                             // reserve argument spills, saved preg state, and the result slot
                emitter.instruction("mov QWORD PTR [rbp - 8], rdi");            // save the pattern pointer
                emitter.instruction("mov QWORD PTR [rbp - 16], rsi");           // save the pattern length
                emitter.instruction("mov QWORD PTR [rbp - 24], rdx");           // save the subject pointer
                emitter.instruction("mov QWORD PTR [rbp - 32], rcx");           // save the subject length
                abi::emit_load_symbol_to_reg(emitter, "r8", "_preg_last_error", 0);
                emitter.instruction("mov QWORD PTR [rbp - 40], r8");            // preserve the caller's PREG_* error code
                abi::emit_load_symbol_to_reg(emitter, "r8", "_preg_exec_error", 0);
                emitter.instruction("mov QWORD PTR [rbp - 48], r8");            // preserve the caller's last-exec outcome
                emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");            // restore the pattern pointer
                emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");           // restore the pattern length
                emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");           // restore the subject pointer
                emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");           // restore the subject length
                emitter.instruction("call __rt_preg_match");                    // run the shared PCRE2 preg_match runtime
                emitter.instruction("mov QWORD PTR [rbp - 8], rax");            // retain the match result across state restoration
                emitter.instruction("mov r8, QWORD PTR [rbp - 40]");            // reload the caller's PREG_* error code
                abi::emit_store_reg_to_symbol(emitter, "r8", "_preg_last_error", 0);
                emitter.instruction("mov r8, QWORD PTR [rbp - 48]");            // reload the caller's last-exec outcome
                abi::emit_store_reg_to_symbol(emitter, "r8", "_preg_exec_error", 0);
                emitter.instruction("mov rax, QWORD PTR [rbp - 8]");            // return the match result unchanged
                emitter.instruction("mov rsp, rbp");                            // release the helper frame
                emitter.instruction("pop rbp");                                 // restore the caller frame pointer
                emitter.instruction("ret");                                     // return to the filter bridge
            }
        }
    } else {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov w0, #-1");                             // no PCRE2 package linked; use the Rust fallback
                emitter.instruction("ret");                                     // return the unavailable marker
            }
            Arch::X86_64 => {
                emitter.instruction("mov eax, -1");                             // no PCRE2 package linked; use the Rust fallback
                emitter.instruction("ret");                                     // return the unavailable marker
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// The filter helpers are emitted only for programs that inject the filter prelude.
    #[test]
    fn filter_regex_symbol_is_emitted_with_the_filter_feature() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = crate::codegen::generate_runtime_with_features(
                8_388_608,
                target,
                crate::codegen_support::RuntimeFeatures {
                    filter: true,
                    ..crate::codegen_support::RuntimeFeatures::none()
                },
            );
            assert!(asm.contains("filter_pcre2_compile_check"), "{target:?}");
            assert!(asm.contains("filter_preg_match"), "{target:?}");
        }
    }

    /// The bridge references these symbols only when it is linked.
    #[test]
    fn filter_regex_symbols_are_absent_without_filter() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = crate::codegen::generate_runtime_with_features(
                8_388_608,
                target,
                crate::codegen_support::RuntimeFeatures::none(),
            );
            assert!(!asm.contains("filter_pcre2_compile_check"), "{target:?}");
            assert!(!asm.contains("filter_preg_match"), "{target:?}");
        }
    }

    /// The matching entry point calls the shared PCRE2 runtime when regex is enabled.
    #[test]
    fn filter_preg_match_calls_shared_runtime_with_regex() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = crate::codegen::generate_runtime_with_features(
                8_388_608,
                target,
                crate::codegen_support::RuntimeFeatures {
                    filter: true,
                    regex: true,
                    ..crate::codegen_support::RuntimeFeatures::none()
                },
            );
            assert!(asm.contains("filter_preg_match"), "{target:?}");
            assert!(asm.contains("__rt_preg_match"), "{target:?}");
            assert!(asm.contains("_preg_last_error"), "{target:?}");
            assert!(asm.contains("_preg_exec_error"), "{target:?}");
        }
    }
}
