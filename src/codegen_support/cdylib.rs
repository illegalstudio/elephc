//! Purpose:
//! Emits the library C ABI boundary, including scalar trampolines, owned
//! binary-string marshaling, lifecycle functions, and recoverable diagnostics.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` for cdylib and staticlib artifacts.
//!
//! Key details:
//! - Scalar exports retain their original C signatures and recover failures through
//!   `elephc_last_status()` plus the shared diagnostic channel.
//! - String-return exports return status plus caller-owned output storage for every fixed input shape.
//! - Nested native boundaries isolate concat scratch state and restore their caller.

use crate::codegen_support::abi;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Target};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
};
use crate::exports::{is_string_return_signature, ExportedFunction, ELEPHC_ABI_VERSION};

mod boundary;
mod owned_string;

pub(crate) const STATUS_OK: i32 = 0;
pub(crate) const STATUS_INVALID_ARGUMENT: i32 = 1;
pub(crate) const STATUS_PHP_EXCEPTION: i32 = 2;
pub(crate) const STATUS_ALLOCATION_FAILURE: i32 = 3;
pub(crate) const STATUS_RUNTIME_FAILURE: i32 = 4;

pub(crate) const BOUNDARY_ACTIVE: &str = "_elephc_boundary_active";
pub(crate) const BOUNDARY_STATUS: &str = "_elephc_boundary_status";
const LAST_ERROR_BUFFER: &str = "_elephc_last_error_buffer";
const LAST_ERROR_LENGTH: &str = "_elephc_last_error_length";
const LAST_ERROR_PRESENT: &str = "_elephc_last_error_present";
const LAST_ERROR_CAPACITY: usize = 4096;
const CONCAT_SCRATCH_CAPACITY: i64 = 65_536;

/// Reserves the fixed, allocation-free state shared by every library export.
fn reserve_boundary_data(data: &mut DataSection) {
    data.add_comm(BOUNDARY_ACTIVE.to_string(), 8);
    data.add_comm(BOUNDARY_STATUS.to_string(), 8);
    data.add_comm(LAST_ERROR_PRESENT.to_string(), 8);
    data.add_comm(LAST_ERROR_LENGTH.to_string(), 8);
    data.add_comm(LAST_ERROR_BUFFER.to_string(), LAST_ERROR_CAPACITY);
}

/// Emits all public export trampolines and lifecycle/error helpers.
pub(crate) fn emit_cdylib_exports(
    emitter: &mut Emitter,
    data: &mut DataSection,
    target: Target,
    exports: &[&ExportedFunction],
    heap_debug: bool,
) {
    reserve_boundary_data(data);
    let (invalid_ptr, invalid_len) = data.add_string(b"invalid string export arguments");
    let (allocation_ptr, allocation_len) = data.add_string(b"elephc allocation failed");
    let (runtime_ptr, runtime_len) = data.add_string(b"elephc runtime boundary failed");

    emit_error_helpers(emitter, target);
    for export in exports {
        if is_string_return_signature(&export.sig) {
            owned_string::emit_owned_string_export(
                emitter,
                target,
                export,
                (&invalid_ptr, invalid_len),
                (&allocation_ptr, allocation_len),
                (&runtime_ptr, runtime_len),
            );
        } else {
            boundary::emit_scalar_export(
                emitter,
                target,
                export,
                (&invalid_ptr, invalid_len),
                (&allocation_ptr, allocation_len),
                (&runtime_ptr, runtime_len),
            );
        }
    }
    emit_lifecycle_exports(emitter, target, heap_debug);
}

/// Builds a deterministic local-label suffix from a public PHP export name.
fn label_suffix(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Pushes the cdylib exception handler record and snapshots it with `setjmp` on AArch64.
fn emit_boundary_push_aarch64(emitter: &mut Emitter, escaped: &str, handler_base: usize) {
    abi::emit_frame_slot_address(emitter, "x11", handler_base);
    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_handler_top", 0);
    emitter.instruction("str x9, [x11]");                                       // snapshot the previous native exception-handler record
    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_call_frame_top", 0);
    emitter.instruction("str x9, [x11, #8]");                                   // snapshot the current PHP cleanup-frame chain
    abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("str x9, [x11, #{TRY_HANDLER_DIAG_DEPTH_OFFSET}]")); // snapshot diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x11", "_exc_handler_top", 0);
    emitter.instruction(&format!("add x0, x11, #{TRY_HANDLER_JMP_BUF_OFFSET}")); // materialize the jump buffer embedded in the handler record
    emitter.bl_c("setjmp");
    emitter.instruction(&format!("cbnz x0, {escaped}"));                        // enter boundary recovery after longjmp returns nonzero
}

/// Restores the previous exception and diagnostic state on AArch64.
fn emit_boundary_pop_aarch64(emitter: &mut Emitter, handler_base: usize) {
    abi::emit_frame_slot_address(emitter, "x11", handler_base);
    emitter.instruction("ldr x10, [x11]");                                      // restore the previous native exception-handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("ldr x10, [x11, #{TRY_HANDLER_DIAG_DEPTH_OFFSET}]")); // restore diagnostic-suppression depth
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
}

/// Pushes the cdylib exception handler record and snapshots it with `setjmp` on x86_64.
fn emit_boundary_push_x86_64(emitter: &mut Emitter, escaped: &str, handler_base: usize) {
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {handler_base}], r10")); // snapshot the previous native exception-handler record
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", handler_base - 8)); // snapshot the current PHP cleanup-frame chain
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!(                                               // snapshot diagnostic suppression in the boundary record
        "mov QWORD PTR [rbp - {}], r10",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    emitter.instruction(&format!("lea r10, [rbp - {handler_base}]"));           // materialize the current exception-handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // pass the embedded jump buffer to setjmp
        "lea rdi, [rbp - {}]",
        handler_base - TRY_HANDLER_JMP_BUF_OFFSET
    ));
    emitter.bl_c("setjmp");
    emitter.instruction("test eax, eax");                                       // test whether setjmp returned through longjmp
    emitter.instruction(&format!("jne {escaped}"));                             // enter boundary recovery after longjmp returns nonzero
}

/// Restores the previous exception and diagnostic state on x86_64.
fn emit_boundary_pop_x86_64(emitter: &mut Emitter, handler_base: usize) {
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {handler_base}]")); // restore the previous native exception-handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!(                                               // restore diagnostic suppression from the boundary record
        "mov r10, QWORD PTR [rbp - {}]",
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET
    ));
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
}

/// Emits the bounded, allocation-free diagnostic copy helper for both targets.
fn emit_error_helpers(emitter: &mut Emitter, target: Target) {
    emitter.blank();
    emitter.comment("--- cdylib: copy borrowed diagnostic into stable storage ---");
    emitter.label_global("__rt_cdylib_set_error");
    match target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", LAST_ERROR_BUFFER);
            emitter.instruction(&format!("mov x10, #{}", LAST_ERROR_CAPACITY - 1)); // cap the copied diagnostic to stable buffer capacity
            emitter.instruction("cmp x1, x10");                                 // bound the stable diagnostic copy to its recorded length
            emitter.instruction("csel x1, x1, x10, ls");                        // move bounded diagnostic state between source and stable storage
            emitter.instruction("mov x11, #0");                                 // move bounded diagnostic state between source and stable storage
            emitter.label("L_cdylib_error_copy_aarch64");
            emitter.instruction("cmp x11, x1");                                 // bound the stable diagnostic copy to its recorded length
            emitter.instruction("b.hs L_cdylib_error_copied_aarch64");          // finish when the bounded diagnostic copy reaches its length
            emitter.instruction("ldrb w12, [x0, x11]");                         // load one byte from the borrowed diagnostic
            emitter.instruction("strb w12, [x9, x11]");                         // copy one byte into stable diagnostic storage
            emitter.instruction("add x11, x11, #1");                            // advance the diagnostic copy cursor
            emitter.instruction("b L_cdylib_error_copy_aarch64");               // continue the bounded diagnostic copy loop
            emitter.label("L_cdylib_error_copied_aarch64");
            emitter.instruction("strb wzr, [x9, x1]");                          // NUL-terminate the stable diagnostic storage
            abi::emit_store_reg_to_symbol(emitter, "x1", LAST_ERROR_LENGTH, 0);
            emit_store_immediate_to_symbol(emitter, LAST_ERROR_PRESENT, 1);
            emitter.instruction("ret");                                         // return to the current C-ABI caller
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r8", LAST_ERROR_BUFFER);
            emitter.instruction(&format!("mov r9, {}", LAST_ERROR_CAPACITY - 1)); // cap the copied diagnostic to stable buffer capacity
            emitter.instruction("cmp rsi, r9");                                 // bound the stable diagnostic copy to its recorded length
            emitter.instruction("cmova rsi, r9");                               // move bounded diagnostic state between source and stable storage
            emitter.instruction("xor r10d, r10d");                              // move bounded diagnostic state between source and stable storage
            emitter.label("L_cdylib_error_copy_x86_64");
            emitter.instruction("cmp r10, rsi");                                // bound the stable diagnostic copy to its recorded length
            emitter.instruction("jae L_cdylib_error_copied_x86_64");            // finish when the bounded diagnostic copy reaches its length
            emitter.instruction("movzx r11d, BYTE PTR [rdi + r10]");            // load one byte from the borrowed diagnostic
            emitter.instruction("mov BYTE PTR [r8 + r10], r11b");               // copy one byte into stable diagnostic storage
            emitter.instruction("add r10, 1");                                  // advance the diagnostic copy cursor
            emitter.instruction("jmp L_cdylib_error_copy_x86_64");              // continue the bounded diagnostic copy loop
            emitter.label("L_cdylib_error_copied_x86_64");
            emitter.instruction("mov BYTE PTR [r8 + rsi], 0");                  // NUL-terminate the stable diagnostic storage
            abi::emit_store_reg_to_symbol(emitter, "rsi", LAST_ERROR_LENGTH, 0);
            emit_store_immediate_to_symbol(emitter, LAST_ERROR_PRESENT, 1);
            emitter.instruction("ret");                                         // return to the current C-ABI caller
        }
    }
}

/// Clears the stable diagnostic state without touching any C argument registers.
fn emit_clear_error_inline(emitter: &mut Emitter) {
    emit_store_immediate_to_symbol(emitter, LAST_ERROR_PRESENT, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", LAST_ERROR_LENGTH);
            emitter.instruction("str xzr, [x9]");                               // clear the recorded diagnostic length or leading byte
            abi::emit_symbol_address(emitter, "x9", LAST_ERROR_BUFFER);
            emitter.instruction("strb wzr, [x9]");                              // clear the recorded diagnostic length or leading byte
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", LAST_ERROR_LENGTH);
            emitter.instruction("mov QWORD PTR [r10], 0");                      // clear the recorded diagnostic length or leading byte
            abi::emit_symbol_address(emitter, "r10", LAST_ERROR_BUFFER);
            emitter.instruction("mov BYTE PTR [r10], 0");                       // clear the recorded diagnostic length or leading byte
        }
    }
}

/// Restores the process-global concat scratch cursor between native host calls.
fn emit_reset_concat_inline(emitter: &mut Emitter) {
    emit_store_immediate_to_symbol(emitter, "_concat_off", 0);
}

/// Stores one small integer in a fixed cdylib state symbol.
fn emit_store_immediate_to_symbol(emitter: &mut Emitter, symbol: &str, value: i64) {
    abi::emit_store_imm_to_symbol(emitter, symbol, 0, value);
}

/// Copies one compiler-emitted diagnostic into stable storage on AArch64.
fn emit_set_static_error_aarch64(emitter: &mut Emitter, error: (&str, usize)) {
    abi::emit_symbol_address(emitter, "x0", error.0);
    emitter.instruction(&format!("mov x1, #{}", error.1));                      // pass the static diagnostic byte length to the copy helper
    emitter.instruction("bl __rt_cdylib_set_error");                            // copy the current diagnostic into stable boundary storage
}

/// Copies one compiler-emitted diagnostic into stable storage on x86_64.
fn emit_set_static_error_x86_64(emitter: &mut Emitter, error: (&str, usize)) {
    abi::emit_symbol_address(emitter, "rdi", error.0);
    emitter.instruction(&format!("mov rsi, {}", error.1));                      // pass the static diagnostic byte length to the copy helper
    emitter.instruction("call __rt_cdylib_set_error");                          // copy the current diagnostic into stable boundary storage
}

/// Emits ABI version, lifecycle, last-status, last-error, and owned-buffer release exports.
fn emit_lifecycle_exports(emitter: &mut Emitter, target: Target, heap_debug: bool) {
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
        if lifecycle == "elephc_init" && matches!(target.arch, Arch::AArch64) {
            abi::emit_frame_prologue(emitter, 16);
        }
        emit_clear_error_inline(emitter);
        emit_reset_concat_inline(emitter);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_ACTIVE, 0);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
        if lifecycle == "elephc_init" {
            crate::codegen::stack_guard::emit_stack_limit_init_call(emitter);
            abi::emit_call_label(emitter, "__rt_gc_request_start");
            if heap_debug {
                abi::emit_enable_heap_debug_flag(emitter);
            }
            match target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("mov w0, #{STATUS_OK}"));      // return successful runtime initialization
                }
                Arch::X86_64 => emitter.instruction(&format!("mov eax, {STATUS_OK}")), // return successful runtime initialization
            }
        }
        if lifecycle == "elephc_init" && matches!(target.arch, Arch::AArch64) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};
    use crate::span::Span;
    use crate::types::{FunctionSig, PhpType};

    /// Builds one single-string-input owned-result export fixture.
    fn string_export() -> ExportedFunction {
        ExportedFunction {
            name: "roundtrip".to_string(),
            c_name: "roundtrip".to_string(),
            sig: FunctionSig {
                params: vec![("input".to_string(), PhpType::Str)],
                param_type_exprs: vec![None],
                param_attributes: vec![Vec::new()],
                defaults: vec![None],
                return_type: PhpType::Str,
                declared_return: true,
                by_ref_return: false,
                ref_params: vec![false],
                declared_params: vec![true],
                variadic: None,
                deprecation: None,
            },
            span: Span::dummy(),
        }
    }

    /// Emits an AArch64 boundary with setjmp, owned copy, and lifecycle exports.
    #[test]
    fn emits_aarch64_owned_string_boundary() {
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let mut emitter = Emitter::new_cdylib(target);
        let mut data = DataSection::new();
        let export = string_export();
        emit_cdylib_exports(&mut emitter, &mut data, target, &[&export], false);
        let asm = emitter.output();
        assert!(asm.contains("_roundtrip:"));
        assert!(asm.contains("bl _setjmp"));
        assert!(asm.contains("bl __rt_heap_alloc"));
        assert!(asm.contains("add x9, x9, #1"));
        assert!(asm.contains("sub x9, x9, #1"));
        assert!(asm.contains("_elephc_abi_version:"));
        assert!(asm.contains("bl __rt_stack_limit_init"));
        let saved_host_args = asm.find("stur x3, [x29, #-32]").unwrap();
        let lazy_init = asm.find("bl __rt_stack_limit_init").unwrap();
        let boundary_push = asm.find("bl _setjmp").unwrap();
        assert!(
            saved_host_args < lazy_init && lazy_init < boundary_push,
            "lazy stack init must follow host-argument preservation and precede boundary entry:\n{asm}"
        );
        assert!(data.emit(target).contains(LAST_ERROR_BUFFER));
    }

    /// Pins the ABI-v3 Darwin boundary to real iOS device and Simulator targets,
    /// including the persisted cache key and Mach-O linker platform token.
    #[test]
    fn emits_ios_aarch64_v3_boundary_with_distinct_target_identity() {
        for (variant, cache_key, macho_platform) in [
            (AppleVariant::IOS, "ios-arm64", "ios"),
            (
                AppleVariant::IOSSimulator,
                "ios-sim-arm64",
                "ios-simulator",
            ),
        ] {
            let target = Target::new_apple(Arch::AArch64, variant);
            assert_eq!(target.as_str(), cache_key);
            assert_eq!(target.apple_platform_name(), macho_platform);

            let mut emitter = Emitter::new_cdylib(target);
            let mut data = DataSection::new();
            let export = string_export();
            emit_cdylib_exports(&mut emitter, &mut data, target, &[&export], false);
            let asm = emitter.output();

            assert!(
                asm.contains(".globl _roundtrip\n_roundtrip:"),
                "{cache_key}: missing public Darwin export"
            );
            assert!(
                asm.contains("stur x2, [x29, #-24]")
                    && asm.contains("stur x3, [x29, #-32]"),
                "{cache_key}: missing ABI-v3 output-address preservation:\n{asm}"
            );
            assert!(asm.contains("_elephc_abi_version:"));
            assert!(asm.contains(&format!("mov w0, #{ELEPHC_ABI_VERSION}")));
            assert!(data.emit(target).contains(LAST_ERROR_BUFFER));
        }
    }

    /// AArch64 lifecycle initialization preserves the host return address around
    /// the stack-limit helper call instead of returning to its own post-call instruction.
    #[test]
    fn aarch64_cdylib_init_preserves_the_host_return_address() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
        ] {
            let mut emitter = Emitter::new_cdylib(target);
            let mut data = DataSection::new();
            let export = string_export();
            emit_cdylib_exports(&mut emitter, &mut data, target, &[&export], false);
            let asm = emitter.output();
            let init_label = format!("{}:", target.extern_symbol("elephc_init"));
            let shutdown_label = format!("{}:", target.extern_symbol("elephc_shutdown"));
            let init_start = asm.find(&init_label).unwrap();
            let init_end = asm[init_start..].find(&shutdown_label).unwrap() + init_start;
            let init = &asm[init_start..init_end];
            let save = init.find("stp x29, x30, [sp, #0]").unwrap();
            let call = init.find("bl __rt_stack_limit_init").unwrap();
            let restore = init.find("ldp x29, x30, [x9]").unwrap();
            assert!(
                save < call && call < restore,
                "{target:?} did not preserve x30 around cdylib initialization:\n{init}"
            );
        }
    }

    /// Emits an x86_64 boundary with the System V status/out-parameter shape.
    #[test]
    fn emits_x86_64_owned_string_boundary() {
        let target = Target::new(Platform::Linux, Arch::X86_64);
        let mut emitter = Emitter::new_cdylib(target);
        let mut data = DataSection::new();
        let export = string_export();
        emit_cdylib_exports(&mut emitter, &mut data, target, &[&export], false);
        let asm = emitter.output();
        assert!(asm.contains("roundtrip:"));
        assert!(asm.contains("call setjmp"));
        assert!(asm.contains("call __rt_heap_alloc"));
        assert!(asm.contains("add r10, 1"));
        assert!(asm.contains("sub r10, 1"));
        assert!(asm.contains("elephc_free:"));
        assert!(asm.contains("call __rt_stack_limit_init"));
        let saved_host_args = asm.find("mov QWORD PTR [rbp - 32], rcx").unwrap();
        let lazy_init = asm.find("call __rt_stack_limit_init").unwrap();
        let boundary_push = asm.find("call setjmp").unwrap();
        assert!(
            saved_host_args < lazy_init && lazy_init < boundary_push,
            "lazy stack init must follow host-argument preservation and precede boundary entry:\n{asm}"
        );
    }

    /// Every supported target saves all four owned-string host arguments before the
    /// lazy initializer and reloads output addresses from stable frame slots afterward.
    #[test]
    fn preserves_owned_string_host_arguments_across_lazy_stack_init_for_all_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new_cdylib(target);
            let mut data = DataSection::new();
            let export = string_export();
            emit_cdylib_exports(&mut emitter, &mut data, target, &[&export], false);
            let asm = emitter.output();
            let lazy_init = asm.find("__rt_stack_limit_init").unwrap();
            let first_output_load = match target.arch {
                Arch::AArch64 => asm[lazy_init..].find("ldur x9, [x29, #-24]").unwrap() + lazy_init,
                Arch::X86_64 => {
                    asm[lazy_init..]
                        .find("mov r10, QWORD PTR [rbp - 24]")
                        .unwrap()
                        + lazy_init
                }
            };
            let saved_arguments = match target.arch {
                Arch::AArch64 => [
                    "stur x0, [x29, #-8]",
                    "stur x1, [x29, #-16]",
                    "stur x2, [x29, #-24]",
                    "stur x3, [x29, #-32]",
                ],
                Arch::X86_64 => [
                    "mov QWORD PTR [rbp - 8], rdi",
                    "mov QWORD PTR [rbp - 16], rsi",
                    "mov QWORD PTR [rbp - 24], rdx",
                    "mov QWORD PTR [rbp - 32], rcx",
                ],
            };
            for saved_argument in saved_arguments {
                let saved = asm.find(saved_argument).unwrap();
                assert!(
                    saved < lazy_init,
                    "{target:?} did not preserve `{saved_argument}` before lazy initialization:\n{asm}"
                );
            }
            assert!(
                lazy_init < first_output_load,
                "{target:?} loaded an output address before lazy initialization completed:\n{asm}"
            );
        }
    }
}
