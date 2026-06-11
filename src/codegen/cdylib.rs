//! Purpose:
//! Emits the cdylib-only assembly fragments: C-ABI trampolines that expose
//! `#[Export]`-marked PHP functions under their unmangled names, plus the
//! `elephc_init` / `elephc_shutdown` / `elephc_last_error` / `elephc_free`
//! lifecycle entry points the embedding host calls before/after exports.
//!
//! Called from:
//! - `crate::codegen::generate_user_asm()` when `emit == Emit::Cdylib`.
//!
//! Key details:
//! - elephc's internal calling convention already routes integer/scalar params
//!   through the same SysV/AAPCS integer-arg registers C callers populate, and
//!   PHP `Str` params arrive as a `(ptr, len)` pair in two consecutive integer
//!   registers — exactly what a C caller passing `const char*, size_t` produces.
//!   That alignment means the trampoline can be a single tail-branch into the
//!   internal `_fn_<name>` symbol for every signature in the v1 scalar set.
//! - Lifecycle exports are v1 stubs: the runtime object pulled in by the
//!   compiled artifact uses BSS-zero-init for allocator state, so `elephc_init`
//!   reports success without additional work. `elephc_free` is a no-op until
//!   string-return marshaling lands and gives the host elephc-owned pointers
//!   to release.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::{Arch, Target};
use crate::exports::ExportedFunction;
use crate::names::function_symbol;

/// Emits a `.globl <c_name>` trampoline for every exported function and the
/// four lifecycle symbols. Called once after user function bodies have been
/// emitted, so the internal `_fn_<name>` targets already exist.
pub(super) fn emit_cdylib_exports(
    emitter: &mut Emitter,
    target: Target,
    exports: &[&ExportedFunction],
) {
    for export in exports {
        emit_export_trampoline(emitter, target, &export.name);
    }
    emit_lifecycle_exports(emitter, target);
}

/// Emits a single `#[Export]` trampoline. The exported symbol receives C-ABI
/// arguments in the standard SysV / AAPCS registers; we forward them unchanged
/// to the internal elephc function symbol with a tail-branch so the internal
/// function's `ret` returns directly to the C caller.
fn emit_export_trampoline(emitter: &mut Emitter, target: Target, php_name: &str) {
    let internal = function_symbol(php_name);
    let exported = target.extern_symbol(php_name);
    emitter.blank();
    emitter.comment(&format!("#[Export] trampoline for PHP function {}", php_name));
    emitter.label_global(&exported);
    emit_tail_branch(emitter, target, &internal);
}

/// Emits the four C-callable lifecycle symbols required for a v1 cdylib host
/// integration. None of them need a stack frame: `elephc_init` returns 0
/// (success), `elephc_shutdown` and `elephc_free` are nullary returns, and
/// `elephc_last_error` returns NULL (no error tracked yet).
fn emit_lifecycle_exports(emitter: &mut Emitter, target: Target) {
    emit_zero_returning_export(emitter, target, "elephc_init", "lifecycle: heap+globals (v1: no-op, BSS-init)");
    emit_void_export(emitter, target, "elephc_shutdown", "lifecycle: teardown (v1: no-op)");
    emit_zero_returning_export(emitter, target, "elephc_last_error", "lifecycle: returns NULL (v1: no error channel)");
    emit_void_export(emitter, target, "elephc_free", "lifecycle: free host-returned pointer (v1: no-op)");
}

/// Emits a `.globl <name>` symbol that returns immediately with the integer
/// return register cleared to zero. Used for `elephc_init` (returns 0 = success)
/// and `elephc_last_error` (returns NULL).
fn emit_zero_returning_export(
    emitter: &mut Emitter,
    target: Target,
    c_name: &str,
    comment: &str,
) {
    let symbol = target.extern_symbol(c_name);
    emitter.blank();
    emitter.comment(comment);
    emitter.label_global(&symbol);
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");
            emitter.instruction("ret");
        }
    }
}

/// Emits a `.globl <name>` symbol that returns immediately. Used for
/// `elephc_shutdown` and `elephc_free` whose return values are `void` /
/// ignored by the C caller.
fn emit_void_export(emitter: &mut Emitter, target: Target, c_name: &str, comment: &str) {
    let symbol = target.extern_symbol(c_name);
    emitter.blank();
    emitter.comment(comment);
    emitter.label_global(&symbol);
    match target.arch {
        Arch::AArch64 => emitter.instruction("ret"),
        Arch::X86_64 => emitter.instruction("ret"),
    }
}

/// Emits a tail-call (unconditional jump) to `target_symbol`. On AArch64 this
/// is `b <symbol>`; on x86_64 it is `jmp <symbol>`. The callee's `ret` returns
/// directly to whoever invoked the trampoline.
fn emit_tail_branch(emitter: &mut Emitter, target: Target, target_symbol: &str) {
    match target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", target_symbol)),
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", target_symbol)),
    }
}
