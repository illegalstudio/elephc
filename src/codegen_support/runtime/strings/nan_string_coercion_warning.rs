//! Purpose:
//! Emits the `__rt_warn_nan_coerced_string` runtime helper: PHP 8.5's
//! `unexpected NAN value was coerced to string` E_WARNING, raised wherever a floating-point
//! NAN is coerced to a PHP string. Also owns the message table so the codegen emitter and
//! the `.data` emitter agree on both the byte contents and the byte lengths of the literal.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - `crate::codegen_support::runtime::data::fixed` for the string literal itself.
//!
//! Key details:
//! - The sibling of `arrays::nan_bool_coercion_warning`, from the same PHP 8.5 RFC
//!   (`warnings-php-8-5`, "Coercing NAN to other types"). Measured locally: php 8.5.6 warns,
//!   php 8.2.31 is silent. The gate therefore lives in `nan_string_coercion_warning_enabled`.
//! - ONE probe covers the whole surface. Every PHP form that coerces a float to a string —
//!   `echo`, `(string)`, `strval()`, `implode()`, concatenation, interpolation,
//!   `sprintf('%s')`, `print_r()`, and the string builtins that take a float — reaches
//!   `__rt_ftoa`, so the diagnostic belongs there rather than at each call site.
//! - `var_dump()`, `json_encode()`, `number_format()` and `sprintf('%f')` do NOT warn in php,
//!   and none of them route through `__rt_ftoa`: they own `__rt_ftoa_repr`,
//!   `__rt_json_encode_float`, a direct `snprintf`, and the printf engine respectively. The
//!   placement is what keeps those four silent.
//! - The probe sits at `__rt_ftoa`'s entry, not on its existing `NAN` TEXT branch, because
//!   that branch keys off the byte `snprintf` produced. The sentinel filters below need the
//!   raw bit pattern, and at entry the float is still the only live value.
//! - elephc's in-band `NULL_SENTINEL` and `UNINITIALIZED_TYPED_PROPERTY_SENTINEL` are
//!   themselves quiet-NaN bit patterns, so a missed `float` read or an uninitialized typed
//!   property reaches this conversion looking exactly like a user NAN. Both are filtered by
//!   exact bit compare, as `nan_bool_coercion_warning` does: php reports `Undefined array
//!   key` / a must-not-be-accessed `Error` for those, never the NAN warning.
//! - Like every other elephc runtime diagnostic, the message body carries no
//!   ` in <file> on line <n>` tail (see `foreach_non_iterable_warning`, `docs/php/opcache.md`).

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::{NULL_SENTINEL, UNINITIALIZED_TYPED_PROPERTY_SENTINEL};

/// The PHP 8.5 warning text for a NAN coerced to string.
///
/// Entries are `(symbol, message)`. `crate::codegen_support::runtime::data::fixed` emits them
/// verbatim as `.ascii` literals; this module derives the helper's `write()` length from
/// `message.len()` so the two can never drift.
///
/// Captured from PHP 8.5.6 with `php -n`, which prints it on STDOUT:
/// `Warning: unexpected NAN value was coerced to string in <file> on line <n>`. elephc does
/// not synthesize the ` in <file> on line <n>` tail.
pub const NAN_STRING_COERCION_MESSAGES: &[(&str, &str)] = &[(
    "_diag_nan_coerced_string",
    "Warning: unexpected NAN value was coerced to string\n",
)];

/// The `.data` symbol holding the NAN-to-string warning literal.
const NAN_STRING_COERCION_SYMBOL: &str = "_diag_nan_coerced_string";

/// Returns the byte length of the message stored under `symbol`.
///
/// Panics when the symbol is not in `NAN_STRING_COERCION_MESSAGES`, which would mean the
/// emitter and the table disagree — a compiler bug, not a user-reachable condition.
fn message_len(symbol: &str) -> usize {
    NAN_STRING_COERCION_MESSAGES
        .iter()
        .find(|(name, _)| *name == symbol)
        .map(|(_, message)| message.len())
        .unwrap_or_else(|| panic!("unknown NAN coercion warning symbol {symbol}"))
}

/// Returns true when the active compile profile reports NAN-to-string coercions.
///
/// The diagnostic landed in PHP 8.5; 8.2, 8.3 and 8.4 render `NAN` silently. `__rt_ftoa`
/// consults this before emitting its probe, so a `--php-version 8.4` build carries neither
/// the test nor the call.
///
/// Reading the thread-local compile profile from a runtime emitter is safe with respect to
/// the runtime object cache: `crate::runtime_cache::prepare_runtime_object` keys the cache on
/// a hash of the GENERATED ASSEMBLY TEXT, so a profile that changes the emitted runtime
/// necessarily changes the cache key too.
pub fn nan_string_coercion_warning_enabled() -> bool {
    crate::codegen_support::compile_php_version().version_id() >= 80500
}

/// Emits the `__rt_warn_nan_coerced_string` runtime helper.
///
/// # ABI
/// Input: `d0` / `xmm0` = the float being coerced, which the CALLER has already proven to be
/// unordered with itself (i.e. a NAN). No other input.
/// Output: none.
/// Clobbers: nothing except the condition flags. Every caller-saved register that
/// `__rt_diag_warning` or its `syscall` disturbs is saved and restored, as is the incoming
/// float — `__rt_ftoa` still has to hand that float to `snprintf` after the probe returns.
pub fn emit_nan_string_coercion_warning(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_nan_string_coercion_warning_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: nan_string_coercion_warning ---");
    emitter.label_global("__rt_warn_nan_coerced_string");
    emitter.instruction("stp x29, x30, [sp, #-64]!");                           // preserve frame linkage across the diagnostic call
    emitter.instruction("mov x29, sp");                                         // establish a stable warning helper frame
    emitter.instruction("stp x0, x1, [sp, #16]");                               // keep the helper clobber-free for its one call site
    emitter.instruction("stp x2, x9, [sp, #32]");                               // preserve the diagnostic argument and scratch registers
    emitter.instruction("str x10, [sp, #48]");                                  // preserve the suppression-depth scratch register
    emitter.instruction("str d0, [sp, #56]");                                   // preserve the float __rt_ftoa still has to format

    // -- filter elephc's in-band sentinels, which are quiet-NaN bit patterns php never warns on --
    emitter.instruction("fmov x9, d0");                                         // reinterpret the coerced float as raw bits for the sentinel guards
    abi::emit_load_int_immediate(emitter, "x10", NULL_SENTINEL);
    emitter.instruction("cmp x9, x10");                                         // is this a missed read's in-band null rather than a user NAN?
    emitter.instruction("b.eq __rt_warn_nan_coerced_string_done");              // php reports Undefined array key for that, never the NAN warning
    abi::emit_load_int_immediate(emitter, "x10", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x9, x10");                                         // is this an uninitialized typed property rather than a user NAN?
    emitter.instruction("b.eq __rt_warn_nan_coerced_string_done");              // php reports a must-not-be-accessed Error for that instead

    abi::emit_symbol_address(emitter, "x1", NAN_STRING_COERCION_SYMBOL);
    emitter.instruction(&format!("mov x2, #{}", message_len(NAN_STRING_COERCION_SYMBOL))); // pass the complete NAN coercion warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the PHP NAN coercion warning

    emitter.label("__rt_warn_nan_coerced_string_done");
    emitter.instruction("ldr d0, [sp, #56]");                                   // restore the float __rt_ftoa still has to format
    emitter.instruction("ldr x10, [sp, #48]");                                  // restore the suppression-depth scratch register
    emitter.instruction("ldp x2, x9, [sp, #32]");                               // restore the diagnostic argument and scratch registers
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // restore the caller's live integer registers
    emitter.instruction("ldp x29, x30, [sp], #64");                             // restore frame linkage
    emitter.instruction("ret");                                                 // return so the conversion can proceed
}

/// x86_64 implementation of `__rt_warn_nan_coerced_string`.
///
/// Mirrors the AArch64 logic under the System V convention. The save set is wider than the
/// AArch64 one because the `syscall` instruction inside `__rt_diag_warning` destroys `rcx`
/// and `r11` on top of the `rax`/`rdx`/`rsi`/`rdi`/`r10` the helper itself uses.
///
/// The frame is `push rbp` + `sub rsp, 80`: entry leaves `rsp ≡ 8 (mod 16)`, the push makes
/// it `≡ 0`, and 80 is a multiple of 16, so `__rt_diag_warning` observes the `rsp ≡ 8 (mod 16)`
/// that System V requires at a callee's first instruction.
fn emit_nan_string_coercion_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: nan_string_coercion_warning ---");
    emitter.label_global("__rt_warn_nan_coerced_string");
    emitter.instruction("push rbp");                                            // preserve the caller frame and align the diagnostic call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable warning helper frame
    emitter.instruction("sub rsp, 80");                                         // reserve the clobber-free save area, keeping the 16-byte alignment
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // keep the helper clobber-free for its one call site
    emitter.instruction("mov QWORD PTR [rsp+8], rcx");                          // the syscall inside __rt_diag_warning destroys rcx
    emitter.instruction("mov QWORD PTR [rsp+16], rdx");                         // preserve the write-length register
    emitter.instruction("mov QWORD PTR [rsp+24], rsi");                         // preserve the write-buffer register
    emitter.instruction("mov QWORD PTR [rsp+32], rdi");                         // preserve the message-pointer register
    emitter.instruction("mov QWORD PTR [rsp+40], r10");                         // preserve the suppression-depth scratch register
    emitter.instruction("mov QWORD PTR [rsp+48], r11");                         // the syscall inside __rt_diag_warning destroys r11
    emitter.instruction("movsd QWORD PTR [rsp+56], xmm0");                      // preserve the float __rt_ftoa still has to format

    // -- filter elephc's in-band sentinels, which are quiet-NaN bit patterns php never warns on --
    emitter.instruction("movq rax, xmm0");                                      // reinterpret the coerced float as raw bits for the sentinel guards
    abi::emit_load_int_immediate(emitter, "rcx", NULL_SENTINEL);
    emitter.instruction("cmp rax, rcx");                                        // is this a missed read's in-band null rather than a user NAN?
    emitter.instruction("je __rt_warn_nan_coerced_string_done_linux_x86_64");   // php reports Undefined array key for that, never the NAN warning
    abi::emit_load_int_immediate(emitter, "rcx", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp rax, rcx");                                        // is this an uninitialized typed property rather than a user NAN?
    emitter.instruction("je __rt_warn_nan_coerced_string_done_linux_x86_64");   // php reports a must-not-be-accessed Error for that instead

    abi::emit_symbol_address(emitter, "rdi", NAN_STRING_COERCION_SYMBOL);
    emitter.instruction(&format!("mov esi, {}", message_len(NAN_STRING_COERCION_SYMBOL))); // pass the complete NAN coercion warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the PHP NAN coercion warning

    emitter.label("__rt_warn_nan_coerced_string_done_linux_x86_64");
    emitter.instruction("movsd xmm0, QWORD PTR [rsp+56]");                      // restore the float __rt_ftoa still has to format
    emitter.instruction("mov r11, QWORD PTR [rsp+48]");                         // restore the syscall-destroyed r11
    emitter.instruction("mov r10, QWORD PTR [rsp+40]");                         // restore the suppression-depth scratch register
    emitter.instruction("mov rdi, QWORD PTR [rsp+32]");                         // restore the message-pointer register
    emitter.instruction("mov rsi, QWORD PTR [rsp+24]");                         // restore the write-buffer register
    emitter.instruction("mov rdx, QWORD PTR [rsp+16]");                         // restore the write-length register
    emitter.instruction("mov rcx, QWORD PTR [rsp+8]");                          // restore the syscall-destroyed rcx
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // restore the caller's live integer result register
    emitter.instruction("add rsp, 80");                                         // release the clobber-free save area
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return so the conversion can proceed
}

/// Emits the inline "is this a NAN?" probe that guards a call to
/// `__rt_warn_nan_coerced_string`, followed by `skip_label` itself.
///
/// Emits NOTHING — not even the label — on pre-8.5 profiles, so `--php-version 8.4` codegen
/// is byte-identical to what it was before the diagnostic existed. `skip_label` must
/// therefore be unique but is never referenced by the caller.
///
/// The probe leaves the float in `d0` / `xmm0` untouched (the helper restores it), so the
/// conversion that follows is unchanged.
///
/// A NAN is the only value that compares UNORDERED with itself, which is what both
/// architectures' self-compare tests: AArch64 sets `V` on unordered (`b.vc` skips), x86_64
/// sets `PF` on unordered (`jnp` skips).
///
/// The caller must already have established a frame: the probe issues a call, so on AArch64
/// `x30` has to be saved and on x86_64 the stack has to be at `rsp ≡ 0 (mod 16)`.
pub fn emit_nan_string_coercion_probe(emitter: &mut Emitter, skip_label: &str) {
    if !nan_string_coercion_warning_enabled() {
        return;
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d0, d0");                                 // a NAN is the only value unordered with itself
            emitter.instruction(&format!("b.vc {}", skip_label));               // ordered values render silently
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm0, xmm0");                          // a NAN is the only value unordered with itself
            emitter.instruction(&format!("jnp {}", skip_label));                // ordered values render silently
        }
    }
    abi::emit_call_label(emitter, "__rt_warn_nan_coerced_string");              // report PHP 8.5's NAN-to-string coercion warning
    emitter.label(skip_label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies the helper filters BOTH in-band sentinels before it reaches the diagnostic.
    ///
    /// The end-to-end path cannot pin this: a missed `float` read renders as `0` rather than the
    /// empty string php prints, a separate divergence, so an integration fixture would have to
    /// encode that wrong value to reach the guard. The guards are what keep a missed read
    /// reporting `Undefined array key` alone instead of also inventing a NAN warning, so they are
    /// pinned on the assembly.
    #[test]
    fn the_helper_filters_both_in_band_sentinels_before_warning() {
        for (platform, arch, done_label) in [
            (Platform::MacOS, Arch::AArch64, "__rt_warn_nan_coerced_string_done:"),
            (
                Platform::Linux,
                Arch::X86_64,
                "__rt_warn_nan_coerced_string_done_linux_x86_64:",
            ),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_nan_string_coercion_warning(&mut emitter);
            let asm = emitter.output();
            assert!(
                asm.contains("__rt_warn_nan_coerced_string:\n"),
                "missing entry point for {arch:?}"
            );
            let guarded = asm
                .split_once("__rt_diag_warning")
                .expect("the helper must reach the diagnostic")
                .0;
            assert_eq!(
                guarded.matches(done_label.trim_end_matches(':')).count(),
                2,
                "both sentinel guards must skip the diagnostic on {arch:?}"
            );
            assert!(
                asm.contains(done_label),
                "the skip target must exist on {arch:?}"
            );
        }
    }

    /// Verifies the emitted `write()` length is the literal's own length, on both targets.
    ///
    /// The message lives in `.data` through `NAN_STRING_COERCION_MESSAGES` and the length is
    /// derived from the same table entry, so a reworded warning cannot silently start writing a
    /// truncated or over-long line.
    #[test]
    fn the_emitted_length_matches_the_message_table() {
        let expected = NAN_STRING_COERCION_MESSAGES[0].1.len();
        assert_eq!(expected, message_len(NAN_STRING_COERCION_SYMBOL));
        for (platform, arch, needle) in [
            (Platform::MacOS, Arch::AArch64, format!("mov x2, #{expected}")),
            (Platform::Linux, Arch::X86_64, format!("mov esi, {expected}")),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_nan_string_coercion_warning(&mut emitter);
            assert!(
                emitter.output().contains(&needle),
                "{arch:?} must pass the table's own length"
            );
        }
    }
}
