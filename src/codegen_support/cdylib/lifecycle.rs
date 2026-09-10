//! Purpose:
//! Emits library lifecycle, status, diagnostic, and owned-buffer C entry points.
//!
//! Called from:
//! - The library export orchestrator after scalar and string wrappers.
//!
//! Key details:
//! - Initialization preserves native linkage and returns optional startup failures through the C ABI.
//! - Repeated initialization does not reset the mbstring request settings used by exported PHP calls.

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
        if lifecycle == "elephc_init" {
            abi::emit_frame_prologue(emitter, 16);
        }
        emit_clear_error_inline(emitter);
        emit_reset_concat_inline(emitter);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_ACTIVE, 0);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
        if lifecycle == "elephc_init" {
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
            abi::emit_frame_restore(emitter, 16);
        }
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
    match target.arch {
        Arch::AArch64 => emitter.instruction("b __rt_heap_free_safe"),          // release non-borrowed runtime storage when present
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // adapt the SysV pointer register to the runtime free ABI
            emitter.instruction("jmp __rt_heap_free_safe");                     // release non-borrowed runtime storage when present
        }
    }
}
