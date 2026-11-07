//! Purpose:
//! Emits the target-aware preserving wrapper for the per-site debug hook and the
//! main-exit dump call. The wrapper exists so a site hook can call Rust without
//! clobbering any caller-saved EIR value at the instruction boundary.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` for the wrapper.
//! - `crate::codegen::block_emit::emit_main_function()` for the dump.
//!
//! Key details:
//! - The wrapper saves x0-x17/q0-q7/NZCV on AArch64 and
//!   rax/rcx/rdx/rsi/rdi/r8-r11/xmm0-xmm15/flags on x86_64.
//! - It calls `elephc_debug_site_inner(site_id, fp)` from `crates/elephc-debug`.
//! - The wrapper is emitted only for `--debug`; no normal build pays for it.

use crate::codegen::emit::Emitter;
use crate::types::PhpType;
use crate::codegen_support::abi;
use crate::codegen_support::platform::Arch;

use super::context::FunctionContext;

/// Local assembly label of the register-preserving site wrapper.
pub(super) const SITE_HOOK: &str = "__elephc_debug_site";
/// Local assembly label of the preserving function-enter wrapper.
pub(super) const ENTER_HOOK: &str = "__elephc_debug_enter";
/// Local assembly label of the preserving function-exit wrapper.
pub(super) const EXIT_HOOK: &str = "__elephc_debug_exit";

/// Number of one-byte site flags reserved in BSS. Kept in sync with
/// `crates/elephc-debug`'s counter table.
pub(super) const MAX_SITES: usize = 1 << 16;

/// Module-private BSS symbol holding one breakpoint/trace flag per site.
pub(super) const SITE_FLAGS: &str = "elephc_debug_site_flags";

/// Emits the site, enter and exit preserving wrappers once per module.
pub(super) fn emit_wrappers(emitter: &mut Emitter) {
    let site = emitter.target.extern_symbol("elephc_debug_site_inner");
    let enter = emitter.target.extern_symbol("elephc_debug_enter_inner");
    let exit = emitter.target.extern_symbol("elephc_debug_exit_inner");
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_aarch64_wrapper(emitter, SITE_HOOK, &site);
            emit_aarch64_wrapper(emitter, ENTER_HOOK, &enter);
            emit_aarch64_wrapper(emitter, EXIT_HOOK, &exit);
        }
        Arch::X86_64 => {
            emit_x86_64_wrapper(emitter, SITE_HOOK, &site);
            emit_x86_64_wrapper(emitter, ENTER_HOOK, &enter);
            emit_x86_64_wrapper(emitter, EXIT_HOOK, &exit);
        }
    }
}

/// Emits the AArch64 wrapper body.
fn emit_aarch64_wrapper(emitter: &mut Emitter, label: &str, inner: &str) {
    emitter.raw("    .p2align 2");
    emitter.label_global(label);
    let mut asm = String::new();
    asm.push_str(
        "    stp x29, x30, [sp, #-16]!\n\
         \x20   mov x29, sp\n\
         \x20   sub sp, sp, #288\n",
    );
    for pair in 0..9 {
        let first = pair * 2;
        let second = first + 1;
        asm.push_str(&format!(
            "    stp x{first}, x{second}, [sp, #{first_step}]\n",
            first_step = pair * 16
        ));
    }
    for pair in 0..4 {
        let first = pair * 2;
        let second = first + 1;
        asm.push_str(&format!(
            "    stp q{first}, q{second}, [sp, #{offset}]\n",
            offset = 144 + pair * 32
        ));
    }
    asm.push_str(
        "    mrs x9, nzcv\n\
         \x20   str x9, [sp, #280]\n",
    );
    asm.push_str(&format!("    bl {inner}\n"));
    asm.push_str(
        "    ldr x9, [sp, #280]\n\
         \x20   msr nzcv, x9\n",
    );
    for pair in (0..4).rev() {
        let first = pair * 2;
        let second = first + 1;
        asm.push_str(&format!(
            "    ldp q{first}, q{second}, [sp, #{offset}]\n",
            offset = 144 + pair * 32
        ));
    }
    for pair in (0..9).rev() {
        let first = pair * 2;
        let second = first + 1;
        asm.push_str(&format!(
            "    ldp x{first}, x{second}, [sp, #{first_step}]\n",
            first_step = pair * 16
        ));
    }
    asm.push_str(
        "    add sp, sp, #288\n\
         \x20   ldp x29, x30, [sp], #16\n\
         \x20   ret\n",
    );
    emitter.raw(&asm);
}

/// Emits the x86_64 wrapper body.
fn emit_x86_64_wrapper(emitter: &mut Emitter, label: &str, inner: &str) {
    emitter.raw("    .p2align 4");
    emitter.label_global(label);
    let mut asm = String::new();
    asm.push_str(
        "    pushfq\n\
         \x20   push rbp\n\
         \x20   mov rbp, rsp\n\
         \x20   sub rsp, 344\n",
    );
    for (index, register) in [
        "rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11",
    ]
    .iter()
    .enumerate()
    {
        asm.push_str(&format!(
            "    mov QWORD PTR [rsp + {}], {register}\n",
            index * 8
        ));
    }
    for index in 0..16 {
        asm.push_str(&format!(
            "    movdqu XMMWORD PTR [rsp + {}], xmm{index}\n",
            80 + index * 16
        ));
    }
    asm.push_str(&format!("    call {inner}\n"));
    for index in (0..16).rev() {
        asm.push_str(&format!(
            "    movdqu xmm{index}, XMMWORD PTR [rsp + {}]\n",
            80 + index * 16
        ));
    }
    for (index, register) in [
        "rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11",
    ]
    .iter()
    .enumerate()
    .rev()
    {
        asm.push_str(&format!(
            "    mov {register}, QWORD PTR [rsp + {}]\n",
            index * 8
        ));
    }
    asm.push_str(
        "    add rsp, 344\n\
         \x20   pop rbp\n\
         \x20   popfq\n\
         \x20   ret\n",
    );
    emitter.raw(&asm);
}

/// Emits the site hook call with the compiler-assigned site id and the current
/// frame pointer. The wrapper preserves every caller-saved register.
pub(super) fn emit_site_call(ctx: &mut FunctionContext<'_>, site_id: usize) {
    let target = ctx.emitter.target;
    let id_arg = abi::int_arg_reg_name(target, 0);
    let frame_arg = abi::int_arg_reg_name(target, 1);
    // The wrapper restores the registers it sees at call entry, which are the
    // argument registers AFTER this setup. Preserve their pre-site values so a
    // caller-saved live value cannot be clobbered by the hook arguments.
    abi::emit_push_reg_pair(ctx.emitter, id_arg, frame_arg);
    let flags = target.extern_symbol(SITE_FLAGS);
    let skip = ctx.next_label("debug_site_skip");
    match target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", &flags);
            if site_id <= 4095 {
                ctx.emitter.instruction(&format!("ldrb w10, [x9, #{site_id}]")); // test this site's flag
            } else {
                abi::emit_load_int_immediate(ctx.emitter, "x10", site_id as i64);
                ctx.emitter.instruction("add x9, x9, x10");                     // address a far site flag
                ctx.emitter.instruction("ldrb w10, [x9]");                      // test this site's flag
            }
            ctx.emitter.instruction(&format!("cbz w10, {skip}"));               // dormant sites skip the wrapper
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r11", &flags);
            ctx.emitter.instruction(&format!(                                   // test this site's flag
                "cmpb BYTE PTR [r11 + {site_id}], 0"
            ));
            ctx.emitter.instruction(&format!("je {skip}"));                     // dormant sites skip the wrapper
        }
    }
    abi::emit_load_int_immediate(ctx.emitter, id_arg, site_id as i64);
    let frame = abi::frame_pointer_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, frame_arg, frame);
    let entry = SITE_HOOK.to_string();
    abi::emit_call_label(ctx.emitter, &entry);
    // Both the dormant path and the active path converge here so the preserved
    // argument registers are restored exactly once.
    ctx.emitter.label(&skip);
    abi::emit_pop_reg_pair(ctx.emitter, id_arg, frame_arg);
}

/// Calls the runtime boot hook with the BSS flag table, its fixed capacity and
/// the generated program's counter addresses.
///
/// `web_master` distinguishes the once-per-process `--web` entry stub from the
/// per-request handler prologue: a web server must decide worker/route sessions
/// from the master before `fork()`, never from a worker that shares a DAP fd with
/// its siblings.
pub(super) fn emit_boot_call(ctx: &mut FunctionContext<'_>, web_master: bool) {
    let target = ctx.emitter.target;
    let flags = target.extern_symbol(SITE_FLAGS);
    let table_arg = abi::int_arg_reg_name(target, 0);
    abi::emit_symbol_address(ctx.emitter, table_arg, &flags);
    let count_arg = abi::int_arg_reg_name(target, 1);
    abi::emit_load_int_immediate(ctx.emitter, count_arg, MAX_SITES as i64);
    // The debug runtime reports per-call allocation/free deltas. It must not
    // resolve `_gc_allocs`/`_gc_frees` itself: those names are legal only in the
    // emitted assembly, so the generated prologue hands over their addresses.
    let allocs_arg = abi::int_arg_reg_name(target, 2);
    abi::emit_symbol_address(ctx.emitter, allocs_arg, "_gc_allocs");
    let frees_arg = abi::int_arg_reg_name(target, 3);
    abi::emit_symbol_address(ctx.emitter, frees_arg, "_gc_frees");
    let web_arg = abi::int_arg_reg_name(target, 4);
    abi::emit_load_int_immediate(ctx.emitter, web_arg, i64::from(web_master));
    let boot = target.extern_symbol("elephc_debug_boot");
    abi::emit_call_label(ctx.emitter, &boot);
}

/// Emits the function-enter hook for the current activation.
pub(super) fn emit_enter_call(ctx: &mut FunctionContext<'_>) {
    let Some(function_id) = ctx.debug_function_id else {
        return;
    };
    let target = ctx.emitter.target;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 0), function_id as i64);
    let frame = abi::frame_pointer_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(target, 1), frame);
    abi::emit_call_label(ctx.emitter, ENTER_HOOK);
}

/// Emits the function-exit hook for the current activation.
///
/// The PHP return value is already live in the result registers when the
/// epilogue runs, so it must be preserved across the hook call. This mirrors
/// `frame::emit_instr_exit`, including the by-reference return representation.
pub(super) fn emit_exit_call(ctx: &mut FunctionContext<'_>) {
    let Some(function_id) = ctx.debug_function_id else {
        return;
    };
    let return_ty = if ctx.function.flags.by_ref_return {
        PhpType::Pointer(None)
    } else {
        ctx.function.return_php_type.codegen_repr()
    };
    let preserves_return = !matches!(return_ty, PhpType::Void | PhpType::Never);
    if preserves_return {
        super::frame::push_return_value(ctx, &return_ty);
    }
    let target = ctx.emitter.target;
    let stack_pointer = match target.arch {
        Arch::AArch64 => "sp",
        Arch::X86_64 => "rsp",
    };
    let tag_arg = abi::int_arg_reg_name(target, 2);
    let lo_arg = abi::int_arg_reg_name(target, 3);
    let hi_arg = abi::int_arg_reg_name(target, 4);
    if !preserves_return {
        abi::emit_load_int_immediate(ctx.emitter, tag_arg, -1);
        abi::emit_load_int_immediate(ctx.emitter, lo_arg, 0);
        abi::emit_load_int_immediate(ctx.emitter, hi_arg, 0);
    } else if return_ty == PhpType::TaggedScalar {
        abi::emit_load_from_address(ctx.emitter, lo_arg, stack_pointer, 0);
        abi::emit_load_from_address(ctx.emitter, tag_arg, stack_pointer, 8);
        abi::emit_load_int_immediate(ctx.emitter, hi_arg, 0);
    } else {
        abi::emit_load_from_address(ctx.emitter, lo_arg, stack_pointer, 0);
        if return_ty == PhpType::Str {
            abi::emit_load_from_address(ctx.emitter, hi_arg, stack_pointer, 8);
            abi::emit_load_int_immediate(ctx.emitter, tag_arg, 1);
        } else {
            abi::emit_load_int_immediate(ctx.emitter, tag_arg, return_value_tag(&return_ty));
            abi::emit_load_int_immediate(ctx.emitter, hi_arg, 0);
        }
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 0), function_id as i64);
    let frame = abi::frame_pointer_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(target, 1), frame);
    abi::emit_call_label(ctx.emitter, EXIT_HOOK);
    if preserves_return {
        super::frame::pop_return_value(ctx, &return_ty);
    }
}

/// Returns the runtime value tag for a non-tagged return representation.
fn return_value_tag(ty: &PhpType) -> i64 {
    match ty.codegen_repr() {
        PhpType::Int | PhpType::Narrow(_) => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool | PhpType::False => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => 7,
        PhpType::Void | PhpType::Never => 8,
        PhpType::Resource(_) => 9,
        PhpType::Callable => 10,
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => 13,
        PhpType::TaggedScalar => 0,
    }
}

/// Emits the main-exit dump call.
pub(super) fn emit_dump_call(ctx: &mut FunctionContext<'_>) {
    let entry = ctx.emitter.target.extern_symbol("elephc_debug_dump");
    abi::emit_call_label(ctx.emitter, &entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::codegen::{Emit, Instrumentation, WebIsolation};
    use std::collections::HashMap;
    use std::path::Path;

    /// Verifies the AArch64 wrapper saves the full caller-saved set, preserves
    /// NZCV, and calls the Rust inner hook.
    #[test]
    fn aarch64_wrapper_saves_caller_saved_state() {
        let mut emitter = Emitter::new(Target::parse("linux-aarch64").unwrap());
        emit_wrappers(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("stp x0, x1, [sp, #0]"), "{asm}");
        assert!(asm.contains("stp q0, q1, [sp, #144]"), "{asm}");
        assert!(asm.contains("stp q6, q7, [sp, #240]"), "{asm}");
        assert!(asm.contains("mrs x9, nzcv"), "{asm}");
        assert!(asm.contains("bl elephc_debug_site_inner"), "{asm}");
        assert!(asm.contains("msr nzcv, x9"), "{asm}");
    }

    /// Verifies the macOS AArch64 wrapper uses the Mach-O underscore prefix for
    /// the Rust hook symbol.
    #[test]
    fn macos_aarch64_wrapper_uses_macho_symbols() {
        let mut emitter = Emitter::new(Target::parse("macos-aarch64").unwrap());
        emit_wrappers(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("bl _elephc_debug_site_inner"), "{asm}");
        assert!(asm.contains("__elephc_debug_site:"), "{asm}");
    }

    /// Verifies the debug exit hook addresses the x86_64 stack pointer as `rsp`.
    ///
    /// The hook pushes the return value and reloads it after the call. A hardcoded
    /// `sp` assembles on AArch64 and fails every x86_64 debug build at `as` time.
    #[test]
    fn x86_64_exit_hook_uses_the_stack_pointer_name() {
        let target = Target::parse("linux-x86_64").unwrap();
        let module = crate::ir_lower::tests::lower_source_at_for_target(
            "<?php function addOne(int $value): int { return $value + 1; } echo addOne(1);",
            Path::new("main.php"),
            Path::new("."),
            target,
        );
        let asm = crate::codegen::generate_user_asm_from_ir_with_options(
            &module,
            false,
            false,
            Instrumentation::Off,
            false,
            true,
            false,
            false,
            Emit::Executable,
            &HashMap::new(),
            true,
            false,
            WebIsolation::Worker,
        )
        .expect("debug assembly");
        assert!(!asm.contains("QWORD PTR [sp"), "{asm}");
        assert!(asm.contains("QWORD PTR [rsp"), "{asm}");
    }

    /// Verifies the x86_64 wrapper saves the SysV caller-saved registers, XMM
    /// state and flags, then calls the Rust inner hook.
    #[test]
    fn x86_64_wrapper_saves_caller_saved_state() {
        let mut emitter = Emitter::new(Target::parse("linux-x86_64").unwrap());
        emit_wrappers(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("pushfq"), "{asm}");
        assert!(asm.contains("mov QWORD PTR [rsp + 0], rax"), "{asm}");
        assert!(asm.contains("movdqu XMMWORD PTR [rsp + 80], xmm0"), "{asm}");
        assert!(asm.contains("movdqu XMMWORD PTR [rsp + 320], xmm15"), "{asm}");
        assert!(asm.contains("call elephc_debug_site_inner"), "{asm}");
        assert!(asm.contains("popfq"), "{asm}");
    }
}
