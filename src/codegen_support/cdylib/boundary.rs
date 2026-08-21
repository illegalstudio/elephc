//! Purpose:
//! Emits recoverable scalar cdylib wrappers while preserving their public C signatures.
//!
//! Called from:
//! - `super::emit_cdylib_exports()` for every non-string-return export.
//!
//! Key details:
//! - Public C arguments are saved before setjmp and rematerialized through Elephc's ABI.
//! - Failures return a deterministic zero while status and diagnostics remain queryable.
//! - Boundary depth and concat scratch state are restored exactly across nested entries.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Target};
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;
use crate::exports::ExportedFunction;
use crate::names::function_symbol;
use crate::types::PhpType;

use super::{
    emit_boundary_pop_aarch64, emit_boundary_pop_x86_64, emit_boundary_push_aarch64,
    emit_boundary_push_x86_64, emit_clear_error_inline, emit_set_static_error_aarch64,
    emit_set_static_error_x86_64, emit_store_immediate_to_symbol, label_suffix, BOUNDARY_ACTIVE,
    BOUNDARY_STATUS, CONCAT_SCRATCH_CAPACITY, STATUS_ALLOCATION_FAILURE,
    STATUS_INVALID_ARGUMENT, STATUS_OK, STATUS_PHP_EXCEPTION, STATUS_RUNTIME_FAILURE,
};

/// Fixed frame metadata for one recoverable scalar export wrapper.
struct ScalarBoundaryLayout {
    param_offsets: Vec<Vec<usize>>,
    result_offset: Option<usize>,
    concat_offset: usize,
    handler_base: usize,
    frame_size: usize,
}
/// Emits one scalar trampoline with the same C signature and a recoverable boundary.
pub(super) fn emit_scalar_export(
    emitter: &mut Emitter,
    target: Target,
    export: &ExportedFunction,
    invalid_error: (&str, usize),
    allocation_error: (&str, usize),
    runtime_error: (&str, usize),
) {
    let internal = function_symbol(&export.name);
    let exported = target.extern_symbol(&export.c_name);
    let suffix = label_suffix(&export.c_name);
    let layout = scalar_boundary_layout(export);
    emitter.blank();
    emitter.comment(&format!(
        "#[Export] recoverable scalar boundary for PHP function {}",
        export.name
    ));
    emitter.label_global(&exported);
    let invalid = emitter_data_label(&suffix, "scalar_invalid");
    let escaped = emitter_data_label(&suffix, "scalar_escaped");
    let allocation = emitter_data_label(&suffix, "scalar_allocation");
    let runtime = emitter_data_label(&suffix, "scalar_runtime");
    let exception = emitter_data_label(&suffix, "scalar_exception");
    match target.arch {
        Arch::AArch64 => emit_scalar_export_aarch64(
            emitter,
            export,
            &layout,
            &suffix,
            &internal,
            &invalid,
            &escaped,
            &allocation,
            &runtime,
            &exception,
            invalid_error,
            allocation_error,
            runtime_error,
        ),
        Arch::X86_64 => emit_scalar_export_x86_64(
            emitter,
            export,
            &layout,
            &suffix,
            &internal,
            &invalid,
            &escaped,
            &allocation,
            &runtime,
            &exception,
            invalid_error,
            allocation_error,
            runtime_error,
        ),
    }
}

/// Builds one local label name within a public export wrapper.
fn emitter_data_label(suffix: &str, purpose: &str) -> String {
    format!("L_cdylib_{suffix}_{purpose}")
}

/// Computes saved-input, result, concat, handler, and footer slots for a scalar wrapper.
fn scalar_boundary_layout(export: &ExportedFunction) -> ScalarBoundaryLayout {
    let mut offset = 0usize;
    let mut param_offsets = Vec::with_capacity(export.sig.params.len());
    for (_, ty) in &export.sig.params {
        let words = if *ty == PhpType::Str { 2 } else { 1 };
        let mut offsets = Vec::with_capacity(words);
        for _ in 0..words {
            offset += 8;
            offsets.push(offset);
        }
        param_offsets.push(offsets);
    }
    let result_offset = if export.sig.return_type == PhpType::Void {
        None
    } else {
        offset += 8;
        Some(offset)
    };
    offset += 8;
    let concat_offset = offset;
    let handler_base = align_16(offset + TRY_HANDLER_SLOT_SIZE);
    let frame_size = align_16(handler_base + 16);
    ScalarBoundaryLayout {
        param_offsets,
        result_offset,
        concat_offset,
        handler_base,
        frame_size,
    }
}

/// Rounds a frame byte count up to the native 16-byte stack alignment.
fn align_16(value: usize) -> usize {
    (value + 15) & !15
}

/// Flattens PHP parameters into the independent scalar words used by the public C ABI.
fn scalar_c_input_types(export: &ExportedFunction) -> Vec<PhpType> {
    export
        .sig
        .params
        .iter()
        .flat_map(|(_, ty)| match ty {
            PhpType::Str => vec![PhpType::Int, PhpType::Int],
            other => vec![other.clone()],
        })
        .collect()
}

/// Saves every public C input register or overflow word into stable wrapper-frame slots.
fn emit_save_scalar_c_inputs(
    emitter: &mut Emitter,
    export: &ExportedFunction,
    layout: &ScalarBoundaryLayout,
) {
    let flattened_types = scalar_c_input_types(export);
    let assignments = abi::build_outgoing_arg_assignments_for_target(
        emitter.target,
        &flattened_types,
        0,
    );
    let offsets = layout.param_offsets.iter().flatten().copied().collect::<Vec<_>>();
    let mut caller_stack_offset = 16usize;
    for ((ty, assignment), offset) in flattened_types.iter().zip(assignments).zip(offsets) {
        let reg = if assignment.in_register() {
            if assignment.is_float {
                abi::float_arg_reg_name(emitter.target, assignment.start_reg)
            } else {
                abi::int_arg_reg_name(emitter.target, assignment.start_reg)
            }
        } else if ty.is_float_reg() {
            abi::float_spill_scratch_reg(emitter.target)
        } else {
            abi::secondary_scratch_reg(emitter)
        };
        if !assignment.in_register() {
            abi::load_from_caller_stack(emitter, reg, caller_stack_offset);
            caller_stack_offset += 8;
        }
        abi::store_at_offset(emitter, reg, offset);
    }
}

/// Re-materializes saved public inputs through Elephc's internal function ABI.
fn emit_call_scalar_body(
    emitter: &mut Emitter,
    export: &ExportedFunction,
    layout: &ScalarBoundaryLayout,
    internal: &str,
) {
    for ((_, ty), offsets) in export.sig.params.iter().zip(&layout.param_offsets) {
        match ty {
            PhpType::Float => {
                abi::load_at_offset(emitter, abi::float_result_reg(emitter), offsets[0]);
            }
            PhpType::Str => {
                let (ptr, len) = abi::string_result_regs(emitter);
                abi::load_at_offset(emitter, ptr, offsets[0]);
                abi::load_at_offset(emitter, len, offsets[1]);
            }
            _ => {
                abi::load_at_offset(emitter, abi::int_result_reg(emitter), offsets[0]);
            }
        }
        abi::emit_push_result_value(emitter, ty);
    }
    let param_types = export
        .sig
        .params
        .iter()
        .map(|(_, ty)| ty.clone())
        .collect::<Vec<_>>();
    let assignments = abi::build_outgoing_arg_assignments_for_target(
        emitter.target,
        &param_types,
        0,
    );
    let overflow = abi::materialize_outgoing_args(emitter, &assignments);
    let pad = abi::outgoing_call_stack_pad_bytes(emitter.target, overflow);
    abi::emit_reserve_temporary_stack(emitter, pad);
    abi::emit_call_label(emitter, internal);
    abi::emit_release_temporary_stack(emitter, pad);
    abi::emit_release_temporary_stack(emitter, overflow);
}

/// Validates every public string pair without clobbering the saved inputs.
fn emit_validate_scalar_string_inputs(
    emitter: &mut Emitter,
    export: &ExportedFunction,
    layout: &ScalarBoundaryLayout,
    invalid: &str,
    suffix: &str,
) {
    for (index, ((_, ty), offsets)) in export
        .sig
        .params
        .iter()
        .zip(&layout.param_offsets)
        .enumerate()
    {
        if *ty != PhpType::Str {
            continue;
        }
        let valid = format!("L_cdylib_{suffix}_scalar_string_{index}_valid");
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offsets[1]);
                emitter.instruction(&format!("cbz x9, {valid}"));               // accept NULL storage only for an empty input string
                abi::load_at_offset(emitter, "x9", offsets[0]);
                emitter.instruction(&format!("cbz x9, {invalid}"));             // reject a non-empty string with NULL storage
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r10", offsets[1]);
                emitter.instruction("test r10, r10");                           // distinguish empty strings from non-empty inputs
                emitter.instruction(&format!("jz {valid}"));                    // accept NULL storage only for an empty input string
                abi::load_at_offset(emitter, "r10", offsets[0]);
                emitter.instruction("test r10, r10");                           // validate the non-empty input storage pointer
                emitter.instruction(&format!("jz {invalid}"));                  // reject a non-empty string with NULL storage
            }
        }
        emitter.label(&valid);
    }
}

/// Saves concat state, isolates nested scratch use, and increments boundary depth.
pub(super) fn emit_enter_boundary(emitter: &mut Emitter, concat_offset: usize, suffix: &str) {
    let nested = format!("L_cdylib_{suffix}_boundary_nested");
    let configured = format!("L_cdylib_{suffix}_boundary_concat_configured");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_ACTIVE, 0);
            emitter.instruction(&format!("cbnz x9, {nested}"));                 // isolate concat scratch only for a nested host entry
            emitter.instruction("mov x10, #0");                                 // outer entries restore an empty concat arena on return
            abi::store_at_offset(emitter, "x10", concat_offset);
            emit_store_immediate_to_symbol(emitter, "_concat_off", 0);
            emitter.instruction(&format!("b {configured}"));                    // join nested and outer concat setup
            emitter.label(&nested);
            abi::emit_load_symbol_to_reg(emitter, "x10", "_concat_off", 0);
            abi::store_at_offset(emitter, "x10", concat_offset);
            emit_store_immediate_to_symbol(
                emitter,
                "_concat_off",
                CONCAT_SCRATCH_CAPACITY,
            );
            emitter.label(&configured);
            abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_ACTIVE, 0);
            emitter.instruction("add x9, x9, #1");                              // increment the re-entrant boundary depth
            abi::emit_store_reg_to_symbol(emitter, "x9", BOUNDARY_ACTIVE, 0);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_ACTIVE, 0);
            emitter.instruction("test r10, r10");                               // distinguish an outer entry from a re-entrant call
            emitter.instruction(&format!("jnz {nested}"));                      // isolate concat scratch only for a nested host entry
            emitter.instruction("xor r11d, r11d");                              // outer entries restore an empty concat arena on return
            abi::store_at_offset(emitter, "r11", concat_offset);
            emit_store_immediate_to_symbol(emitter, "_concat_off", 0);
            emitter.instruction(&format!("jmp {configured}"));                  // join nested and outer concat setup
            emitter.label(&nested);
            abi::emit_load_symbol_to_reg(emitter, "r11", "_concat_off", 0);
            abi::store_at_offset(emitter, "r11", concat_offset);
            emit_store_immediate_to_symbol(
                emitter,
                "_concat_off",
                CONCAT_SCRATCH_CAPACITY,
            );
            emitter.label(&configured);
            abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_ACTIVE, 0);
            emitter.instruction("add r10, 1");                                  // increment the re-entrant boundary depth
            abi::emit_store_reg_to_symbol(emitter, "r10", BOUNDARY_ACTIVE, 0);
        }
    }
}

/// Decrements boundary depth and restores the caller's concat scratch cursor.
pub(super) fn emit_leave_boundary(emitter: &mut Emitter, concat_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_ACTIVE, 0);
            emitter.instruction("sub x9, x9, #1");                              // leave exactly one nested host boundary
            abi::emit_store_reg_to_symbol(emitter, "x9", BOUNDARY_ACTIVE, 0);
            abi::load_at_offset(emitter, "x10", concat_offset);
            abi::emit_store_reg_to_symbol(emitter, "x10", "_concat_off", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_ACTIVE, 0);
            emitter.instruction("sub r10, 1");                                  // leave exactly one nested host boundary
            abi::emit_store_reg_to_symbol(emitter, "r10", BOUNDARY_ACTIVE, 0);
            abi::load_at_offset(emitter, "r11", concat_offset);
            abi::emit_store_reg_to_symbol(emitter, "r11", "_concat_off", 0);
        }
    }
}

/// Saves the internal scalar result before boundary teardown clobbers scratch registers.
fn emit_save_scalar_result(
    emitter: &mut Emitter,
    return_type: &PhpType,
    result_offset: Option<usize>,
) {
    let Some(offset) = result_offset else {
        return;
    };
    let reg = if *return_type == PhpType::Float {
        abi::float_result_reg(emitter)
    } else {
        abi::int_result_reg(emitter)
    };
    abi::store_at_offset(emitter, reg, offset);
}

/// Reloads a successful scalar result into the public C ABI return register.
fn emit_load_scalar_result(
    emitter: &mut Emitter,
    return_type: &PhpType,
    result_offset: Option<usize>,
) {
    let Some(offset) = result_offset else {
        return;
    };
    let reg = if *return_type == PhpType::Float {
        abi::float_result_reg(emitter)
    } else {
        abi::int_result_reg(emitter)
    };
    abi::load_at_offset(emitter, reg, offset);
}

/// Materializes a deterministic zero result after a recovered scalar failure.
fn emit_zero_scalar_result(emitter: &mut Emitter, return_type: &PhpType) {
    match (emitter.target.arch, return_type) {
        (_, PhpType::Void) => {}
        (Arch::AArch64, PhpType::Float) => {
            emitter.instruction("fmov d0, xzr");                                // return 0.0 after a recovered scalar failure
        }
        (Arch::AArch64, _) => {
            emitter.instruction("mov x0, #0");                                  // return zero after a recovered scalar failure
        }
        (Arch::X86_64, PhpType::Float) => {
            emitter.instruction("pxor xmm0, xmm0");                             // return 0.0 after a recovered scalar failure
        }
        (Arch::X86_64, _) => {
            emitter.instruction("xor eax, eax");                                // return zero after a recovered scalar failure
        }
    }
}

/// Restores the native wrapper frame and returns to the host.
fn emit_scalar_native_return(emitter: &mut Emitter, frame_size: usize) {
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits the AArch64 recoverable wrapper for one scalar-returning export.
#[allow(clippy::too_many_arguments)]
fn emit_scalar_export_aarch64(
    emitter: &mut Emitter,
    export: &ExportedFunction,
    layout: &ScalarBoundaryLayout,
    suffix: &str,
    internal: &str,
    invalid: &str,
    escaped: &str,
    allocation: &str,
    runtime: &str,
    exception: &str,
    invalid_error: (&str, usize),
    allocation_error: (&str, usize),
    runtime_error: (&str, usize),
) {
    abi::emit_frame_prologue(emitter, layout.frame_size);
    emit_save_scalar_c_inputs(emitter, export, layout);
    emit_clear_error_inline(emitter);
    emit_validate_scalar_string_inputs(emitter, export, layout, invalid, suffix);
    emit_enter_boundary(emitter, layout.concat_offset, suffix);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
    emit_boundary_push_aarch64(emitter, escaped, layout.handler_base);
    emit_call_scalar_body(emitter, export, layout, internal);
    emit_save_scalar_result(emitter, &export.sig.return_type, layout.result_offset);
    emit_boundary_pop_aarch64(emitter, layout.handler_base);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_load_scalar_result(emitter, &export.sig.return_type, layout.result_offset);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(escaped);
    emit_boundary_pop_aarch64(emitter, layout.handler_base);
    abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_STATUS, 0);
    emitter.instruction(&format!("cmp x9, #{STATUS_ALLOCATION_FAILURE}"));      // distinguish allocation failure from other boundary escapes
    emitter.instruction(&format!("b.eq {allocation}"));                         // report a recoverable allocation failure
    emitter.instruction(&format!("cbnz x9, {runtime}"));                        // report a non-Throwable runtime escape
    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
    emitter.instruction(&format!("cbnz x9, {exception}"));                      // capture an escaping Throwable diagnostic
    emitter.instruction(&format!("b {runtime}"));                               // report a generic runtime escape

    emitter.label(exception);
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the escaping Throwable message pointer
    emitter.instruction("ldr x1, [x9, #16]");                                   // load the escaping Throwable message length
    emitter.instruction("bl __rt_cdylib_set_error");                            // copy the Throwable diagnostic into stable boundary storage
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    emitter.instruction("bl __rt_decref_any");                                  // release the consumed escaping Throwable object
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_PHP_EXCEPTION as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(allocation);
    emit_set_static_error_aarch64(emitter, allocation_error);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_ALLOCATION_FAILURE as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(runtime);
    emit_set_static_error_aarch64(emitter, runtime_error);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_RUNTIME_FAILURE as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(invalid);
    emit_set_static_error_aarch64(emitter, invalid_error);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_INVALID_ARGUMENT as i64);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);
}

/// Emits the x86_64 System V recoverable wrapper for one scalar export.
#[allow(clippy::too_many_arguments)]
fn emit_scalar_export_x86_64(
    emitter: &mut Emitter,
    export: &ExportedFunction,
    layout: &ScalarBoundaryLayout,
    suffix: &str,
    internal: &str,
    invalid: &str,
    escaped: &str,
    allocation: &str,
    runtime: &str,
    exception: &str,
    invalid_error: (&str, usize),
    allocation_error: (&str, usize),
    runtime_error: (&str, usize),
) {
    abi::emit_frame_prologue(emitter, layout.frame_size);
    emit_save_scalar_c_inputs(emitter, export, layout);
    emit_clear_error_inline(emitter);
    emit_validate_scalar_string_inputs(emitter, export, layout, invalid, suffix);
    emit_enter_boundary(emitter, layout.concat_offset, suffix);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
    emit_boundary_push_x86_64(emitter, escaped, layout.handler_base);
    emit_call_scalar_body(emitter, export, layout, internal);
    emit_save_scalar_result(emitter, &export.sig.return_type, layout.result_offset);
    emit_boundary_pop_x86_64(emitter, layout.handler_base);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_load_scalar_result(emitter, &export.sig.return_type, layout.result_offset);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(escaped);
    emit_boundary_pop_x86_64(emitter, layout.handler_base);
    abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_STATUS, 0);
    emitter.instruction(&format!("cmp r10, {STATUS_ALLOCATION_FAILURE}"));      // distinguish allocation failure from other boundary escapes
    emitter.instruction(&format!("je {allocation}"));                           // report a recoverable allocation failure
    emitter.instruction("test r10, r10");                                       // distinguish Throwable propagation from runtime status escapes
    emitter.instruction(&format!("jne {runtime}"));                             // report a non-Throwable runtime escape
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
    emitter.instruction("test r10, r10");                                       // check whether a Throwable escaped the PHP body
    emitter.instruction(&format!("jne {exception}"));                           // capture an escaping Throwable diagnostic
    emitter.instruction(&format!("jmp {runtime}"));                             // report a generic runtime escape

    emitter.label(exception);
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // load the escaping Throwable message pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // load the escaping Throwable message length
    emitter.instruction("call __rt_cdylib_set_error");                          // copy the Throwable diagnostic into stable boundary storage
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    emitter.instruction("call __rt_decref_any");                                // release the consumed escaping Throwable object
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_PHP_EXCEPTION as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(allocation);
    emit_set_static_error_x86_64(emitter, allocation_error);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_ALLOCATION_FAILURE as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(runtime);
    emit_set_static_error_x86_64(emitter, runtime_error);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_RUNTIME_FAILURE as i64);
    emit_leave_boundary(emitter, layout.concat_offset);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);

    emitter.label(invalid);
    emit_set_static_error_x86_64(emitter, invalid_error);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_INVALID_ARGUMENT as i64);
    emit_zero_scalar_result(emitter, &export.sig.return_type);
    emit_scalar_native_return(emitter, layout.frame_size);
}
