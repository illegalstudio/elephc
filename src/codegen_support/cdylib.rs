//! Purpose:
//! Emits the cdylib-only C ABI boundary, including scalar trampolines, owned
//! binary-string marshaling, lifecycle functions, and recoverable diagnostics.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` for `Emit::Cdylib` artifacts.
//!
//! Key details:
//! - Scalar exports retain their original C signatures and recover failures through
//!   `elephc_last_status()` plus the shared diagnostic channel.
//! - Exact `string -> string` exports return status plus caller-owned output storage.
//! - Nested native boundaries isolate concat scratch state and restore their caller.

use crate::codegen_support::abi;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Target};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::exports::{is_string_roundtrip_signature, ExportedFunction, ELEPHC_ABI_VERSION};
use crate::names::function_symbol;

mod boundary;

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

const AARCH64_FRAME_SIZE: usize = TRY_HANDLER_SLOT_SIZE + 80;
const AARCH64_FOOTER_OFFSET: usize = AARCH64_FRAME_SIZE - 16;
const AARCH64_INPUT_PTR_OFFSET: usize = TRY_HANDLER_SLOT_SIZE;
const AARCH64_INPUT_LEN_OFFSET: usize = TRY_HANDLER_SLOT_SIZE + 8;
const AARCH64_OUT_PTR_OFFSET: usize = TRY_HANDLER_SLOT_SIZE + 16;
const AARCH64_OUT_LEN_OFFSET: usize = TRY_HANDLER_SLOT_SIZE + 24;
const AARCH64_RESULT_PTR_OFFSET: usize = TRY_HANDLER_SLOT_SIZE + 32;
const AARCH64_RESULT_LEN_OFFSET: usize = TRY_HANDLER_SLOT_SIZE + 40;
const AARCH64_OWNED_PTR_OFFSET: usize = TRY_HANDLER_SLOT_SIZE + 48;
const AARCH64_CONCAT_OFFSET: usize = 8;

const X86_FRAME_SIZE: usize = TRY_HANDLER_SLOT_SIZE + 96;
const X86_HANDLER_BASE: usize = X86_FRAME_SIZE;
const X86_INPUT_PTR_OFFSET: usize = 8;
const X86_INPUT_LEN_OFFSET: usize = 16;
const X86_OUT_PTR_OFFSET: usize = 24;
const X86_OUT_LEN_OFFSET: usize = 32;
const X86_RESULT_PTR_OFFSET: usize = 40;
const X86_RESULT_LEN_OFFSET: usize = 48;
const X86_OWNED_PTR_OFFSET: usize = 56;
const X86_CONCAT_OFFSET: usize = 64;

/// Reserves the fixed, allocation-free state shared by every cdylib export.
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
        if is_string_roundtrip_signature(&export.sig) {
            emit_string_export(
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

/// Emits one exact `string -> string` status/out-parameter C ABI wrapper.
fn emit_string_export(
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
    emitter.blank();
    emitter.comment(&format!("#[Export] owned string boundary for PHP function {}", export.name));
    emitter.label_global(&exported);
    match target.arch {
        Arch::AArch64 => emit_string_export_aarch64(
            emitter,
            &suffix,
            &internal,
            invalid_error,
            allocation_error,
            runtime_error,
        ),
        Arch::X86_64 => emit_string_export_x86_64(
            emitter,
            &suffix,
            &internal,
            invalid_error,
            allocation_error,
            runtime_error,
        ),
    }
}

/// Emits the AArch64 implementation of the owned string boundary.
fn emit_string_export_aarch64(
    emitter: &mut Emitter,
    suffix: &str,
    internal: &str,
    invalid_error: (&str, usize),
    allocation_error: (&str, usize),
    runtime_error: (&str, usize),
) {
    let invalid = format!("L_cdylib_{suffix}_invalid");
    let invoke = format!("L_cdylib_{suffix}_invoke");
    let escaped = format!("L_cdylib_{suffix}_escaped");
    let allocation = format!("L_cdylib_{suffix}_allocation");
    let allocation_active = format!("L_cdylib_{suffix}_allocation_active");
    let allocation_release_done = format!("L_cdylib_{suffix}_allocation_release_done");
    let runtime = format!("L_cdylib_{suffix}_runtime");
    let exception = format!("L_cdylib_{suffix}_exception");
    let copy = format!("L_cdylib_{suffix}_copy");
    let copied = format!("L_cdylib_{suffix}_copied");
    let skip_release = format!("L_cdylib_{suffix}_skip_release");
    let finish = format!("L_cdylib_{suffix}_finish");
    let out_ptr_ready = format!("L_cdylib_{suffix}_out_ptr_ready");
    let out_len_ready = format!("L_cdylib_{suffix}_out_len_ready");

    emitter.instruction(&format!("sub sp, sp, #{AARCH64_FRAME_SIZE}"));         // reserve the aligned owned-string boundary frame
    emitter.instruction(&format!("stp x29, x30, [sp, #{AARCH64_FOOTER_OFFSET}]")); // save the native frame pointer and return address
    emitter.instruction(&format!("add x29, sp, #{AARCH64_FOOTER_OFFSET}"));     // establish the stable wrapper frame pointer
    emitter.instruction(&format!("str x0, [sp, #{AARCH64_INPUT_PTR_OFFSET}]")); // save the host input pointer across nested calls
    emitter.instruction(&format!("str x1, [sp, #{AARCH64_INPUT_LEN_OFFSET}]")); // save the authoritative host input length
    emitter.instruction(&format!("str x2, [sp, #{AARCH64_OUT_PTR_OFFSET}]"));   // save the caller output-pointer address
    emitter.instruction(&format!("str x3, [sp, #{AARCH64_OUT_LEN_OFFSET}]"));   // save the caller output-length address
    emitter.instruction(&format!("str xzr, [sp, #{AARCH64_RESULT_PTR_OFFSET}]")); // clear the borrowed PHP result pointer slot
    emitter.instruction(&format!("str xzr, [sp, #{AARCH64_OWNED_PTR_OFFSET}]")); // clear the caller-owned output pointer slot
    emit_clear_error_inline(emitter);
    boundary::emit_enter_boundary(emitter, AARCH64_CONCAT_OFFSET, suffix);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);

    emitter.instruction(&format!("cbz x2, {out_ptr_ready}"));                   // clear the output pointer only when the caller supplied its address
    emitter.instruction("str xzr, [x2]");                                       // clear the caller output pointer before validating arguments
    emitter.label(&out_ptr_ready);
    emitter.instruction(&format!("cbz x3, {out_len_ready}"));                   // clear the output length only when the caller supplied its address
    emitter.instruction("str xzr, [x3]");                                       // clear the caller output length before validating arguments
    emitter.label(&out_len_ready);
    emitter.instruction(&format!("cbz x2, {invalid}"));                         // route a missing required pointer to invalid-argument reporting
    emitter.instruction(&format!("cbz x3, {invalid}"));                         // route a missing required pointer to invalid-argument reporting
    emitter.instruction(&format!("cbz x1, {invoke}"));                          // accept NULL input storage only when its byte length is zero
    emitter.instruction(&format!("cbz x0, {invalid}"));                         // route a missing required pointer to invalid-argument reporting

    emitter.label(&invoke);
    emit_boundary_push_aarch64(emitter, &escaped, AARCH64_FOOTER_OFFSET);
    emitter.instruction(&format!("ldr x0, [sp, #{AARCH64_INPUT_PTR_OFFSET}]")); // restore the host input pointer for the PHP call
    emitter.instruction(&format!("ldr x1, [sp, #{AARCH64_INPUT_LEN_OFFSET}]")); // restore the host input length for the PHP call
    emitter.instruction(&format!("bl {internal}"));                             // invoke the internal PHP body through its native ABI
    emitter.instruction(&format!("str x1, [sp, #{AARCH64_RESULT_PTR_OFFSET}]")); // save the borrowed PHP result pointer
    emitter.instruction(&format!("str x2, [sp, #{AARCH64_RESULT_LEN_OFFSET}]")); // save or reload the authoritative PHP result length
    emitter.instruction("cmn x2, #1");                                          // test the allocation-failure sentinel returned as a string length
    emitter.instruction(&format!("b.eq {allocation_active}"));                  // recover a sentinel result while the native boundary is installed
    emitter.instruction("add x0, x2, #1");                                      // include one byte for the convenience trailing NUL
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate caller-owned storage for the exported string result
    emitter.instruction(&format!("str x0, [sp, #{AARCH64_OWNED_PTR_OFFSET}]")); // save the caller-owned output pointer
    emitter.instruction(&format!("ldr x9, [sp, #{AARCH64_RESULT_PTR_OFFSET}]")); // load the borrowed PHP result pointer for copying
    emitter.instruction(&format!("ldr x10, [sp, #{AARCH64_RESULT_LEN_OFFSET}]")); // save or reload the authoritative PHP result length
    emitter.instruction("mov x11, x0");                                         // keep the owned output base in a copy-loop register
    emitter.instruction("mov x12, #0");                                         // initialize the binary result copy cursor
    emitter.label(&copy);
    emitter.instruction("cmp x12, x10");                                        // compare the copy cursor with the result length
    emitter.instruction(&format!("b.hs {copied}"));                             // finish once every binary result byte has been copied
    emitter.instruction("ldrb w13, [x9, x12]");                                 // load one byte from the borrowed PHP result
    emitter.instruction("strb w13, [x11, x12]");                                // copy one byte into caller-owned output storage
    emitter.instruction("add x12, x12, #1");                                    // advance the binary result copy cursor
    emitter.instruction(&format!("b {copy}"));                                  // continue the binary-safe result copy loop
    emitter.label(&copied);
    emitter.instruction("strb wzr, [x11, x10]");                                // append the non-authoritative trailing NUL byte
    emitter.instruction(&format!("ldr x0, [sp, #{AARCH64_RESULT_PTR_OFFSET}]")); // load the PHP result pointer for ownership release
    emitter.instruction(&format!("ldr x9, [sp, #{AARCH64_INPUT_PTR_OFFSET}]")); // load the host-owned input pointer for alias comparison
    emitter.instruction("cmp x0, x9");                                          // test whether the PHP result aliases host-owned input
    emitter.instruction(&format!("b.eq {skip_release}"));                       // preserve a result that aliases the host-owned input buffer
    emitter.instruction("bl __rt_heap_free_safe");                              // release non-borrowed runtime storage when present
    emitter.label(&skip_release);
    emit_boundary_pop_aarch64(emitter, AARCH64_FOOTER_OFFSET);
    emitter.instruction(&format!("ldr x9, [sp, #{AARCH64_OUT_PTR_OFFSET}]"));   // load the caller output-pointer address
    emitter.instruction(&format!("ldr x10, [sp, #{AARCH64_OWNED_PTR_OFFSET}]")); // load the caller-owned output pointer
    emitter.instruction("str x10, [x9]");                                       // publish the caller-owned pointer or authoritative result length
    emitter.instruction(&format!("ldr x9, [sp, #{AARCH64_OUT_LEN_OFFSET}]"));   // load the caller output-length address
    emitter.instruction(&format!("ldr x10, [sp, #{AARCH64_RESULT_LEN_OFFSET}]")); // save or reload the authoritative PHP result length
    emitter.instruction("str x10, [x9]");                                       // publish the caller-owned pointer or authoritative result length
    emitter.instruction(&format!("mov w0, #{STATUS_OK}"));                      // return the successful native boundary status
    emitter.instruction(&format!("b {finish}"));                                // join the common native boundary teardown path

    emitter.label(&allocation_active);
    emit_boundary_pop_aarch64(emitter, AARCH64_FOOTER_OFFSET);
    emitter.instruction(&format!("b {allocation}"));                            // continue through allocation-failure cleanup and reporting

    emitter.label(&escaped);
    emit_boundary_pop_aarch64(emitter, AARCH64_FOOTER_OFFSET);
    abi::emit_load_symbol_to_reg(emitter, "x9", BOUNDARY_STATUS, 0);
    emitter.instruction(&format!("cmp x9, #{STATUS_ALLOCATION_FAILURE}"));      // classify or return a recoverable allocation failure
    emitter.instruction(&format!("b.eq {allocation}"));                         // continue through allocation-failure cleanup and reporting
    emitter.instruction(&format!("cbnz x9, {runtime}"));                        // continue through generic runtime-failure reporting
    abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
    emitter.instruction(&format!("cbnz x9, {exception}"));                      // continue through escaping-Throwable diagnostic capture
    emitter.instruction(&format!("b {runtime}"));                               // continue through generic runtime-failure reporting

    emitter.label(&exception);
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the escaping Throwable message pointer
    emitter.instruction("ldr x1, [x9, #16]");                                   // load the escaping Throwable message length
    emitter.instruction("bl __rt_cdylib_set_error");                            // copy the current diagnostic into stable boundary storage
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    emitter.instruction("bl __rt_decref_any");                                  // release the consumed escaping Throwable object
    emitter.instruction(&format!("mov w0, #{STATUS_PHP_EXCEPTION}"));           // return the recoverable PHP-exception status
    emitter.instruction(&format!("b {finish}"));                                // join the common native boundary teardown path

    emitter.label(&allocation);
    emitter.instruction(&format!("ldr x0, [sp, #{AARCH64_RESULT_PTR_OFFSET}]")); // load any partial PHP result for release
    emitter.instruction(&format!("ldr x9, [sp, #{AARCH64_INPUT_PTR_OFFSET}]")); // load the host input pointer for alias comparison
    emitter.instruction("cmp x0, x9");                                          // avoid releasing a result borrowed from the host input
    emitter.instruction(&format!("b.eq {allocation_release_done}"));            // skip result release when no owned PHP buffer exists
    emitter.instruction("bl __rt_heap_free_safe");                              // release non-borrowed runtime storage when present
    emitter.label(&allocation_release_done);
    emit_set_static_error_aarch64(emitter, allocation_error);
    emitter.instruction(&format!("mov w0, #{STATUS_ALLOCATION_FAILURE}"));      // classify or return a recoverable allocation failure
    emitter.instruction(&format!("b {finish}"));                                // join the common native boundary teardown path
    emitter.label(&runtime);
    emit_set_static_error_aarch64(emitter, runtime_error);
    emitter.instruction(&format!("mov w0, #{STATUS_RUNTIME_FAILURE}"));         // return the generic recoverable runtime-failure status
    emitter.instruction(&format!("b {finish}"));                                // join the common native boundary teardown path
    emitter.label(&invalid);
    emit_set_static_error_aarch64(emitter, invalid_error);
    emitter.instruction(&format!("mov w0, #{STATUS_INVALID_ARGUMENT}"));        // return the invalid-argument boundary status

    emitter.label(&finish);
    abi::emit_store_reg_to_symbol(emitter, "x0", BOUNDARY_STATUS, 0);
    boundary::emit_leave_boundary(emitter, AARCH64_CONCAT_OFFSET);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{AARCH64_FOOTER_OFFSET}]")); // restore the native frame pointer and return address
    emitter.instruction(&format!("add sp, sp, #{AARCH64_FRAME_SIZE}"));         // release the aligned owned-string boundary frame
    emitter.instruction("ret");                                                 // return to the current C-ABI caller
}

/// Emits the x86_64 System V implementation of the owned string boundary.
fn emit_string_export_x86_64(
    emitter: &mut Emitter,
    suffix: &str,
    internal: &str,
    invalid_error: (&str, usize),
    allocation_error: (&str, usize),
    runtime_error: (&str, usize),
) {
    let invalid = format!("L_cdylib_{suffix}_invalid");
    let invoke = format!("L_cdylib_{suffix}_invoke");
    let escaped = format!("L_cdylib_{suffix}_escaped");
    let allocation = format!("L_cdylib_{suffix}_allocation");
    let allocation_active = format!("L_cdylib_{suffix}_allocation_active");
    let allocation_release_done = format!("L_cdylib_{suffix}_allocation_release_done");
    let runtime = format!("L_cdylib_{suffix}_runtime");
    let exception = format!("L_cdylib_{suffix}_exception");
    let copy = format!("L_cdylib_{suffix}_copy");
    let copied = format!("L_cdylib_{suffix}_copied");
    let skip_release = format!("L_cdylib_{suffix}_skip_release");
    let finish = format!("L_cdylib_{suffix}_finish");
    let out_ptr_ready = format!("L_cdylib_{suffix}_out_ptr_ready");
    let out_len_ready = format!("L_cdylib_{suffix}_out_len_ready");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the stable wrapper frame pointer
    emitter.instruction(&format!("sub rsp, {X86_FRAME_SIZE}"));                 // reserve aligned owned-string boundary storage
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_INPUT_PTR_OFFSET}], rdi")); // save the host input pointer across nested calls
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_INPUT_LEN_OFFSET}], rsi")); // save the authoritative host input length
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_OUT_PTR_OFFSET}], rdx")); // save the caller output-pointer address
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_OUT_LEN_OFFSET}], rcx")); // save the caller output-length address
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_RESULT_PTR_OFFSET}], 0")); // clear the borrowed PHP result pointer slot
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_OWNED_PTR_OFFSET}], 0")); // clear the caller-owned output pointer slot
    emit_clear_error_inline(emitter);
    boundary::emit_enter_boundary(emitter, X86_CONCAT_OFFSET, suffix);
    emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);

    emitter.instruction("test rdx, rdx");                                       // check whether the caller supplied an output pointer address
    emitter.instruction(&format!("je {out_ptr_ready}"));                        // clear the output pointer only when the caller supplied its address
    emitter.instruction("mov QWORD PTR [rdx], 0");                              // clear the caller output pointer before validating arguments
    emitter.label(&out_ptr_ready);
    emitter.instruction("test rcx, rcx");                                       // check whether the caller supplied an output length address
    emitter.instruction(&format!("je {out_len_ready}"));                        // clear the output length only when the caller supplied its address
    emitter.instruction("mov QWORD PTR [rcx], 0");                              // clear the caller output length before validating arguments
    emitter.label(&out_len_ready);
    emitter.instruction("test rdx, rdx");                                       // validate the required output pointer address
    emitter.instruction(&format!("je {invalid}"));                              // route a missing required pointer to invalid-argument reporting
    emitter.instruction("test rcx, rcx");                                       // validate the required output length address
    emitter.instruction(&format!("je {invalid}"));                              // route a missing required pointer to invalid-argument reporting
    emitter.instruction("test rsi, rsi");                                       // accept NULL input storage only for an empty input string
    emitter.instruction(&format!("je {invoke}"));                               // accept NULL input storage only when its byte length is zero
    emitter.instruction("test rdi, rdi");                                       // validate storage for a non-empty input string
    emitter.instruction(&format!("je {invalid}"));                              // route a missing required pointer to invalid-argument reporting

    emitter.label(&invoke);
    emit_boundary_push_x86_64(emitter, &escaped, X86_HANDLER_BASE);
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {X86_INPUT_PTR_OFFSET}]")); // restore the host input pointer for the PHP call
    emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {X86_INPUT_LEN_OFFSET}]")); // restore the host input length for the PHP call
    emitter.instruction(&format!("call {internal}"));                           // invoke the internal PHP body through its native ABI
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_RESULT_PTR_OFFSET}], rax")); // save the borrowed PHP result pointer
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_RESULT_LEN_OFFSET}], rdx")); // save or reload the authoritative PHP result length
    emitter.instruction("cmp rdx, -1");                                         // test the allocation-failure sentinel returned as a string length
    emitter.instruction(&format!("je {allocation_active}"));                    // recover a sentinel result while the native boundary is installed
    emitter.instruction("lea rax, [rdx + 1]");                                  // include one byte for the convenience trailing NUL
    emitter.instruction("call __rt_heap_alloc");                                // allocate caller-owned storage for the exported string result
    emitter.instruction(&format!("mov QWORD PTR [rbp - {X86_OWNED_PTR_OFFSET}], rax")); // save the caller-owned output pointer
    emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {X86_RESULT_PTR_OFFSET}]")); // load the borrowed PHP result pointer for copying
    emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {X86_RESULT_LEN_OFFSET}]")); // save or reload the authoritative PHP result length
    emitter.instruction("mov r10, rax");                                        // keep the owned output base in a copy-loop register
    emitter.instruction("xor r11d, r11d");                                      // initialize the binary result copy cursor
    emitter.label(&copy);
    emitter.instruction("cmp r11, r9");                                         // compare the copy cursor with the result length
    emitter.instruction(&format!("jae {copied}"));                              // finish once every binary result byte has been copied
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");                      // load one byte from the borrowed PHP result
    emitter.instruction("mov BYTE PTR [r10 + r11], al");                        // copy one byte into caller-owned output storage
    emitter.instruction("add r11, 1");                                          // advance the binary result copy cursor
    emitter.instruction(&format!("jmp {copy}"));                                // continue the binary-safe result copy loop
    emitter.label(&copied);
    emitter.instruction("mov BYTE PTR [r10 + r9], 0");                          // append the non-authoritative trailing NUL byte
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {X86_RESULT_PTR_OFFSET}]")); // load the PHP result pointer for ownership release
    emitter.instruction(&format!("cmp rax, QWORD PTR [rbp - {X86_INPUT_PTR_OFFSET}]")); // test whether the result aliases host-owned input
    emitter.instruction(&format!("je {skip_release}"));                         // preserve a result that aliases the host-owned input buffer
    emitter.instruction("call __rt_heap_free_safe");                            // release non-borrowed runtime storage when present
    emitter.label(&skip_release);
    emit_boundary_pop_x86_64(emitter, X86_HANDLER_BASE);
    emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {X86_OUT_PTR_OFFSET}]")); // load the caller output-pointer address
    emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {X86_OWNED_PTR_OFFSET}]")); // load the caller-owned output pointer
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the caller-owned pointer or authoritative result length
    emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {X86_OUT_LEN_OFFSET}]")); // load the caller output-length address
    emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {X86_RESULT_LEN_OFFSET}]")); // save or reload the authoritative PHP result length
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the caller-owned pointer or authoritative result length
    emitter.instruction(&format!("mov eax, {STATUS_OK}"));                      // return the successful native boundary status
    emitter.instruction(&format!("jmp {finish}"));                              // join the common native boundary teardown path

    emitter.label(&allocation_active);
    emit_boundary_pop_x86_64(emitter, X86_HANDLER_BASE);
    emitter.instruction(&format!("jmp {allocation}"));                          // continue through allocation-failure cleanup and reporting

    emitter.label(&escaped);
    emit_boundary_pop_x86_64(emitter, X86_HANDLER_BASE);
    abi::emit_load_symbol_to_reg(emitter, "r9", BOUNDARY_STATUS, 0);
    emitter.instruction(&format!("cmp r9, {STATUS_ALLOCATION_FAILURE}"));       // classify or return a recoverable allocation failure
    emitter.instruction(&format!("je {allocation}"));                           // continue through allocation-failure cleanup and reporting
    emitter.instruction("test r9, r9");                                         // distinguish Throwable propagation from status escapes
    emitter.instruction(&format!("jne {runtime}"));                             // continue through generic runtime-failure reporting
    abi::emit_load_symbol_to_reg(emitter, "r9", "_exc_value", 0);
    emitter.instruction("test r9, r9");                                         // check whether a Throwable escaped the PHP body
    emitter.instruction(&format!("jne {exception}"));                           // continue through escaping-Throwable diagnostic capture
    emitter.instruction(&format!("jmp {runtime}"));                             // continue through generic runtime-failure reporting

    emitter.label(&exception);
    emitter.instruction("mov rdi, QWORD PTR [r9 + 8]");                         // load the escaping Throwable message pointer
    emitter.instruction("mov rsi, QWORD PTR [r9 + 16]");                        // load the escaping Throwable message length
    emitter.instruction("call __rt_cdylib_set_error");                          // copy the current diagnostic into stable boundary storage
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    emitter.instruction("call __rt_decref_any");                                // release the consumed escaping Throwable object
    emitter.instruction(&format!("mov eax, {STATUS_PHP_EXCEPTION}"));           // return the recoverable PHP-exception status
    emitter.instruction(&format!("jmp {finish}"));                              // join the common native boundary teardown path

    emitter.label(&allocation);
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {X86_RESULT_PTR_OFFSET}]")); // load any partial PHP result for release
    emitter.instruction(&format!("cmp rax, QWORD PTR [rbp - {X86_INPUT_PTR_OFFSET}]")); // avoid releasing host-owned input storage
    emitter.instruction(&format!("je {allocation_release_done}"));              // skip result release when no owned PHP buffer exists
    emitter.instruction("call __rt_heap_free_safe");                            // release non-borrowed runtime storage when present
    emitter.label(&allocation_release_done);
    emit_set_static_error_x86_64(emitter, allocation_error);
    emitter.instruction(&format!("mov eax, {STATUS_ALLOCATION_FAILURE}"));      // classify or return a recoverable allocation failure
    emitter.instruction(&format!("jmp {finish}"));                              // join the common native boundary teardown path
    emitter.label(&runtime);
    emit_set_static_error_x86_64(emitter, runtime_error);
    emitter.instruction(&format!("mov eax, {STATUS_RUNTIME_FAILURE}"));         // return the generic recoverable runtime-failure status
    emitter.instruction(&format!("jmp {finish}"));                              // join the common native boundary teardown path
    emitter.label(&invalid);
    emit_set_static_error_x86_64(emitter, invalid_error);
    emitter.instruction(&format!("mov eax, {STATUS_INVALID_ARGUMENT}"));        // return the invalid-argument boundary status

    emitter.label(&finish);
    abi::emit_store_reg_to_symbol(emitter, "rax", BOUNDARY_STATUS, 0);
    boundary::emit_leave_boundary(emitter, X86_CONCAT_OFFSET);
    emitter.instruction("mov rsp, rbp");                                        // release wrapper locals through the stable frame pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the current C-ABI caller
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
        emit_clear_error_inline(emitter);
        emit_reset_concat_inline(emitter);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_ACTIVE, 0);
        emit_store_immediate_to_symbol(emitter, BOUNDARY_STATUS, STATUS_OK as i64);
        if lifecycle == "elephc_init" {
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
    use crate::codegen_support::platform::{Platform, Target};
    use crate::span::Span;
    use crate::types::{FunctionSig, PhpType};

    /// Builds one exact `string -> string` export fixture.
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
        assert!(data.emit(target).contains(LAST_ERROR_BUFFER));
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
    }
}
