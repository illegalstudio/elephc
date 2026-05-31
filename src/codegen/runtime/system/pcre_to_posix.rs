//! Purpose:
//! Emits the `__rt_pcre_to_posix` compatibility helper used by preg runtimes.
//! The helper now preserves PCRE syntax and only materializes a C string for
//! the PCRE2 POSIX wrapper.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - The symbol name is retained so existing preg emitters keep one call path,
//!   but no PCRE escapes are rewritten before PCRE2 sees the pattern.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_pcre_to_posix` as a compatibility tail-call to `__rt_cstr`.
///
/// Despite the historic helper name, this no longer translates PCRE to POSIX
/// syntax. It copies the stripped pattern bytes into the primary C-string
/// scratch buffer and returns the buffer pointer in the platform result
/// register so `pcre2_regcomp()` receives the original PCRE pattern.
pub(crate) fn emit_pcre_to_posix(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pcre_pattern_to_cstr ---");
    emitter.label_global("__rt_pcre_to_posix");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("b __rt_cstr");                                 // tail-call the shared C-string copier while preserving the caller return address
        }
        Arch::X86_64 => {
            emitter.instruction("jmp __rt_cstr");                               // tail-call the x86_64 C-string copier while preserving the caller return address
        }
    }
}
