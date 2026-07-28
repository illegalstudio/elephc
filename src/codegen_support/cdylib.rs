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
//! - SysV/AAPCS exports can tail-branch because their scalar register routing
//!   matches elephc's internal ABI. Windows x86_64 instead needs an adapter:
//!   MSx64 shares four positional integer/float slots while elephc tracks both
//!   register classes independently, and C strings are flattened pointer/length
//!   parameters that may straddle the register/stack boundary.
//! - Lifecycle exports are v1 stubs: the runtime object pulled in by the
//!   compiled artifact uses BSS-zero-init for allocator state, so `elephc_init`
//!   reports success without additional work. `elephc_free` is a no-op until
//!   string-return marshaling lands and gives the host elephc-owned pointers
//!   to release.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform, Target};
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
        emit_export_trampoline(emitter, target, export);
    }
    emit_lifecycle_exports(emitter, target);
    if target.platform == Platform::Windows {
        emit_windows_cdylib_entry_stub(emitter);
        emit_windows_export_directives(emitter, target, exports);
    }
}

/// Emits a single `#[Export]` trampoline. The exported symbol receives C-ABI
/// arguments in the standard SysV / AAPCS registers; we forward them unchanged
/// to the internal elephc function symbol with a tail-branch so the internal
/// function's `ret` returns directly to the C caller.
fn emit_export_trampoline(emitter: &mut Emitter, target: Target, export: &ExportedFunction) {
    let internal = function_symbol(&export.name);
    let exported = target.extern_symbol(&export.name);
    emitter.blank();
    emitter.comment(&format!("#[Export] trampoline for PHP function {}", export.name));
    emitter.label_global(&exported);
    if (target.platform, target.arch) == (Platform::Windows, Arch::X86_64) {
        emit_windows_x86_64_export_adapter(emitter, target, export, &internal);
    } else {
        emit_tail_branch(emitter, target, &internal);
    }
}

/// Adapts the positional MSx64 C register file to elephc's internal split
/// integer/float register files, including flattened `(pointer, length)` string
/// arguments and both ABIs' stack-overflow areas.
fn emit_windows_x86_64_export_adapter(
    emitter: &mut Emitter,
    target: Target,
    export: &ExportedFunction,
    internal: &str,
) {
    let arg_types: Vec<PhpType> = export
        .sig
        .params
        .iter()
        .map(|(_, ty)| ty.codegen_repr())
        .collect();
    let internal_assignments =
        abi::build_outgoing_arg_assignments_for_target(target, &arg_types, 0);
    let flattened_units: usize = arg_types.iter().map(PhpType::register_count).sum();
    let arg_spill_bytes = flattened_units * 8;
    let xmm_save_base = align16(arg_spill_bytes + 32);
    let local_bytes = xmm_save_base + 9 * 16;

    emitter.instruction("push rbp");                                            // preserve the C caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable access to incoming C stack arguments
    emitter.instruction(&format!("sub rsp, {}", local_bytes));                  // reserve aligned spill slots for every flattened C argument
    abi::store_at_offset(emitter, "rdi", arg_spill_bytes + 8);
    abi::store_at_offset(emitter, "rsi", arg_spill_bytes + 16);
    for xmm_index in 6..=15 {
        let offset = xmm_save_base + (xmm_index - 6) * 16;
        emitter.instruction(&format!(                                           // preserve an MSx64 nonvolatile vector register across the internal call
            "movdqu XMMWORD PTR [rbp - {}], xmm{}",
            offset, xmm_index
        ));
    }

    let mut c_slot = 0usize;
    let mut local_unit = 0usize;
    for ty in &arg_types {
        match ty {
            PhpType::Float => {
                spill_windows_c_unit(emitter, true, c_slot, local_unit);
                c_slot += 1;
                local_unit += 1;
            }
            PhpType::Str | PhpType::TaggedScalar => {
                spill_windows_c_unit(emitter, false, c_slot, local_unit);
                spill_windows_c_unit(emitter, false, c_slot + 1, local_unit + 1);
                c_slot += 2;
                local_unit += 2;
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                spill_windows_c_unit(emitter, false, c_slot, local_unit);
                c_slot += 1;
                local_unit += 1;
            }
        }
    }

    let overflow_count = internal_assignments
        .iter()
        .filter(|assignment| !assignment.in_register())
        .count();
    let outgoing_bytes = 32 + overflow_count * 16;
    emitter.instruction(&format!("sub rsp, {}", outgoing_bytes));               // reserve MSx64 shadow space plus internal overflow slots

    local_unit = 0;
    let mut overflow_offset = 32usize;
    for (ty, assignment) in arg_types.iter().zip(internal_assignments.iter()) {
        if assignment.in_register() {
            match ty {
                PhpType::Float => {
                    let dst = abi::float_arg_reg_name(target, assignment.start_reg);
                    abi::load_at_offset(emitter, dst, export_spill_offset(local_unit));
                }
                PhpType::Str | PhpType::TaggedScalar => {
                    let low = abi::int_arg_reg_name(target, assignment.start_reg);
                    let high = abi::int_arg_reg_name(target, assignment.start_reg + 1);
                    abi::load_at_offset(emitter, low, export_spill_offset(local_unit));
                    abi::load_at_offset(emitter, high, export_spill_offset(local_unit + 1));
                }
                PhpType::Void | PhpType::Never => {}
                _ => {
                    let dst = abi::int_arg_reg_name(target, assignment.start_reg);
                    abi::load_at_offset(emitter, dst, export_spill_offset(local_unit));
                }
            }
        } else {
            match ty {
                PhpType::Float => {
                    abi::load_at_offset(emitter, "xmm15", export_spill_offset(local_unit));
                    abi::emit_store_to_sp(emitter, "xmm15", overflow_offset);
                }
                PhpType::Str | PhpType::TaggedScalar => {
                    abi::load_at_offset(emitter, "r10", export_spill_offset(local_unit));
                    abi::load_at_offset(emitter, "r11", export_spill_offset(local_unit + 1));
                    abi::emit_store_to_sp(emitter, "r10", overflow_offset);
                    abi::emit_store_to_sp(emitter, "r11", overflow_offset + 8);
                }
                PhpType::Void | PhpType::Never => {}
                _ => {
                    abi::load_at_offset(emitter, "r10", export_spill_offset(local_unit));
                    abi::emit_store_to_sp(emitter, "r10", overflow_offset);
                }
            }
            overflow_offset += 16;
        }
        local_unit += ty.register_count();
    }

    emitter.instruction(&format!("call {}", internal));                         // invoke the internal PHP body with elephc's ABI
    emitter.instruction(&format!("add rsp, {}", outgoing_bytes));               // release shadow space and internal overflow slots
    for xmm_index in (6..=15).rev() {
        let offset = xmm_save_base + (xmm_index - 6) * 16;
        emitter.instruction(&format!(                                           // restore an MSx64 nonvolatile vector register after the internal call
            "movdqu xmm{}, XMMWORD PTR [rbp - {}]",
            xmm_index, offset
        ));
    }
    abi::load_at_offset(emitter, "rsi", arg_spill_bytes + 16);
    abi::load_at_offset(emitter, "rdi", arg_spill_bytes + 8);
    emitter.instruction(&format!("add rsp, {}", local_bytes));                  // release flattened C argument spill slots
    emitter.instruction("pop rbp");                                             // restore the C caller's frame pointer
    emitter.instruction("ret");                                                 // return the scalar result through the matching C result register
}

/// Spills one flattened MSx64 C argument unit into a stable local slot.
fn spill_windows_c_unit(emitter: &mut Emitter, is_float: bool, c_slot: usize, local_unit: usize) {
    let local_offset = export_spill_offset(local_unit);
    if c_slot < 4 {
        let reg = if is_float {
            abi::float_arg_reg_name(emitter.target, c_slot)
        } else {
            abi::int_arg_reg_name(emitter.target, c_slot)
        };
        abi::store_at_offset(emitter, reg, local_offset);
    } else {
        let scratch = if is_float { "xmm15" } else { "r10" };
        abi::load_from_caller_stack(emitter, scratch, 48 + (c_slot - 4) * 8);
        abi::store_at_offset(emitter, scratch, local_offset);
    }
}

/// Returns the rbp-relative local offset for one flattened export argument unit.
fn export_spill_offset(unit: usize) -> usize {
    (unit + 1) * 8
}

/// Emits PE linker directives that export only the public API symbols and keep
/// internal runtime globals out of the DLL export table.
fn emit_windows_export_directives(
    emitter: &mut Emitter,
    target: Target,
    exports: &[&ExportedFunction],
) {
    emitter.blank();
    emitter.raw(".section .drectve");
    for export in exports {
        emitter.raw(&format!(
            ".ascii \" -export:{}\"",
            target.extern_symbol(&export.name)
        ));
    }
    for name in [
        "elephc_init",
        "elephc_shutdown",
        "elephc_last_error",
        "elephc_free",
    ] {
        emitter.raw(&format!(
            ".ascii \" -export:{}\"",
            target.extern_symbol(name)
        ));
    }
}

/// Satisfies the runtime object's executable-entry shim when linking it into a
/// DLL. A cdylib has no PHP top-level entry body, and this private zero-returning
/// symbol is deliberately absent from the PE export directives.
fn emit_windows_cdylib_entry_stub(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("private cdylib entry stub for the shared Windows runtime object");
    emitter.label_global(emitter.entry_symbol());
    emitter.instruction("xor eax, eax");                                        // report a no-op top-level body if the unused CRT shim is reached
    emitter.instruction("ret");                                                 // return from the private DLL-only entry stub
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

/// Rounds a byte count up to the stack's required sixteen-byte alignment.
fn align16(bytes: usize) -> usize {
    (bytes + 15) & !15
}
