//! Purpose:
//! Emits recoverable library wrappers for every fixed-input export returning a PHP string.
//! Flattens scalar/string C inputs and appends caller-owned output pointer/length parameters.
//!
//! Called from:
//! - `super::emit_cdylib_exports()` for exports whose declared return type is `string`.
//!
//! Key details:
//! - Public inputs are saved before helper calls and rematerialized through Elephc's ABI.
//! - Returned bytes are copied into caller-owned storage and remain binary-safe.
//! - Results aliasing any borrowed host string input are never released as runtime storage.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Target};
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;
use crate::exports::ExportedFunction;
use crate::names::function_symbol;
use crate::types::PhpType;

use super::{
    boundary, emit_boundary_pop_aarch64, emit_boundary_pop_x86_64,
    emit_boundary_push_aarch64, emit_boundary_push_x86_64, emit_clear_error_inline,
    emit_set_static_error_aarch64, emit_set_static_error_x86_64,
    emit_store_immediate_to_symbol, label_suffix, BOUNDARY_STATUS, STATUS_ALLOCATION_FAILURE,
    STATUS_INVALID_ARGUMENT, STATUS_OK, STATUS_PHP_EXCEPTION, STATUS_RUNTIME_FAILURE,
};

/// Frame slots required by one generic caller-owned string result wrapper.
struct OwnedStringBoundaryLayout {
    param_offsets: Vec<Vec<usize>>,
    output_ptr_offset: usize,
    output_len_offset: usize,
    result_ptr_offset: usize,
    result_len_offset: usize,
    owned_ptr_offset: usize,
    concat_offset: usize,
    /// Slot holding the HOST's callee-saved ctx register across the call; the
    /// publish below overwrites it and every return path must put it back.
    saved_ctx_offset: usize,
    handler_base: usize,
    frame_size: usize,
}

/// Local labels used by all success and recovery paths for one wrapper.
struct OwnedStringLabels {
    invalid: String,
    escaped: String,
    allocation: String,
    allocation_active: String,
    runtime: String,
    exception: String,
    copy: String,
    copied: String,
    success_release_done: String,
    allocation_release_done: String,
}

/// Emits one generic status/out-parameter C ABI wrapper for a string-return export.
pub(super) fn emit_owned_string_export(
    emitter: &mut Emitter,
    target: Target,
    export: &ExportedFunction,
    invalid_error: (&str, usize),
    allocation_error: (&str, usize),
    runtime_error: (&str, usize),
) {
    let suffix = label_suffix(&export.c_name);
    let internal = function_symbol(&export.name);
    let exported = target.extern_symbol(&export.c_name);
    let layout = owned_string_boundary_layout(export);
    let labels = owned_string_labels(&suffix);

    emitter.blank();
    emitter.comment(&format!(
        "#[Export] owned string boundary for PHP function {}",
        export.name
    ));
    emitter.label_global(&exported);
    abi::emit_frame_prologue(emitter, layout.frame_size);
    // Foreign-entry publish (spike review, B2): re-establish the per-context
    // state pointer before compiled PHP code runs; the host's callee-saved ctx
    // register is foreign. Publish-only: never reset allocator state here.
    crate::codegen_support::runtime::ctx::emit_ctx_save_foreign(
        emitter,
        layout.saved_ctx_offset,
    );
    crate::codegen_support::runtime::ctx::emit_ctx_publish(emitter);
    emit_save_public_arguments(emitter, export, &layout);
    crate::codegen::stack_guard::emit_lazy_stack_limit_init(
        emitter,
        &format!("L_cdylib_{suffix}_stack_limit_ready"),
    );
    abi::emit_store_zero_to_local_slot(emitter, layout.result_ptr_offset);
    abi::emit_store_zero_to_local_slot(emitter, layout.owned_ptr_offset);
    emit_clear_error_inline(emitter);
    emit_clear_and_validate_outputs(emitter, &layout, &labels.invalid, &suffix);
    boundary::emit_validate_string_inputs(
        emitter,
        export,
        &layout.param_offsets,
        &labels.invalid,
        &suffix,
    );
    boundary::emit_enter_boundary(emitter, layout.concat_offset, &suffix);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);

    emit_boundary_push(emitter, &labels.escaped, layout.handler_base);
    boundary::emit_call_body(emitter, export, &layout.param_offsets, &internal);
    emit_save_result(emitter, &layout);
    emit_branch_on_allocation_sentinel(emitter, &layout, &labels.allocation_active);
    emit_copy_owned_result(emitter, &layout, &labels);
    emit_release_result_unless_borrowed(
        emitter,
        export,
        &layout,
        &labels.success_release_done,
    );
    emit_boundary_pop(emitter, layout.handler_base);
    emit_publish_outputs(emitter, &layout);
    emit_entered_return(emitter, &layout, STATUS_OK);

    emitter.label(&labels.allocation_active);
    emit_boundary_pop(emitter, layout.handler_base);
    emit_branch(emitter, &labels.allocation);

    emitter.label(&labels.escaped);
    emit_boundary_pop(emitter, layout.handler_base);
    emit_classify_escape(emitter, &labels);

    emitter.label(&labels.exception);
    emit_capture_exception(emitter);
    emit_entered_return(emitter, &layout, STATUS_PHP_EXCEPTION);

    emitter.label(&labels.allocation);
    emit_release_result_unless_borrowed(
        emitter,
        export,
        &layout,
        &labels.allocation_release_done,
    );
    emit_set_static_error(emitter, allocation_error);
    emit_entered_return(emitter, &layout, STATUS_ALLOCATION_FAILURE);

    emitter.label(&labels.runtime);
    emit_set_static_error(emitter, runtime_error);
    emit_entered_return(emitter, &layout, STATUS_RUNTIME_FAILURE);

    emitter.label(&labels.invalid);
    emit_set_static_error(emitter, invalid_error);
    emit_unentered_return(emitter, &layout, STATUS_INVALID_ARGUMENT);
}

/// Computes stable frame slots for all flattened inputs, outputs, result state, and recovery data.
fn owned_string_boundary_layout(export: &ExportedFunction) -> OwnedStringBoundaryLayout {
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
    offset += 8;
    let output_ptr_offset = offset;
    offset += 8;
    let output_len_offset = offset;
    offset += 8;
    let result_ptr_offset = offset;
    offset += 8;
    let result_len_offset = offset;
    offset += 8;
    let owned_ptr_offset = offset;
    offset += 8;
    let concat_offset = offset;
    offset += 8;
    let saved_ctx_offset = offset;
    let handler_base = boundary::align_16(offset + TRY_HANDLER_SLOT_SIZE);
    let frame_size = boundary::align_16(handler_base + 16);
    OwnedStringBoundaryLayout {
        param_offsets,
        output_ptr_offset,
        output_len_offset,
        result_ptr_offset,
        result_len_offset,
        owned_ptr_offset,
        concat_offset,
        saved_ctx_offset,
        handler_base,
        frame_size,
    }
}

/// Builds deterministic labels for one public export wrapper.
fn owned_string_labels(suffix: &str) -> OwnedStringLabels {
    let label = |purpose: &str| format!("L_cdylib_{suffix}_{purpose}");
    OwnedStringLabels {
        invalid: label("string_invalid"),
        escaped: label("string_escaped"),
        allocation: label("string_allocation"),
        allocation_active: label("string_allocation_active"),
        runtime: label("string_runtime"),
        exception: label("string_exception"),
        copy: label("string_copy"),
        copied: label("string_copied"),
        success_release_done: label("string_success_release_done"),
        allocation_release_done: label("string_allocation_release_done"),
    }
}

/// Saves flattened PHP inputs followed by the two public output addresses.
fn emit_save_public_arguments(
    emitter: &mut Emitter,
    export: &ExportedFunction,
    layout: &OwnedStringBoundaryLayout,
) {
    let mut types = boundary::flattened_c_param_types(export);
    types.extend([PhpType::Int, PhpType::Int]);
    let mut offsets = layout
        .param_offsets
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    offsets.extend([layout.output_ptr_offset, layout.output_len_offset]);
    boundary::emit_save_c_words(emitter, &types, &offsets);
}

/// Clears supplied outputs and rejects either missing output address.
fn emit_clear_and_validate_outputs(
    emitter: &mut Emitter,
    layout: &OwnedStringBoundaryLayout,
    invalid: &str,
    suffix: &str,
) {
    let ptr_ready = format!("L_cdylib_{suffix}_string_output_ptr_ready");
    let len_ready = format!("L_cdylib_{suffix}_string_output_len_ready");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", layout.output_ptr_offset);
            emitter.instruction(&format!("cbz x9, {ptr_ready}"));               // clear the output pointer only when its address is present
            emitter.instruction("str xzr, [x9]");                               // clear caller-owned output storage before validation
            emitter.label(&ptr_ready);
            abi::load_at_offset(emitter, "x9", layout.output_len_offset);
            emitter.instruction(&format!("cbz x9, {len_ready}"));               // clear the output length only when its address is present
            emitter.instruction("str xzr, [x9]");                               // clear the authoritative output length before validation
            emitter.label(&len_ready);
            abi::load_at_offset(emitter, "x9", layout.output_ptr_offset);
            emitter.instruction(&format!("cbz x9, {invalid}"));                 // reject a missing required output-pointer address
            abi::load_at_offset(emitter, "x9", layout.output_len_offset);
            emitter.instruction(&format!("cbz x9, {invalid}"));                 // reject a missing required output-length address
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r10", layout.output_ptr_offset);
            emitter.instruction("test r10, r10");                               // check whether the output-pointer address is present
            emitter.instruction(&format!("jz {ptr_ready}"));                    // skip clearing a missing address until validation
            emitter.instruction("mov QWORD PTR [r10], 0");                      // clear caller-owned output storage before validation
            emitter.label(&ptr_ready);
            abi::load_at_offset(emitter, "r10", layout.output_len_offset);
            emitter.instruction("test r10, r10");                               // check whether the output-length address is present
            emitter.instruction(&format!("jz {len_ready}"));                    // skip clearing a missing address until validation
            emitter.instruction("mov QWORD PTR [r10], 0");                      // clear the authoritative output length before validation
            emitter.label(&len_ready);
            abi::load_at_offset(emitter, "r10", layout.output_ptr_offset);
            emitter.instruction("test r10, r10");                               // validate the required output-pointer address
            emitter.instruction(&format!("jz {invalid}"));                      // reject a missing required output-pointer address
            abi::load_at_offset(emitter, "r10", layout.output_len_offset);
            emitter.instruction("test r10, r10");                               // validate the required output-length address
            emitter.instruction(&format!("jz {invalid}"));                      // reject a missing required output-length address
        }
    }
}

/// Installs the target-specific setjmp recovery record.
fn emit_boundary_push(emitter: &mut Emitter, escaped: &str, handler_base: usize) {
    match emitter.target.arch {
        Arch::AArch64 => emit_boundary_push_aarch64(emitter, escaped, handler_base),
        Arch::X86_64 => emit_boundary_push_x86_64(emitter, escaped, handler_base),
    }
}

/// Removes the target-specific setjmp recovery record.
fn emit_boundary_pop(emitter: &mut Emitter, handler_base: usize) {
    match emitter.target.arch {
        Arch::AArch64 => emit_boundary_pop_aarch64(emitter, handler_base),
        Arch::X86_64 => emit_boundary_pop_x86_64(emitter, handler_base),
    }
}

/// Saves the borrowed internal string result before any helper call clobbers it.
fn emit_save_result(emitter: &mut Emitter, layout: &OwnedStringBoundaryLayout) {
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::store_at_offset(emitter, ptr, layout.result_ptr_offset);
    abi::store_at_offset(emitter, len, layout.result_len_offset);
}

/// Branches when the internal string length carries the allocation-failure sentinel.
fn emit_branch_on_allocation_sentinel(
    emitter: &mut Emitter,
    layout: &OwnedStringBoundaryLayout,
    allocation: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", layout.result_len_offset);
            emitter.instruction("cmn x9, #1");                                  // test the allocation-failure string-length sentinel
            emitter.instruction(&format!("b.eq {allocation}"));                 // recover a sentinel while the boundary is installed
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r10", layout.result_len_offset);
            emitter.instruction("cmp r10, -1");                                 // test the allocation-failure string-length sentinel
            emitter.instruction(&format!("je {allocation}"));                   // recover a sentinel while the boundary is installed
        }
    }
}

/// Allocates caller-owned storage and copies the binary PHP result plus a convenience NUL.
fn emit_copy_owned_result(
    emitter: &mut Emitter,
    layout: &OwnedStringBoundaryLayout,
    labels: &OwnedStringLabels,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x0", layout.result_len_offset);
            emitter.instruction("add x0, x0, #1");                              // include one byte for the optional trailing NUL
            emitter.instruction("bl __rt_heap_alloc");                          // allocate caller-owned result storage
            abi::store_at_offset(emitter, "x0", layout.owned_ptr_offset);
            abi::load_at_offset(emitter, "x9", layout.result_ptr_offset);
            abi::load_at_offset(emitter, "x10", layout.result_len_offset);
            emitter.instruction("mov x11, x0");                                 // retain the owned output base across the copy loop
            emitter.instruction("mov x12, #0");                                 // initialize the binary copy cursor
            emitter.label(&labels.copy);
            emitter.instruction("cmp x12, x10");                                // compare the cursor with the authoritative byte length
            emitter.instruction(&format!("b.hs {}", labels.copied));            // finish after every result byte is copied
            emitter.instruction("ldrb w13, [x9, x12]");                         // load one binary result byte
            emitter.instruction("strb w13, [x11, x12]");                        // copy one byte into caller-owned storage
            emitter.instruction("add x12, x12, #1");                            // advance the binary copy cursor
            emitter.instruction(&format!("b {}", labels.copy));                 // continue the result copy loop
            emitter.label(&labels.copied);
            emitter.instruction("strb wzr, [x11, x10]");                        // append a non-authoritative trailing NUL
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "rax", layout.result_len_offset);
            emitter.instruction("add rax, 1");                                  // include one byte for the optional trailing NUL
            emitter.instruction("call __rt_heap_alloc");                        // allocate caller-owned result storage
            abi::store_at_offset(emitter, "rax", layout.owned_ptr_offset);
            abi::load_at_offset(emitter, "r8", layout.result_ptr_offset);
            abi::load_at_offset(emitter, "r9", layout.result_len_offset);
            emitter.instruction("mov r10, rax");                                // retain the owned output base across the copy loop
            emitter.instruction("xor r11d, r11d");                              // initialize the binary copy cursor
            emitter.label(&labels.copy);
            emitter.instruction("cmp r11, r9");                                 // compare the cursor with the authoritative byte length
            emitter.instruction(&format!("jae {}", labels.copied));             // finish after every result byte is copied
            emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");              // load one binary result byte
            emitter.instruction("mov BYTE PTR [r10 + r11], al");                // copy one byte into caller-owned storage
            emitter.instruction("add r11, 1");                                  // advance the binary copy cursor
            emitter.instruction(&format!("jmp {}", labels.copy));               // continue the result copy loop
            emitter.label(&labels.copied);
            emitter.instruction("mov BYTE PTR [r10 + r9], 0");                  // append a non-authoritative trailing NUL
        }
    }
}

/// Releases a runtime-owned result unless it aliases one of the host's borrowed strings.
fn emit_release_result_unless_borrowed(
    emitter: &mut Emitter,
    export: &ExportedFunction,
    layout: &OwnedStringBoundaryLayout,
    skip_release: &str,
) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, layout.result_ptr_offset);
    for ((_, ty), offsets) in export.sig.params.iter().zip(&layout.param_offsets) {
        if *ty != PhpType::Str {
            continue;
        }
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offsets[0]);
                emitter.instruction("cmp x0, x9");                              // compare the result with one borrowed host string
                emitter.instruction(&format!("b.eq {skip_release}"));           // preserve storage still owned by the host
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r10", offsets[0]);
                emitter.instruction("cmp rax, r10");                            // compare the result with one borrowed host string
                emitter.instruction(&format!("je {skip_release}"));             // preserve storage still owned by the host
            }
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("bl __rt_heap_free_safe"),         // release runtime-owned result storage when present
        Arch::X86_64 => emitter.instruction("call __rt_heap_free_safe"),        // release runtime-owned result storage when present
    }
    emitter.label(skip_release);
}

/// Publishes caller-owned result storage and its authoritative byte length.
fn emit_publish_outputs(emitter: &mut Emitter, layout: &OwnedStringBoundaryLayout) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", layout.output_ptr_offset);
            abi::load_at_offset(emitter, "x10", layout.owned_ptr_offset);
            emitter.instruction("str x10, [x9]");                               // publish the caller-owned result pointer
            abi::load_at_offset(emitter, "x9", layout.output_len_offset);
            abi::load_at_offset(emitter, "x10", layout.result_len_offset);
            emitter.instruction("str x10, [x9]");                               // publish the authoritative result byte length
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r8", layout.output_ptr_offset);
            abi::load_at_offset(emitter, "r9", layout.owned_ptr_offset);
            emitter.instruction("mov QWORD PTR [r8], r9");                      // publish the caller-owned result pointer
            abi::load_at_offset(emitter, "r8", layout.output_len_offset);
            abi::load_at_offset(emitter, "r9", layout.result_len_offset);
            emitter.instruction("mov QWORD PTR [r8], r9");                      // publish the authoritative result byte length
        }
    }
}

/// Classifies a longjmp escape as allocation, runtime, or PHP-exception failure.
fn emit_classify_escape(emitter: &mut Emitter, labels: &OwnedStringLabels) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_STATUS, 0);
            emitter.instruction(&format!("cmp x9, #{STATUS_ALLOCATION_FAILURE}")); // distinguish allocation failure from other escapes
            emitter.instruction(&format!("b.eq {}", labels.allocation));        // report a recoverable allocation failure
            emitter.instruction(&format!("cbnz x9, {}", labels.runtime));       // report a non-Throwable runtime escape
            abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
            emitter.instruction(&format!("cbnz x9, {}", labels.exception));     // capture an escaping Throwable diagnostic
            emitter.instruction(&format!("b {}", labels.runtime));              // report a generic runtime escape
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", BOUNDARY_STATUS, 0);
            emitter.instruction(&format!("cmp r10, {STATUS_ALLOCATION_FAILURE}")); // distinguish allocation failure from other escapes
            emitter.instruction(&format!("je {}", labels.allocation));          // report a recoverable allocation failure
            emitter.instruction("test r10, r10");                               // distinguish Throwable propagation from status escapes
            emitter.instruction(&format!("jne {}", labels.runtime));            // report a non-Throwable runtime escape
            abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
            emitter.instruction("test r10, r10");                               // check whether a Throwable escaped the PHP body
            emitter.instruction(&format!("jne {}", labels.exception));          // capture an escaping Throwable diagnostic
            emitter.instruction(&format!("jmp {}", labels.runtime));            // report a generic runtime escape
        }
    }
}

/// Copies and consumes an escaping Throwable's message through stable boundary storage.
fn emit_capture_exception(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [x9, #8]");                            // load the escaping Throwable message pointer
            emitter.instruction("ldr x1, [x9, #16]");                           // load the escaping Throwable message length
            emitter.instruction("bl __rt_cdylib_set_error");                    // copy the diagnostic into stable boundary storage
            abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("bl __rt_decref_any");                          // release the consumed escaping Throwable object
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                // load the escaping Throwable message pointer
            emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");               // load the escaping Throwable message length
            emitter.instruction("call __rt_cdylib_set_error");                  // copy the diagnostic into stable boundary storage
            abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
            abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
            emitter.instruction("call __rt_decref_any");                        // release the consumed escaping Throwable object
        }
    }
}

/// Stores a compiler-emitted error through the target's diagnostic calling convention.
fn emit_set_static_error(emitter: &mut Emitter, error: (&str, usize)) {
    match emitter.target.arch {
        Arch::AArch64 => emit_set_static_error_aarch64(emitter, error),
        Arch::X86_64 => emit_set_static_error_x86_64(emitter, error),
    }
}

/// Leaves an entered boundary, records status, restores the wrapper frame, and returns.
fn emit_entered_return(
    emitter: &mut Emitter,
    layout: &OwnedStringBoundaryLayout,
    status: i32,
) {
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, status as i64);
    boundary::emit_leave_boundary(emitter, layout.concat_offset);
    emit_status_result(emitter, status);
    emit_restore_foreign_ctx(emitter, layout.saved_ctx_offset);
    abi::emit_frame_restore(emitter, layout.frame_size);
    abi::emit_return(emitter);
}

/// Records an argument failure before boundary entry and returns through the native frame.
fn emit_unentered_return(
    emitter: &mut Emitter,
    layout: &OwnedStringBoundaryLayout,
    status: i32,
) {
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, status as i64);
    emit_status_result(emitter, status);
    emit_restore_foreign_ctx(emitter, layout.saved_ctx_offset);
    abi::emit_frame_restore(emitter, layout.frame_size);
    abi::emit_return(emitter);
}

/// Hands the host back its own ctx register before leaving the boundary.
///
/// The ctx register is callee-saved on both targets, so the publish in the
/// wrapper prologue is a borrow that every return path has to give back.
fn emit_restore_foreign_ctx(emitter: &mut Emitter, saved_ctx_offset: usize) {
    crate::codegen_support::runtime::ctx::emit_ctx_restore_foreign(emitter, saved_ctx_offset);
}

/// Materializes one public status code in the target's integer return register.
fn emit_status_result(emitter: &mut Emitter, status: i32) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov w0, #{status}"));                 // return the recoverable boundary status
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov eax, {status}"));                 // return the recoverable boundary status
        }
    }
}

/// Emits an unconditional target branch to a local wrapper label.
fn emit_branch(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {label}")),            // continue through the selected AArch64 recovery path
        Arch::X86_64 => emitter.instruction(&format!("jmp {label}")),           // continue through the selected x86_64 recovery path
    }
}
