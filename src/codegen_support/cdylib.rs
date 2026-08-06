//! Purpose:
//! Emits the cdylib-only assembly fragments: C-ABI trampolines that expose
//! `#[Export]`-marked PHP functions under their unmangled names, plus the
//! `elephc_init` / `elephc_shutdown` / `elephc_last_error` / `elephc_free`
//! lifecycle entry points the embedding host calls before/after exports.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when `emit == Emit::Cdylib`.
//!
//! Key details:
//! - elephc's internal calling convention already routes integer/scalar params
//!   through the same SysV/AAPCS integer-arg registers C callers populate, and
//!   PHP `Str` params arrive as a `(ptr, len)` pair in two consecutive integer
//!   registers — exactly what a C caller passing `const char*, size_t` produces.
//!   That alignment means the trampoline can be a single tail-branch into the
//!   internal `_fn_<name>` symbol for every scalar signature.
//! - String *returns* are the exception: they need the result moved out of
//!   elephc's internal register pair into the platform's aggregate-return
//!   registers, and the in-band null sentinel translated to a real C `NULL`, so
//!   they get a framed trampoline instead of a tail-branch.
//! - `elephc_init`, `elephc_shutdown` and `elephc_last_error` remain stubs: the
//!   runtime object uses BSS-zero-init for allocator state, so `elephc_init`
//!   reports success without additional work. `elephc_free` is real — it
//!   releases the owned heap block a string-returning export handed the host.

use crate::codegen_support::abi::emit_load_int_immediate;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Target};
use crate::codegen_support::sentinels::NULL_SENTINEL;
use crate::exports::ExportedFunction;
use crate::names::function_symbol;
use crate::types::PhpType;

/// Emits a `.globl <c_name>` trampoline for every exported function and the
/// four lifecycle symbols. Called once after user function bodies have been
/// emitted, so the internal `_fn_<name>` targets already exist.
pub(crate) fn emit_cdylib_exports(
    emitter: &mut Emitter,
    target: Target,
    exports: &[&ExportedFunction],
) {
    for export in exports {
        let returns_string = matches!(export.sig.return_type, PhpType::Str);
        emit_export_trampoline(emitter, target, &export.name, returns_string);
    }
    emit_lifecycle_exports(emitter, target);
}

/// Emits a single `#[Export]` trampoline. The exported symbol receives C-ABI
/// arguments in the standard SysV / AAPCS registers; we forward them unchanged
/// to the internal elephc function symbol.
///
/// Scalar returns tail-branch, so the internal function's `ret` returns straight
/// to the C caller. String returns cannot: they need the result marshaled out of
/// elephc's internal register pair and the null sentinel translated, so they get
/// a real frame (see `emit_string_return_trampoline`).
fn emit_export_trampoline(
    emitter: &mut Emitter,
    target: Target,
    php_name: &str,
    returns_string: bool,
) {
    let internal = function_symbol(php_name);
    let exported = target.extern_symbol(php_name);
    emitter.blank();
    emitter.comment(&format!("#[Export] trampoline for PHP function {}", php_name));
    emitter.label_global(&exported);
    if returns_string {
        emit_string_return_trampoline(emitter, target, &internal);
    } else {
        emit_tail_branch(emitter, target, &internal);
    }
}

/// Emits the trampoline body for an export returning a PHP string, handing the C
/// caller a `(ptr, len)` pair it owns and releases through `elephc_free`.
///
/// Two things make this more than a tail-branch:
/// - **Register placement.** elephc returns strings in `string_result_regs`
///   (`x1`/`x2` on AArch64, `rax`/`rdx` on x86_64). On x86_64 that already *is*
///   the SysV convention for a 16-byte two-INTEGER-member struct return, so the
///   pair needs no move. On AArch64 it is not: AAPCS64 returns such an aggregate
///   in `x0`/`x1`, so the pair must be shifted down one register. Tail-branching
///   here would hand the host whatever `x0` happened to hold as the pointer.
/// - **Null sentinel.** A missing string is the in-band `NULL_SENTINEL`, not a
///   null pointer. Leaking it across the boundary would turn any host deref — or
///   the matching `elephc_free` — into a wild access, so it is translated to a
///   real C `NULL` with a zero length. The translation is branchless (`csel` /
///   `cmov`) to keep the trampoline free of local labels.
///
/// The returned buffer is always an owned heap block: lowering persists every
/// string returned from an exported function (`persist_scratch_return_string`).
fn emit_string_return_trampoline(emitter: &mut Emitter, target: Target, internal: &str) {
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // this trampoline calls, so it needs a real frame
            emitter.instruction("mov x29, sp");                                 // establish the frame pointer
            emitter.instruction(&format!("bl {}", internal));                   // x1 = string pointer, x2 = length
            emit_load_int_immediate(emitter, "x9", NULL_SENTINEL);              // x9 = the in-band "no string" marker
            emitter.instruction("cmp x1, x9");                                  // did the body return the sentinel?
            emitter.instruction("csel x1, xzr, x1, eq");                        // sentinel becomes a real C NULL pointer
            emitter.instruction("csel x2, xzr, x2, eq");                        // ...with a zero length to match
            emitter.instruction("mov x0, x1");                                  // AAPCS64 returns the aggregate in x0/x1
            emitter.instruction("mov x1, x2");                                  // shift the length down into the second result register
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore the frame
            emitter.instruction("ret");                                         // return the (ptr, len) pair to the host
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // this trampoline calls, so it needs a real frame
            emitter.instruction("mov rbp, rsp");                                // establish the frame pointer
            emitter.instruction(&format!("call {}", internal));                 // rax = string pointer, rdx = length
            emitter.instruction("xor r11d, r11d");                              // zero source for the cmovs (sets flags, so it precedes the cmp)
            emit_load_int_immediate(emitter, "r10", NULL_SENTINEL);             // r10 = the in-band "no string" marker
            emitter.instruction("cmp rax, r10");                                // did the body return the sentinel?
            emitter.instruction("cmove rax, r11");                              // sentinel becomes a real C NULL pointer
            emitter.instruction("cmove rdx, r11");                              // ...with a zero length to match
            emitter.instruction("pop rbp");                                     // restore the frame
            emitter.instruction("ret");                                         // rax/rdx already is the SysV 16-byte struct return
        }
    }
}

/// Emits the four C-callable lifecycle symbols required for a v1 cdylib host
/// integration. None of them need a stack frame: `elephc_init` returns 0
/// (success), `elephc_shutdown` and `elephc_free` are nullary returns, and
/// `elephc_last_error` returns NULL (no error tracked yet).
fn emit_lifecycle_exports(emitter: &mut Emitter, target: Target) {
    emit_zero_returning_export(emitter, target, "elephc_init", "lifecycle: heap+globals (v1: no-op, BSS-init)");
    emit_void_export(emitter, target, "elephc_shutdown", "lifecycle: teardown (v1: no-op)");
    emit_zero_returning_export(emitter, target, "elephc_last_error", "lifecycle: returns NULL (v1: no error channel)");
    emit_free_export(emitter, target);
}

/// Emits `elephc_free`, releasing a string buffer an export handed the host.
///
/// Routes to `__rt_heap_free_safe` rather than `__rt_heap_free`: the safe form
/// validates that the pointer really is a live block inside elephc's heap before
/// touching the free list, so a host passing back a null pointer, a stale one, or
/// something elephc never owned is a no-op instead of heap corruption.
///
/// The argument registers do not line up the same way on both architectures.
/// AArch64 takes the C first argument in `x0`, which is exactly the helper's
/// input register, so the call tail-branches unchanged. SysV x86_64 delivers it
/// in `rdi` while the helper reads `rax`, so it has to be moved first — a tail
/// `jmp` without that move would free whatever `rax` happened to hold.
fn emit_free_export(emitter: &mut Emitter, target: Target) {
    let symbol = target.extern_symbol("elephc_free");
    emitter.blank();
    emitter.comment("lifecycle: release a string buffer returned to the host");
    emitter.label_global(&symbol);
    if target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // SysV passes the pointer in rdi; the helper reads rax
    }
    emit_tail_branch(emitter, target, "__rt_heap_free_safe");                   // validates liveness, then frees; null-safe
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
            emitter.instruction("mov x0, #0");                                  // return success or NULL through the C integer result register
            emitter.instruction("ret");                                         // return directly to the embedding host
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // return success or NULL through the C integer result register
            emitter.instruction("ret");                                         // return directly to the embedding host
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
        Arch::AArch64 => emitter.instruction("ret"),                            // return directly to the embedding host
        Arch::X86_64 => emitter.instruction("ret"),                             // return directly to the embedding host
    }
}

/// Emits a tail-call (unconditional jump) to `target_symbol`. On AArch64 this
/// is `b <symbol>`; on x86_64 it is `jmp <symbol>`. The callee's `ret` returns
/// directly to whoever invoked the trampoline.
fn emit_tail_branch(emitter: &mut Emitter, target: Target, target_symbol: &str) {
    match target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", target_symbol)),  // tail-call the internal PHP function body
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", target_symbol)), // tail-call the internal PHP function body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Platform;

    /// Renders the string-return trampoline body for one architecture, on the
    /// platform that architecture is actually built for.
    fn render_string_return(arch: Arch) -> String {
        let target = match arch {
            Arch::AArch64 => Target::new(Platform::MacOS, Arch::AArch64),
            Arch::X86_64 => Target::new(Platform::Linux, Arch::X86_64),
        };
        let mut emitter = Emitter::new(target);
        emit_string_return_trampoline(&mut emitter, target, "_fn_greet");
        emitter.output()
    }

    /// Renders `elephc_free` for one architecture.
    fn render_free(arch: Arch) -> String {
        let target = match arch {
            Arch::AArch64 => Target::new(Platform::MacOS, Arch::AArch64),
            Arch::X86_64 => Target::new(Platform::Linux, Arch::X86_64),
        };
        let mut emitter = Emitter::new(target);
        emit_free_export(&mut emitter, target);
        emitter.output()
    }

    /// AAPCS64 returns a 16-byte aggregate in x0/x1 while elephc produces the
    /// string pair in x1/x2, so the trampoline must shift both registers down.
    /// Getting this wrong hands the host whatever x0 happened to hold as the
    /// pointer, which is the failure this asserts against.
    #[test]
    fn aarch64_string_return_shifts_pair_into_aapcs_result_registers() {
        let asm = render_string_return(Arch::AArch64);
        let ptr = asm.find("mov x0, x1").expect("pointer must move x1 -> x0");
        let len = asm.find("mov x1, x2").expect("length must move x2 -> x1");
        assert!(
            ptr < len,
            "the pointer move must precede the length move, or x1 is clobbered first:\n{}",
            asm
        );
        assert!(asm.contains("bl _fn_greet"), "must call, not tail-branch:\n{}", asm);
    }

    /// x86_64 needs no shift: rax/rdx already is the SysV convention for a
    /// two-INTEGER-member 16-byte struct return. Asserting the absence of a
    /// move guards against "fixing" both arches symmetrically.
    #[test]
    fn x86_64_string_return_leaves_the_sysv_pair_in_place() {
        let asm = render_string_return(Arch::X86_64);
        assert!(asm.contains("call _fn_greet"), "must call, not tail-branch:\n{}", asm);
        assert!(
            !asm.contains("mov rax, rdx"),
            "rax/rdx is already the SysV struct return; no shift belongs here:\n{}",
            asm
        );
    }

    /// Both architectures must translate the in-band null sentinel into a real
    /// C NULL with a zero length, or a host deref crashes on a wild address.
    #[test]
    fn string_return_translates_the_null_sentinel_on_both_arches() {
        let aarch64 = render_string_return(Arch::AArch64);
        assert!(aarch64.contains("csel x1, xzr, x1, eq"), "pointer:\n{}", aarch64);
        assert!(aarch64.contains("csel x2, xzr, x2, eq"), "length:\n{}", aarch64);

        let x86_64 = render_string_return(Arch::X86_64);
        assert!(x86_64.contains("cmove rax, r11"), "pointer:\n{}", x86_64);
        assert!(x86_64.contains("cmove rdx, r11"), "length:\n{}", x86_64);
        let zero = x86_64.find("xor r11d, r11d").expect("cmov needs a zeroed source");
        let cmp = x86_64.find("cmp rax, r10").expect("sentinel comparison");
        assert!(
            zero < cmp,
            "zeroing sets flags, so it must precede the comparison:\n{}",
            x86_64
        );
    }

    /// SysV delivers `elephc_free`'s argument in rdi while
    /// `__rt_heap_free_safe` reads rax, so the move is mandatory on x86_64 and
    /// must come before the tail jump. AArch64 passes it in x0, which is
    /// already the helper's input register, so it must NOT move.
    #[test]
    fn free_bridges_the_c_argument_register_only_where_it_differs() {
        let x86_64 = render_free(Arch::X86_64);
        let mov = x86_64.find("mov rax, rdi").expect("rdi -> rax bridge is required");
        let jmp = x86_64
            .find("jmp __rt_heap_free_safe")
            .expect("must tail-jump into the safe free helper");
        assert!(mov < jmp, "the move must precede the jump:\n{}", x86_64);

        let aarch64 = render_free(Arch::AArch64);
        assert!(
            aarch64.contains("b __rt_heap_free_safe"),
            "must tail-branch into the safe free helper:\n{}",
            aarch64
        );
        assert!(
            !aarch64.contains("mov x0,"),
            "x0 is already the helper's input register; no move belongs here:\n{}",
            aarch64
        );
    }
}
