//! Purpose:
//! Bridges the filter bridge's PCRE2 diagnostics and matching to the managed pcre2 package.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Both symbols are always emitted because the `elephc_filter` staticlib references them.
//! - Only a program that actually selected the regex runtime feature emits the tail calls,
//!   so filter programs without FILTER_VALIDATE_REGEXP never require the pcre2 package.
//! - The stubs return -1, telling the bridge to use its pure-Rust fallback.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};
use crate::codegen_support::RuntimeFeatures;

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
            _ => {
                emitter.instruction("mov w0, #-1");                             // unsupported pairing; use the Rust fallback
                emitter.instruction("ret");                                     // return the unavailable marker
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
    emit_filter_preg_match(emitter, features);
}

/// Emits the filter bridge's PCRE2 matching entry point, a tail call or a negative stub.
///
/// The bridge calls this with the C ABI `(pattern, pattern_len, subject, subject_len)`.
/// The shared `__rt_preg_match` helper parses PHP's delimiters and modifiers and runs
/// the match through the same managed PCRE2 path as `preg_match()`.
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
                emitter.instruction("mov x4, x3");                              // move the subject length into the helper's fourth argument register
                emitter.instruction("mov x3, x2");                              // move the subject pointer into the helper's third argument register
                emitter.instruction("mov x2, x1");                              // move the pattern length into the helper's second argument register
                emitter.instruction("mov x1, x0");                              // move the pattern pointer into the helper's first argument register
                emitter.instruction("b __rt_preg_match");                       // tail-call the shared PCRE2 preg_match runtime
            }
            Arch::X86_64 => {
                emitter.instruction("jmp __rt_preg_match");                     // the C ABI already matches the shared preg_match helper
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

    /// The filter bridge always references this runtime symbol, even without native regex.
    #[test]
    fn filter_regex_symbol_is_emitted() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = crate::codegen::generate_runtime_with_features(
                8_388_608,
                target,
                crate::codegen_support::RuntimeFeatures::none(),
            );
            assert!(asm.contains("filter_pcre2_compile_check"), "{target:?}");
            assert!(asm.contains("filter_preg_match"), "{target:?}");
        }
    }

    /// The matching entry point tail-calls the shared PCRE2 runtime when regex is enabled.
    #[test]
    fn filter_preg_match_tail_calls_shared_runtime_with_regex() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = crate::codegen::generate_runtime_with_features(
                8_388_608,
                target,
                crate::codegen_support::RuntimeFeatures {
                    regex: true,
                    ..crate::codegen_support::RuntimeFeatures::none()
                },
            );
            assert!(asm.contains("filter_preg_match"), "{target:?}");
            assert!(asm.contains("__rt_preg_match"), "{target:?}");
        }
    }
}
