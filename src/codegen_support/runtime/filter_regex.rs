//! Purpose:
//! Bridges the filter bridge's optional PCRE2 diagnostics to the managed pcre2 package.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - The symbol is always emitted because the `elephc_filter` staticlib references it.
//! - Only a program that actually selected the regex runtime feature emits the tail call,
//!   so filter programs without FILTER_VALIDATE_REGEXP never require the pcre2 package.
//! - The stub returns -1, telling the bridge to use its pure-Rust fallback diagnostic.

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
        }
    }
}
