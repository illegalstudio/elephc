//! Purpose:
//! Emits the inline locale preparation sequence used before libc regex compilation.
//! It lets POSIX character classes observe the process UTF-8 locale for PCRE property shims.
//!
//! Called from:
//! - `crate::codegen::runtime::system::preg_match`
//! - `crate::codegen::runtime::system::preg_match_all`
//! - `crate::codegen::runtime::system::preg_replace`
//! - `crate::codegen::runtime::system::preg_split`
//!
//! Key details:
//! - `setlocale(LC_CTYPE, "C.UTF-8")` avoids depending on C runtime environment
//!   initialization; the empty locale name remains a fallback for other platforms.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the locale preparation sequence used before compiling POSIX regex patterns.
///
/// For each target architecture, emits instructions that:
/// - Select `LC_CTYPE` category so character classes observe the locale
/// - Attempt to activate `"C.UTF-8"` locale first for Unicode POSIX class support
/// - Fall back to the empty-string locale (environment-supplied) if `C.UTF-8` is unavailable
///
/// # Arguments
/// * `emitter` — target-specific instruction emitter; carries architecture and platform context
///
/// # Side effects
/// Calls `setlocale(LC_CTYPE, ...)` which modifies the process-wide locale state.
/// This must be called before any regex compilation to ensure consistent character class behavior
/// in statically linked binaries that lack runtime C library initialization.
///
/// # ABI notes
/// AArch64: passes `LC_CTYPE` in `x0`, locale name pointer in `x1`, result in `x0`
/// X86_64: passes `LC_CTYPE` in `edi`, locale name pointer in `rsi`, result in `rax`
pub(crate) fn emit_prepare_regex_locale(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", emitter.platform.lc_ctype())); // select LC_CTYPE so character classes use the environment locale
            emitter.adrp("x1", "_locale_utf8_name");                           // load page address of the explicit UTF-8 locale name
            emitter.add_lo12("x1", "x1", "_locale_utf8_name");                 // pass C.UTF-8 so startup-free binaries get Unicode classes
            emitter.bl_c("setlocale");                                          // activate the UTF-8 locale category before compiling regex
            emitter.instruction("cbnz x0, 1f");                                 // skip fallback when the explicit UTF-8 locale is available
            emitter.instruction(&format!("mov x0, #{}", emitter.platform.lc_ctype())); // reselect LC_CTYPE for the environment-locale fallback
            emitter.adrp("x1", "_locale_env_name");                            // load page address of the empty locale name
            emitter.add_lo12("x1", "x1", "_locale_env_name");                  // pass "" so setlocale reads LC_* from the environment
            emitter.bl_c("setlocale");                                          // try the environment locale when C.UTF-8 is unavailable
            emitter.label("1");
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov edi, {}", emitter.platform.lc_ctype())); // select LC_CTYPE so character classes use the environment locale
            abi::emit_symbol_address(emitter, "rsi", "_locale_utf8_name");
            emitter.bl_c("setlocale");                                          // activate the UTF-8 locale category before compiling regex
            emitter.instruction("test rax, rax");                               // check whether the explicit UTF-8 locale was accepted
            emitter.instruction("jnz 1f");                                      // skip fallback when the explicit UTF-8 locale is available
            emitter.instruction(&format!("mov edi, {}", emitter.platform.lc_ctype())); // reselect LC_CTYPE for the environment-locale fallback
            abi::emit_symbol_address(emitter, "rsi", "_locale_env_name");
            emitter.bl_c("setlocale");                                          // try the environment locale when C.UTF-8 is unavailable
            emitter.label("1");
        }
    }
}
