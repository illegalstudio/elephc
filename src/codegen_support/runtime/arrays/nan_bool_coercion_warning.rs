//! Purpose:
//! Emits the `__rt_warn_nan_coerced_bool` and `__rt_warn_nan_coerced_string` runtime helpers:
//! PHP 8.5's `unexpected NAN value was coerced to bool|string` E_WARNINGs, raised at every
//! site where a floating-point NAN is coerced to a PHP boolean or string. Also emits
//! `__rt_ftoa_coerce`, the float-to-string formatter entry for string COERCIONS, and owns the
//! message table so the codegen emitter and the `.data` emitter agree on both the byte
//! contents and the byte lengths of the literals.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//! - `crate::codegen_support::runtime::data::fixed` for the string literal itself.
//!
//! Key details:
//! - The diagnostic is NEW IN PHP 8.5 (RFC `warnings-php-8-5`, "Coercing NAN to other
//!   types"); 8.2/8.3/8.4 coerce silently. Verified locally against php 8.5.6 (warns) and
//!   php 8.2.31 (silent). Gating therefore lives in `nan_bool_coercion_warning_enabled`,
//!   which every call site consults so pre-8.5 profiles emit no probe at all.
//! - The NAN TEST ITSELF IS INLINE AT THE CALL SITE (`fcmp d0, d0` + `b.vc` /
//!   `ucomisd xmm0, xmm0` + `jnp`), so the non-NAN hot path costs two instructions and never
//!   makes a call. This helper only runs once a NAN has already been observed.
//! - Because those call sites sit inside arbitrary lowering, the helper is written to be
//!   CLOBBER-FREE: it saves and restores every caller-saved register `__rt_diag_warning`
//!   (and the `syscall` instruction underneath it) disturbs, plus the incoming float. Only
//!   the condition flags are left dirty, and every caller re-computes them immediately after.
//! - elephc's in-band `NULL_SENTINEL` (`0x7fff_ffff_ffff_fffe`) and
//!   `UNINITIALIZED_TYPED_PROPERTY_SENTINEL` (`0x7fff_ffff_ffff_fffd`) are THEMSELVES quiet-NaN
//!   bit patterns, so a missed `float` array read or an uninitialized typed property reaches a
//!   truthiness site looking exactly like a user NAN. Both are filtered here by exact bit
//!   compare — php-src reports `Undefined array key` / `must not be accessed before
//!   initialization` for those, never the NAN warning, and warning on them would be a
//!   user-visible false positive.
//! - Like every other elephc runtime diagnostic, the message body carries no
//!   ` in <file> on line <n>` tail (see `foreach_non_iterable_warning`, `docs/php/opcache.md`).
//! - The string warning's probe lives in `__rt_ftoa_coerce` rather than at the call sites:
//!   every string coercion of a float already calls a formatter, so routing the coercing
//!   callers there costs one self-compare per conversion. `__rt_ftoa` itself stays silent,
//!   because php-src also formats floats without coercing them (a comparison against a
//!   non-numeric string, a `debug_print_backtrace()` argument) and never warns for those.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::{NULL_SENTINEL, UNINITIALIZED_TYPED_PROPERTY_SENTINEL};

/// The PHP 8.5 warning texts for a NAN coerced to bool and to string.
///
/// Entries are `(symbol, message)`. `crate::codegen_support::runtime::data::fixed`
/// emits them verbatim as `.ascii` literals; this module derives each helper's `write()`
/// length from `message.len()` so the two can never drift.
///
/// The wording is php-src's `E_WARNING` from `zend_operators.c`'s NAN coercion paths,
/// captured from PHP 8.5 with `php -d xdebug.mode=off`: `unexpected NAN value was coerced to
/// bool` and `unexpected NAN value was coerced to string`. elephc does not synthesize the
/// ` in <file> on line <n>` tail that php-src appends.
pub const NAN_COERCION_MESSAGES: &[(&str, &str)] = &[
    (
        "_diag_nan_coerced_bool",
        "Warning: unexpected NAN value was coerced to bool\n",
    ),
    (
        "_diag_nan_coerced_string",
        "Warning: unexpected NAN value was coerced to string\n",
    ),
];

/// The `.data` symbol holding the NAN-to-bool warning literal.
const NAN_BOOL_COERCION_SYMBOL: &str = "_diag_nan_coerced_bool";

/// The `.data` symbol holding the NAN-to-string warning literal.
const NAN_STRING_COERCION_SYMBOL: &str = "_diag_nan_coerced_string";

/// Returns the byte length of the message stored under `symbol`.
///
/// Panics when the symbol is not in `NAN_COERCION_MESSAGES`, which would mean the
/// emitter and the table disagree — a compiler bug, not a user-reachable condition.
fn message_len(symbol: &str) -> usize {
    NAN_COERCION_MESSAGES
        .iter()
        .find(|(name, _)| *name == symbol)
        .map(|(_, message)| message.len())
        .unwrap_or_else(|| panic!("unknown NAN coercion warning symbol {symbol}"))
}

/// Returns true when the active compile profile reports NAN-to-bool and NAN-to-string
/// coercions.
///
/// The diagnostic landed in PHP 8.5 (RFC `warnings-php-8-5`); 8.2, 8.3 and 8.4 coerce NAN to
/// `true` silently. Every call site — the three `src/codegen/lower_inst` float truthiness
/// emitters and the two boxed-Mixed runtime helpers — consults this before emitting its probe,
/// so a `--php-version 8.4` build carries neither the test nor the call.
///
/// Reading the thread-local compile profile from a runtime emitter is safe with respect to the
/// runtime object cache: `crate::runtime_cache::prepare_runtime_object` keys the cache on a hash
/// of the GENERATED ASSEMBLY TEXT, so a profile that changes the emitted runtime necessarily
/// changes the cache key too.
pub fn nan_bool_coercion_warning_enabled() -> bool {
    crate::codegen_support::compile_php_version().version_id() >= 80500
}

/// Emits the NAN coercion runtime helpers: `__rt_warn_nan_coerced_bool`,
/// `__rt_warn_nan_coerced_string` and `__rt_ftoa_coerce`.
pub fn emit_nan_coercion_warnings(emitter: &mut Emitter) {
    emit_nan_coercion_warning(emitter, "__rt_warn_nan_coerced_bool", NAN_BOOL_COERCION_SYMBOL);
    emit_nan_coercion_warning(
        emitter,
        "__rt_warn_nan_coerced_string",
        NAN_STRING_COERCION_SYMBOL,
    );
    emit_ftoa_coerce(emitter);
}

/// Emits one NAN coercion warning helper, `label`, that reports the message under `symbol`.
///
/// Dispatches to the target-specific implementation; x86_64 uses the System V
/// register convention, every other target uses the AArch64 path.
///
/// # ABI
/// Input: `d0` / `xmm0` = the float being coerced, which the CALLER has already proven to be
/// unordered with itself (i.e. a NAN). No other input.
/// Output: none.
/// Clobbers: nothing except the condition flags. Every caller-saved register that
/// `__rt_diag_warning` or its `syscall` disturbs — `x0`/`x1`/`x2`/`x9`/`x10` on AArch64,
/// `rax`/`rcx`/`rdx`/`rsi`/`rdi`/`r10`/`r11` on x86_64 — is saved and restored, as is the
/// incoming float, so the helper can be dropped into any lowering context without disturbing
/// live values.
fn emit_nan_coercion_warning(emitter: &mut Emitter, label: &str, symbol: &str) {
    if emitter.target.arch == Arch::X86_64 {
        emit_nan_coercion_warning_x86_64(emitter, label, symbol);
        return;
    }

    let done = format!("{label}_done");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);
    emitter.instruction("stp x29, x30, [sp, #-64]!");                           // preserve frame linkage across the diagnostic call
    emitter.instruction("mov x29, sp");                                         // establish a stable warning helper frame
    emitter.instruction("stp x0, x1, [sp, #16]");                               // keep the helper clobber-free for arbitrary truthiness call sites
    emitter.instruction("stp x2, x9, [sp, #32]");                               // preserve the diagnostic argument and scratch registers
    emitter.instruction("str x10, [sp, #48]");                                  // preserve the suppression-depth scratch register
    emitter.instruction("str d0, [sp, #56]");                                   // preserve the float being coerced for the caller's own compare

    // -- filter elephc's in-band sentinels, which are quiet-NaN bit patterns php never warns on --
    emitter.instruction("fmov x9, d0");                                         // reinterpret the coerced float as raw bits for the sentinel guards
    abi::emit_load_int_immediate(emitter, "x10", NULL_SENTINEL);
    emitter.instruction("cmp x9, x10");                                         // is this a missed read's in-band null rather than a user NAN?
    emitter.instruction(&format!("b.eq {done}"));                               // php reports Undefined array key for that, never the NAN warning
    abi::emit_load_int_immediate(emitter, "x10", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x9, x10");                                         // is this an uninitialized typed property rather than a user NAN?
    emitter.instruction(&format!("b.eq {done}"));                               // php reports a must-not-be-accessed Error for that instead

    abi::emit_symbol_address(emitter, "x1", symbol);
    emitter.instruction(&format!("mov x2, #{}", message_len(symbol)));          // pass the complete NAN coercion warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the PHP NAN coercion warning

    emitter.label(&done);
    emitter.instruction("ldr d0, [sp, #56]");                                   // restore the float the caller still has to compare
    emitter.instruction("ldr x10, [sp, #48]");                                  // restore the suppression-depth scratch register
    emitter.instruction("ldp x2, x9, [sp, #32]");                               // restore the diagnostic argument and scratch registers
    emitter.instruction("ldp x0, x1, [sp, #16]");                               // restore the caller's live integer registers
    emitter.instruction("ldp x29, x30, [sp], #64");                             // restore frame linkage
    emitter.instruction("ret");                                                 // return so the caller can finish the coercion
}

/// x86_64 implementation of one NAN coercion warning helper (`label`, reporting `symbol`).
///
/// Mirrors the AArch64 logic under the System V convention. The save set is wider than the
/// AArch64 one because the `syscall` instruction inside `__rt_diag_warning` destroys `rcx` and
/// `r11` on top of the `rax`/`rdx`/`rsi`/`rdi`/`r10` the helper itself uses.
///
/// The frame is `push rbp` + `sub rsp, 80`: entry leaves `rsp ≡ 8 (mod 16)`, the push makes it
/// `≡ 0`, and 80 is a multiple of 16, so `__rt_diag_warning` observes the `rsp ≡ 8 (mod 16)`
/// that System V requires at a callee's first instruction.
fn emit_nan_coercion_warning_x86_64(emitter: &mut Emitter, label: &str, symbol: &str) {
    let done = format!("{label}_done_linux_x86_64");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);
    emitter.instruction("push rbp");                                            // preserve the caller frame and align the diagnostic call
    emitter.instruction("mov rbp, rsp");                                        // establish a stable warning helper frame
    emitter.instruction("sub rsp, 80");                                         // reserve the clobber-free save area, keeping the 16-byte alignment
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // keep the helper clobber-free for arbitrary truthiness call sites
    emitter.instruction("mov QWORD PTR [rsp+8], rcx");                          // the syscall inside __rt_diag_warning destroys rcx
    emitter.instruction("mov QWORD PTR [rsp+16], rdx");                         // preserve the write-length register
    emitter.instruction("mov QWORD PTR [rsp+24], rsi");                         // preserve the write-buffer register
    emitter.instruction("mov QWORD PTR [rsp+32], rdi");                         // preserve the message-pointer register
    emitter.instruction("mov QWORD PTR [rsp+40], r10");                         // preserve the suppression-depth scratch register
    emitter.instruction("mov QWORD PTR [rsp+48], r11");                         // the syscall inside __rt_diag_warning destroys r11
    emitter.instruction("movsd QWORD PTR [rsp+56], xmm0");                      // preserve the float being coerced for the caller's own compare

    // -- filter elephc's in-band sentinels, which are quiet-NaN bit patterns php never warns on --
    emitter.instruction("movq rax, xmm0");                                      // reinterpret the coerced float as raw bits for the sentinel guards
    abi::emit_load_int_immediate(emitter, "rcx", NULL_SENTINEL);
    emitter.instruction("cmp rax, rcx");                                        // is this a missed read's in-band null rather than a user NAN?
    emitter.instruction(&format!("je {done}"));                                 // php reports Undefined array key for that, never the NAN warning
    abi::emit_load_int_immediate(emitter, "rcx", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp rax, rcx");                                        // is this an uninitialized typed property rather than a user NAN?
    emitter.instruction(&format!("je {done}"));                                 // php reports a must-not-be-accessed Error for that instead

    abi::emit_symbol_address(emitter, "rdi", symbol);
    emitter.instruction(&format!("mov esi, {}", message_len(symbol)));          // pass the complete NAN coercion warning length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the PHP NAN coercion warning

    emitter.label(&done);
    emitter.instruction("movsd xmm0, QWORD PTR [rsp+56]");                      // restore the float the caller still has to compare
    emitter.instruction("mov r11, QWORD PTR [rsp+48]");                         // restore the syscall-destroyed r11
    emitter.instruction("mov r10, QWORD PTR [rsp+40]");                         // restore the suppression-depth scratch register
    emitter.instruction("mov rdi, QWORD PTR [rsp+32]");                         // restore the message-pointer register
    emitter.instruction("mov rsi, QWORD PTR [rsp+24]");                         // restore the write-buffer register
    emitter.instruction("mov rdx, QWORD PTR [rsp+16]");                         // restore the write-length register
    emitter.instruction("mov rcx, QWORD PTR [rsp+8]");                          // restore the syscall-destroyed rcx
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // restore the caller's live integer result register
    emitter.instruction("add rsp, 80");                                         // release the clobber-free save area
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return so the caller can finish the coercion
}

/// Emits `__rt_ftoa_coerce`: `__rt_ftoa` preceded by PHP 8.5's NAN-to-string warning.
///
/// # ABI
/// Same as `__rt_ftoa`: input `d0` / `xmm0`, output the formatted string pair. A NAN first
/// calls the clobber-free `__rt_warn_nan_coerced_string`, which restores the float, and then
/// every value tail-jumps into `__rt_ftoa`. On pre-8.5 profiles the entry is a bare jump.
fn emit_ftoa_coerce(emitter: &mut Emitter) {
    let enabled = nan_bool_coercion_warning_enabled();
    emitter.blank();
    emitter.comment("--- runtime: ftoa_coerce ---");
    emitter.label_global("__rt_ftoa_coerce");
    match emitter.target.arch {
        Arch::AArch64 => {
            if enabled {
                emitter.instruction("fcmp d0, d0");                             // a NAN is the only value unordered with itself
                emitter.instruction("b.vs __rt_ftoa_coerce_nan");               // only a NAN takes the warning path
                emitter.instruction("b __rt_ftoa");                             // ordered values format silently
                emitter.label("__rt_ftoa_coerce_nan");
                emitter.instruction("stp x29, x30, [sp, #-16]!");               // preserve the return address across the diagnostic call
                emitter.instruction("mov x29, sp");                             // establish a frame for the diagnostic call
                emitter.instruction("bl __rt_warn_nan_coerced_string");         // report PHP 8.5's NAN-to-string coercion warning
                emitter.instruction("ldp x29, x30, [sp], #16");                 // restore the caller's return address
            }
            emitter.instruction("b __rt_ftoa");                                 // format the float, returning straight to the caller
        }
        Arch::X86_64 => {
            if enabled {
                emitter.instruction("ucomisd xmm0, xmm0");                      // a NAN is the only value unordered with itself
                emitter.instruction("jp __rt_ftoa_coerce_nan_linux_x86_64");    // only a NAN takes the warning path
                emitter.instruction("jmp __rt_ftoa");                           // ordered values format silently
                emitter.label("__rt_ftoa_coerce_nan_linux_x86_64");
                emitter.instruction("sub rsp, 8");                              // realign the stack for a System V call
                emitter.instruction("call __rt_warn_nan_coerced_string");       // report PHP 8.5's NAN-to-string coercion warning
                emitter.instruction("add rsp, 8");                              // release the realignment padding
            }
            emitter.instruction("jmp __rt_ftoa");                               // format the float, returning straight to the caller
        }
    }
}

/// Emits the inline "is this a NAN?" probe that guards a call to `__rt_warn_nan_coerced_bool`,
/// followed by `skip_label` itself.
///
/// Emits NOTHING — not even the label — on pre-8.5 profiles, so `--php-version 8.4` codegen is
/// byte-identical to what it was before the diagnostic existed. `skip_label` must therefore be
/// unique per call site but is never referenced by the caller.
///
/// The probe leaves the float in `d0` / `xmm0` untouched (the helper restores it), so the
/// caller's own zero-compare follows unchanged.
///
/// A NAN is the only value that compares UNORDERED with itself, which is what both
/// architectures' self-compare tests: AArch64 sets `V` on unordered (`b.vc` skips), x86_64 sets
/// `PF` on unordered (`jnp` skips).
pub fn emit_nan_bool_coercion_probe(emitter: &mut Emitter, skip_label: &str) {
    emit_probe(emitter, skip_label, false);
}

/// `emit_nan_bool_coercion_probe` for a LEAF runtime helper that has not built a frame.
///
/// `__rt_mixed_is_empty` returns through the link register it was entered with and never
/// touches the stack, so the plain probe would silently destroy its return address on AArch64
/// (`bl` overwrites `x30`) and misalign the stack on x86_64 (a leaf sits at
/// `rsp ≡ 8 (mod 16)`, but `call` requires `rsp ≡ 0` at the call instruction). This variant
/// brackets the call with a 16-byte save that fixes both.
pub fn emit_nan_bool_coercion_probe_leaf(emitter: &mut Emitter, skip_label: &str) {
    emit_probe(emitter, skip_label, true);
}

/// Shared body of the two probe entry points; `leaf` requests the link-register/alignment save.
fn emit_probe(emitter: &mut Emitter, skip_label: &str, leaf: bool) {
    if !nan_bool_coercion_warning_enabled() {
        return;
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d0, d0");                                 // a NAN is the only value unordered with itself
            emitter.instruction(&format!("b.vc {}", skip_label));               // ordered values coerce to bool silently
            if leaf {
                emitter.instruction("str x30, [sp, #-16]!");                    // preserve the leaf helper's return address across the diagnostic call
            }
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm0, xmm0");                          // a NAN is the only value unordered with itself
            emitter.instruction(&format!("jnp {}", skip_label));                // ordered values coerce to bool silently
            if leaf {
                emitter.instruction("sub rsp, 8");                              // realign the leaf helper's stack for a System V call
            }
        }
    }
    abi::emit_call_label(emitter, "__rt_warn_nan_coerced_bool");                // report PHP 8.5's NAN-to-bool coercion warning
    if leaf {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x30, [sp], #16");                      // restore the leaf helper's return address
            }
            Arch::X86_64 => {
                emitter.instruction("add rsp, 8");                              // release the realignment padding
            }
        }
    }
    emitter.label(skip_label);
}
