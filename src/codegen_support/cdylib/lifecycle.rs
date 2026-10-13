//! Purpose:
//! Emits library lifecycle, status, diagnostic, and owned-buffer C entry points.
//!
//! Called from:
//! - The library export orchestrator after scalar and string wrappers.
//!
//! Key details:
//! - Initialization preserves native linkage and returns optional startup failures through the C ABI.
//! - Repeated initialization does not reset the mbstring request settings used by exported PHP calls.
//! - Foreign entries publish the runtime context before accessing it and restore the host register.

use super::*;

/// Emits ABI version, lifecycle, last-status, last-error, and owned-buffer release exports.
pub(super) fn emit(emitter: &mut Emitter, target: Target, heap_debug: bool,
    startup: Option<&str>, runtime_error: (&str, usize)) {
    emitter.blank();
    emitter.comment("cdylib ABI version");
    emitter.label_global(&target.extern_symbol("elephc_abi_version"));
    match target.arch {
        Arch::AArch64 => emitter.instruction(&format!("mov w0, #{ELEPHC_ABI_VERSION}")), // return the ABI version declared by the generated header
        Arch::X86_64 => emitter.instruction(&format!("mov eax, {ELEPHC_ABI_VERSION}")), // return the ABI version declared by the generated header
    }
    emitter.instruction("ret");                                                 // return to the current C-ABI caller

    for lifecycle in ["elephc_init", "elephc_shutdown"] {
        emitter.blank();
        emitter.comment(&format!("cdylib lifecycle: {lifecycle}"));
        emitter.label_global(&target.extern_symbol(lifecycle));
        abi::emit_frame_prologue(emitter, 32);
        crate::codegen_support::runtime::ctx::emit_ctx_save_foreign(emitter, 8);
        crate::codegen_support::runtime::ctx::emit_ctx_publish(emitter);
        emit_clear_error_inline(emitter);
        emit_reset_concat_inline(emitter);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_ACTIVE, 0);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
        if lifecycle == "elephc_init" {
            crate::codegen_support::runtime::ctx::emit_lazy_ctx_arena_install(
                emitter, "L_cdylib_init_arena_ready",
            );
            crate::codegen::stack_guard::emit_stack_limit_init_call(emitter);
            if heap_debug {
                abi::emit_enable_heap_debug_flag(emitter);
            }
            super::emit_startup_check(emitter, startup, "L_cdylib_init_failed");
            match target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("mov w0, #{STATUS_OK}"));      // return successful runtime initialization
                }
                Arch::X86_64 => emitter.instruction(&format!("mov eax, {STATUS_OK}")), // return successful runtime initialization
            }
        }
        if lifecycle == "elephc_init" {
            if startup.is_some() {
                emitter.instruction(if target.arch == Arch::AArch64 { "b L_cdylib_init_return" } else { "jmp L_cdylib_init_return" }); // preserve successful initialization status
                emitter.label("L_cdylib_init_failed");
                match target.arch {
                    Arch::AArch64 => emit_set_static_error_aarch64(emitter, runtime_error),
                    Arch::X86_64 => emit_set_static_error_x86_64(emitter, runtime_error),
                }
                emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_RUNTIME_FAILURE as i64);
                let failure = match target.arch {
                    Arch::AArch64 => format!("mov w0, #{STATUS_RUNTIME_FAILURE}"),
                    Arch::X86_64 => format!("mov eax, {STATUS_RUNTIME_FAILURE}"),
                };
                emitter.instruction(&failure);                                  // return recoverable runtime failure to the host initializer
                emitter.label("L_cdylib_init_return");
            }
        }
        crate::codegen_support::runtime::ctx::emit_ctx_restore_foreign(emitter, 8);
        abi::emit_frame_restore(emitter, 32);
        emitter.instruction("ret");                                             // return to the current C-ABI caller
    }

    emitter.blank();
    emitter.comment("cdylib status of the most recent exported call");
    emitter.label_global(&target.extern_symbol("elephc_last_status"));
    match target.arch {
        Arch::AArch64 => abi::emit_load_symbol_to_reg(emitter, "x0", BOUNDARY_STATUS, 0),
        Arch::X86_64 => abi::emit_load_symbol_to_reg(emitter, "rax", BOUNDARY_STATUS, 0),
    }
    emitter.instruction("ret");                                                 // return the most recent recoverable boundary status

    emitter.blank();
    emitter.comment("cdylib borrowed last-error pointer");
    emitter.label_global(&target.extern_symbol("elephc_last_error"));
    match target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", LAST_ERROR_PRESENT, 0);
            emitter.instruction("cbz x9, L_cdylib_last_error_none_aarch64");    // return NULL only when no diagnostic is recorded
            abi::emit_symbol_address(emitter, "x0", LAST_ERROR_BUFFER);
            emitter.instruction("ret");                                         // return to the current C-ABI caller
            emitter.label("L_cdylib_last_error_none_aarch64");
            emitter.instruction("mov x0, #0");                                  // return a NULL last-error pointer
            emitter.instruction("ret");                                         // return to the current C-ABI caller
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", LAST_ERROR_PRESENT, 0);
            emitter.instruction("test r10, r10");                               // test whether a diagnostic is recorded
            emitter.instruction("je L_cdylib_last_error_none_x86_64");          // return NULL only when no diagnostic is recorded
            abi::emit_symbol_address(emitter, "rax", LAST_ERROR_BUFFER);
            emitter.instruction("ret");                                         // return to the current C-ABI caller
            emitter.label("L_cdylib_last_error_none_x86_64");
            emitter.instruction("xor eax, eax");                                // return a NULL last-error pointer
            emitter.instruction("ret");                                         // return to the current C-ABI caller
        }
    }

    emitter.blank();
    emitter.comment("cdylib release of caller-owned export storage");
    emitter.label_global(&target.extern_symbol("elephc_free"));
    abi::emit_frame_prologue(emitter, 32);
    crate::codegen_support::runtime::ctx::emit_ctx_save_foreign(emitter, 8);
    crate::codegen_support::runtime::ctx::emit_ctx_publish(emitter);
    if target.arch == Arch::X86_64 {
        abi::emit_reg_move(emitter, "rax", "rdi");
    }
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    crate::codegen_support::runtime::ctx::emit_ctx_restore_foreign(emitter, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Init, shutdown and free preserve the foreign context register on every supported target.
    #[test]
    fn lifecycle_entries_publish_context_after_save_and_restore_it_before_return() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new_cdylib(target);
            emitter.ctx_register = true;
            emit(&mut emitter, target, false, None, ("unused_runtime_error", 0));
            let asm = emitter.output();
            let (save, restore) = match target.arch {
                Arch::AArch64 => (
                    "stur x28, [x29, #-8]", "ldur x28, [x29, #-8]",
                ),
                Arch::X86_64 => (
                    "mov QWORD PTR [rbp - 8], r14",
                    "mov r14, QWORD PTR [rbp - 8]",
                ),
            };
            // Library entries use PIC/GOT materialization on ELF, ordinary addresses on
            // Mach-O. Check the ordering against the exact target-aware publish sequence.
            let mut publication = Emitter::new_cdylib(target);
            publication.ctx_register = true;
            crate::codegen_support::runtime::ctx::emit_ctx_publish(&mut publication);
            let publish = publication.output();
            for entry in ["elephc_init", "elephc_shutdown", "elephc_free"] {
                let label = format!("{}:\n", target.extern_symbol(entry));
                let body = asm.split_once(&label).unwrap().1.split_once("ret").unwrap().0;
                let saved = body.find(save).unwrap_or_else(|| panic!("{name}/{entry}: {body}"));
                let published = body.find(publish.trim()).unwrap_or_else(|| panic!("{name}/{entry}: {body}"));
                let restored = body.find(restore).unwrap_or_else(|| panic!("{name}/{entry}: {body}"));
                assert!(saved < published && published < restored, "{name}/{entry}: {body}");
            }
            assert!(asm.contains("L_cdylib_init_arena_ready"), "{name}: initialization must preserve a live arena");
        }
    }
}
