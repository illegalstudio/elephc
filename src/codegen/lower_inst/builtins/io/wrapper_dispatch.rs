//! Purpose:
//! Userspace wrapper reads, stat probes, and mutations.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// The `url_stat()` flags PHP hands a wrapper, per stat-family builtin.
///
/// Read off reference PHP with a wrapper that echoes its `$flags`, not inferred from the two
/// documented `STREAM_URL_STAT_*` constants: PHP also sets an internal no-cache bit (4) that
/// userland never sees named. The full observed table is
/// `stat 4 · lstat 5 · filesize 4 · filemtime 4 · file_exists 6 · is_file 6 · is_dir 6 ·
/// is_readable 6 · is_writable 6`, i.e. NOCACHE everywhere, plus LINK for `lstat` and QUIET
/// for the predicates. A wrapper that branches on QUIET to decide whether to warn therefore
/// sees the same value it would under PHP.
pub(super) const URL_STAT_FLAGS_NOCACHE: u64 = 4;
/// `lstat()`: the no-cache bit plus `STREAM_URL_STAT_LINK`.
pub(super) const URL_STAT_FLAGS_LINK: u64 = 5;
/// The existence predicates: the no-cache bit plus `STREAM_URL_STAT_QUIET`.
const URL_STAT_FLAGS_QUIET: u64 = 6;

/// Emits the wrapper-vs-filesystem dispatch for `readfile()`.
pub(super) fn emit_readfile_wrapper_dispatch(ctx: &mut FunctionContext<'_>) {
    let wrapper = ctx.next_label("readfile_wrapper");
    let after = ctx.next_label("readfile_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across the wrapper probe
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the readfile path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the readfile path length
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to the wrapper-scheme probe
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to the wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for the chosen readfile helper
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for the chosen readfile helper
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes stream through the wrapper helper
            abi::emit_call_label(ctx.emitter, "__rt_readfile");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip the wrapper readfile helper after native streaming
            ctx.emitter.label(&wrapper);
            abi::emit_call_label(ctx.emitter, "__rt_readfile_wrapper");
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across the wrapper probe
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the readfile path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the readfile path length
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to the wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to the wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a registered wrapper
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for the chosen readfile helper
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for the chosen readfile helper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes stream through the wrapper helper
            abi::emit_call_label(ctx.emitter, "__rt_readfile");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip the wrapper readfile helper after native streaming
            ctx.emitter.label(&wrapper);
            abi::emit_call_label(ctx.emitter, "__rt_readfile_wrapper");
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// Lowers `file_exists()` through userspace `url_stat()` before filesystem stat.
pub(super) fn lower_file_exists_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "file_exists", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "file_exists")?;
    emit_file_exists_wrapper_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Emits `file_exists()` wrapper `url_stat()` dispatch for the loaded path.
pub(super) fn emit_file_exists_wrapper_dispatch(ctx: &mut FunctionContext<'_>) {
    let fallback = ctx.next_label("file_exists_fs");
    let done = ctx.next_label("file_exists_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path/result scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for filesystem stat
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for filesystem stat
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to url_stat
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to url_stat
            ctx.emitter
                .instruction(&format!("mov x2, #{}", URL_STAT_FLAGS_QUIET));    // PHP passes NOCACHE|QUIET for the existence predicates
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat");
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether a registered wrapper scheme matched
            ctx.emitter.instruction(&format!("cbz w9, {}", fallback));          // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction("ldr x10, [x0]");                           // load the boxed url_stat result tag
            ctx.emitter.instruction("cmp x10, #3");                             // tag 3 means PHP false from url_stat
            ctx.emitter.instruction("cset x10, ne");                            // file exists when url_stat returned a non-false value
            ctx.emitter.instruction("str x10, [sp, #0]");                       // preserve the boolean result across boxed-result release
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore the boolean file_exists result
            ctx.emitter.instruction(&format!("b {}", done));                    // skip filesystem stat after wrapper handling
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for filesystem stat
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for filesystem stat
            abi::emit_call_label(ctx.emitter, "__rt_file_exists");
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path/result scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path/result scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for filesystem stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for filesystem stat
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to url_stat
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to url_stat
            ctx.emitter
                .instruction(&format!("mov edx, {}", URL_STAT_FLAGS_QUIET));    // PHP passes NOCACHE|QUIET for the existence predicates
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat");
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether a registered wrapper scheme matched
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", fallback));               // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // load the boxed url_stat result tag
            ctx.emitter.instruction("mov rdi, rax");                            // preserve the boxed result pointer across boolean materialization
            ctx.emitter.instruction("cmp r10, 3");                              // tag 3 means PHP false from url_stat
            ctx.emitter.instruction("setne al");                                // file exists when url_stat returned a non-false value
            ctx.emitter.instruction("movzx eax, al");                           // widen the boolean into the result register
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the boolean result across boxed-result release
            ctx.emitter.instruction("mov rax, rdi");                            // pass the boxed result pointer to decref
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the boolean file_exists result
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip filesystem stat after wrapper handling
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for filesystem stat
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for filesystem stat
            abi::emit_call_label(ctx.emitter, "__rt_file_exists");
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 16");                             // release path/result scratch storage
        }
    }
}

/// Lowers `stat()`/`lstat()` through userspace `url_stat()` before filesystem stat.
///
/// `stat()` was the one member of the stat family that never consulted a registered wrapper:
/// `file_exists()`, `filesize()` and `is_file()` all probe `url_stat()` first, but `stat()`
/// went straight to the filesystem, so `stat("scheme://x")` on a wrapper returned the
/// filesystem's answer for a path that does not exist there. The flags are PHP's own,
/// observed from a wrapper that echoes them: `stat()` passes 4 and `lstat()` 5.
pub(super) fn lower_path_stat_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    filesystem_label: &str,
    url_stat_flags: u64,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    emit_path_stat_wrapper_dispatch(ctx, name, filesystem_label, url_stat_flags)?;
    store_if_result(ctx, inst)
}

/// Emits the `url_stat()`-then-filesystem dispatch for a loaded path.
///
/// The wrapper arm needs no boxing: `__rt_user_wrapper_url_stat` already returns the boxed
/// `array|false` these builtins hand back, which is the same shape
/// `box_stat_array_or_false_result` builds for the filesystem arm.
fn emit_path_stat_wrapper_dispatch(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    filesystem_label: &str,
    url_stat_flags: u64,
) -> Result<()> {
    let fallback = ctx.next_label(&format!("{}_fs", name));
    let done = ctx.next_label(&format!("{}_done", name));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across the wrapper probe
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for filesystem stat
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for filesystem stat
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to url_stat
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to url_stat
            ctx.emitter.instruction(&format!("mov x2, #{}", url_stat_flags));   // pass PHP's own url_stat flags for this builtin
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat");
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether a registered wrapper scheme matched
            ctx.emitter.instruction(&format!("cbz w9, {}", fallback));          // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction(&format!("b {}", done));                    // the wrapper result is already the boxed array-or-false
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for filesystem stat
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for filesystem stat
            abi::emit_call_label(ctx.emitter, filesystem_label);
            box_stat_array_or_false_result(ctx);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across the wrapper probe
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for filesystem stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for filesystem stat
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to url_stat
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to url_stat
            ctx.emitter.instruction(&format!("mov edx, {}", url_stat_flags));   // pass PHP's own url_stat flags for this builtin
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat");
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether a registered wrapper scheme matched
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", fallback));               // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction(&format!("jmp {}", done));                  // the wrapper result is already the boxed array-or-false
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for filesystem stat
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for filesystem stat
            abi::emit_call_label(ctx.emitter, filesystem_label);
            box_stat_array_or_false_result(ctx);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
    Ok(())
}

/// Lowers `filesize()` through userspace `url_stat()['size']` before filesystem stat.
pub(super) fn lower_filesize_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "filesize", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "filesize")?;
    emit_url_stat_field_or_fallback(ctx, "__rt_filesize", 0);
    store_if_result(ctx, inst)
}

/// Lowers `is_file()` through userspace `url_stat()['mode']` before filesystem stat.
pub(super) fn lower_is_file_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "is_file", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "is_file")?;
    emit_is_file_wrapper_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Emits a wrapper url_stat field lookup with a native filesystem fallback.
pub(super) fn emit_url_stat_field_or_fallback(
    ctx: &mut FunctionContext<'_>,
    fallback_runtime: &str,
    field_selector: usize,
) {
    let fallback = ctx.next_label("url_stat_field_fs");
    let done = ctx.next_label("url_stat_field_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across url_stat
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for filesystem fallback
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for filesystem fallback
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to url_stat field lookup
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to url_stat field lookup
            ctx.emitter.instruction(&format!("mov x2, #{}", field_selector));   // select the url_stat field to extract
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_field");
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether a registered wrapper scheme matched
            ctx.emitter.instruction(&format!("cbz w9, {}", fallback));          // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction(&format!("b {}", done));                    // keep the wrapper field result
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for filesystem fallback
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for filesystem fallback
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across url_stat
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for filesystem fallback
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for filesystem fallback
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to url_stat field lookup
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to url_stat field lookup
            ctx.emitter.instruction(&format!("mov edx, {}", field_selector));   // select the url_stat field to extract
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_url_stat_field");
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether a registered wrapper scheme matched
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", fallback));               // fall back to filesystem stat when no wrapper matched
            ctx.emitter.instruction(&format!("jmp {}", done));                  // keep the wrapper field result
            ctx.emitter.label(&fallback);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for filesystem fallback
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for filesystem fallback
            abi::emit_call_label(ctx.emitter, fallback_runtime);
            ctx.emitter.label(&done);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// Emits `is_file()` wrapper url_stat mode extraction plus file-type test.
pub(super) fn emit_is_file_wrapper_dispatch(ctx: &mut FunctionContext<'_>) {
    emit_url_stat_field_or_fallback(ctx, "__rt_is_file", 1);
    let no_wrapper = ctx.next_label("is_file_no_wrapper_adjust");
    let done = ctx.next_label("is_file_adjust_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_url_stat_matched");
            ctx.emitter.instruction("ldrb w9, [x9]");                           // read whether the mode came from a wrapper
            ctx.emitter.instruction(&format!("cbz w9, {}", no_wrapper));        // native fallback already returned a boolean
            ctx.emitter.instruction("and x0, x0, #0xF000");                     // isolate mode file-type bits from wrapper url_stat
            ctx.emitter.instruction("mov x9, #0x8000");                         // materialize S_IFREG for regular files
            ctx.emitter.instruction("cmp x0, x9");                              // compare wrapper mode against regular-file type
            ctx.emitter.instruction("cset x0, eq");                             // return true only for regular-file modes
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the native-result path
            ctx.emitter.label(&no_wrapper);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_url_stat_matched");
            ctx.emitter.instruction("movzx r9d, BYTE PTR [r9]");                // read whether the mode came from a wrapper
            ctx.emitter.instruction("test r9d, r9d");                           // test the url_stat matched flag
            ctx.emitter.instruction(&format!("jz {}", no_wrapper));             // native fallback already returned a boolean
            ctx.emitter.instruction("and eax, 0xF000");                         // isolate mode file-type bits from wrapper url_stat
            ctx.emitter.instruction("cmp eax, 0x8000");                         // compare wrapper mode against regular-file type
            ctx.emitter.instruction("sete al");                                 // return true only for regular-file modes
            ctx.emitter.instruction("movzx eax, al");                           // widen the boolean into the result register
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the native-result path
            ctx.emitter.label(&no_wrapper);
            ctx.emitter.label(&done);
        }
    }
}

/// Lowers a single-path wrapper-aware filesystem mutation.
pub(super) fn lower_single_path_wrapper_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
    vtable_slot: usize,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    emit_single_path_wrapper_dispatch(ctx, runtime_label, vtable_slot);
    store_if_result(ctx, inst)
}

/// Emits wrapper dispatch for a single-path mutation with native fallback.
pub(super) fn emit_single_path_wrapper_dispatch(
    ctx: &mut FunctionContext<'_>,
    libc_helper: &str,
    vtable_slot: usize,
) {
    let wrapper = ctx.next_label("path_op_wrapper");
    let after = ctx.next_label("path_op_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across the wrapper probe
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for the chosen helper
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for the chosen helper
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to the wrapper-scheme probe
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to the wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for the chosen helper
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for the chosen helper
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use userspace path-op dispatch
            abi::emit_call_label(ctx.emitter, libc_helper);
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper path-op after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov x0, x1");                              // pass the wrapper path pointer
            ctx.emitter.instruction("mov x1, x2");                              // pass the wrapper path length
            ctx.emitter.instruction(&format!("mov x2, #{}", vtable_slot));      // pass the wrapper vtable slot
            ctx.emitter.instruction("mov x3, #0");                              // pass default mode/options argument
            ctx.emitter.instruction("mov x4, #0");                              // pass default value/options argument
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across the wrapper probe
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer for the chosen helper
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length for the chosen helper
            ctx.emitter.instruction("mov rdi, rax");                            // pass the path pointer to the wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the path length to the wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a registered wrapper
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for the chosen helper
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for the chosen helper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use userspace path-op dispatch
            abi::emit_call_label(ctx.emitter, libc_helper);
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper path-op after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rdi, rax");                            // pass the wrapper path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the wrapper path length
            ctx.emitter.instruction(&format!("mov rdx, {}", vtable_slot));      // pass the wrapper vtable slot
            ctx.emitter.instruction("xor ecx, ecx");                            // pass default mode/options argument
            ctx.emitter.instruction("xor r8d, r8d");                            // pass default value/options argument
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// Emits PHAR-aware `unlink()` dispatch with wrapper/native fallback.
pub(super) fn emit_unlink_maybe_phar_dispatch(ctx: &mut FunctionContext<'_>) {
    let not_phar = ctx.next_label("unlink_not_phar");
    let phar_fail = ctx.next_label("unlink_phar_fail");
    let after = ctx.next_label("unlink_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path scratch storage across the PHAR probe
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the unlink path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the unlink path length
            ctx.emitter.instruction("cmp x2, #7");                              // path must be at least "phar://" long
            ctx.emitter.instruction(&format!("b.lt {}", not_phar));             // shorter paths use normal unlink dispatch
            ctx.emitter.instruction("ldrb w9, [x1, #0]");                       // read scheme byte 0
            ctx.emitter.instruction("cmp w9, #112");                            // compare with 'p'
            ctx.emitter.instruction(&format!("b.ne {}", not_phar));             // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("ldrb w9, [x1, #1]");                       // read scheme byte 1
            ctx.emitter.instruction("cmp w9, #104");                            // compare with 'h'
            ctx.emitter.instruction(&format!("b.ne {}", not_phar));             // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("ldrb w9, [x1, #2]");                       // read scheme byte 2
            ctx.emitter.instruction("cmp w9, #97");                             // compare with 'a'
            ctx.emitter.instruction(&format!("b.ne {}", not_phar));             // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("ldrb w9, [x1, #3]");                       // read scheme byte 3
            ctx.emitter.instruction("cmp w9, #114");                            // compare with 'r'
            ctx.emitter.instruction(&format!("b.ne {}", not_phar));             // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("ldrb w9, [x1, #4]");                       // read scheme separator byte
            ctx.emitter.instruction("cmp w9, #58");                             // compare with ':'
            ctx.emitter.instruction(&format!("b.ne {}", not_phar));             // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("ldrb w9, [x1, #5]");                       // read first slash byte
            ctx.emitter.instruction("cmp w9, #47");                             // compare with '/'
            ctx.emitter.instruction(&format!("b.ne {}", not_phar));             // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("ldrb w9, [x1, #6]");                       // read second slash byte
            ctx.emitter.instruction("cmp w9, #47");                             // compare with '/'
            ctx.emitter.instruction(&format!("b.ne {}", not_phar));             // non-PHAR scheme uses normal unlink dispatch
            abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_phar_delete_url_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR delete bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", phar_fail));         // missing bridge makes PHAR unlink fail
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // bridge arg 0 = full phar:// URL pointer
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // bridge arg 1 = full phar:// URL length
            ctx.emitter.instruction("blr x9");                                  // delete the archive entry through elephc-phar
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge success flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize bridge result to PHP bool
            ctx.emitter.instruction(&format!("b {}", after));                   // skip native unlink fallback for PHAR URLs
            ctx.emitter.label(&phar_fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false for failed PHAR unlink
            ctx.emitter.instruction(&format!("b {}", after));                   // skip native unlink fallback for PHAR URLs
            ctx.emitter.label(&not_phar);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // restore the path pointer for normal unlink dispatch
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // restore the path length for normal unlink dispatch
            emit_single_path_wrapper_dispatch(ctx, "__rt_unlink", STREAM_WRAPPER_UNLINK_SLOT);
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add sp, sp, #16");                         // release path scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve path scratch storage across the PHAR probe
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the unlink path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the unlink path length
            ctx.emitter.instruction("cmp rdx, 7");                              // path must be at least "phar://" long
            ctx.emitter.instruction(&format!("jl {}", not_phar));               // shorter paths use normal unlink dispatch
            ctx.emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");            // compare scheme byte 0 with 'p'
            ctx.emitter.instruction(&format!("jne {}", not_phar));              // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");            // compare scheme byte 1 with 'h'
            ctx.emitter.instruction(&format!("jne {}", not_phar));              // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");            // compare scheme byte 2 with 'a'
            ctx.emitter.instruction(&format!("jne {}", not_phar));              // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");            // compare scheme byte 3 with 'r'
            ctx.emitter.instruction(&format!("jne {}", not_phar));              // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("cmp BYTE PTR [rax + 4], 0x3A");            // compare scheme separator with ':'
            ctx.emitter.instruction(&format!("jne {}", not_phar));              // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("cmp BYTE PTR [rax + 5], 0x2F");            // compare first slash byte
            ctx.emitter.instruction(&format!("jne {}", not_phar));              // non-PHAR scheme uses normal unlink dispatch
            ctx.emitter.instruction("cmp BYTE PTR [rax + 6], 0x2F");            // compare second slash byte
            ctx.emitter.instruction(&format!("jne {}", not_phar));              // non-PHAR scheme uses normal unlink dispatch
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_elephc_phar_delete_url_fn", 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR delete bridge was published
            ctx.emitter.instruction(&format!("jz {}", phar_fail));              // missing bridge makes PHAR unlink fail
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // bridge arg 0 = full phar:// URL pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // bridge arg 1 = full phar:// URL length
            ctx.emitter.instruction("call r10");                                // delete the archive entry through elephc-phar
            ctx.emitter.instruction("test rax, rax");                           // test the bridge success flag
            ctx.emitter.instruction("setne al");                                // normalize bridge result to PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip native unlink fallback for PHAR URLs
            ctx.emitter.label(&phar_fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false for failed PHAR unlink
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip native unlink fallback for PHAR URLs
            ctx.emitter.label(&not_phar);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the path pointer for normal unlink dispatch
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // restore the path length for normal unlink dispatch
            emit_single_path_wrapper_dispatch(ctx, "__rt_unlink", STREAM_WRAPPER_UNLINK_SLOT);
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add rsp, 16");                             // release path scratch storage
        }
    }
}

/// Lowers `rename()` through userspace wrapper rename dispatch before libc rename.
pub(super) fn lower_rename_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "rename", 2)?;
    let from = expect_operand(inst, 0)?;
    let to = expect_operand(inst, 1)?;
    let wrapper = ctx.next_label("rename_wrapper");
    let after = ctx.next_label("rename_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, from, "rename source")?;
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve source and destination path scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the source path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the source path length
            load_string_to_result(ctx, to, "rename destination")?;
            ctx.emitter.instruction("str x1, [sp, #16]");                       // preserve the destination path pointer
            ctx.emitter.instruction("str x2, [sp, #24]");                       // preserve the destination path length
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass source path pointer to wrapper-scheme probe
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass source path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered source scheme uses wrapper rename
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // pass source path pointer to native rename
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // pass source path length to native rename
            ctx.emitter.instruction("ldr x3, [sp, #16]");                       // pass destination path pointer to native rename
            ctx.emitter.instruction("ldr x4, [sp, #24]");                       // pass destination path length to native rename
            abi::emit_call_label(ctx.emitter, "__rt_rename");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper rename after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass source path pointer to wrapper rename
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass source path length to wrapper rename
            ctx.emitter.instruction("ldr x2, [sp, #16]");                       // pass destination path pointer to wrapper rename
            ctx.emitter.instruction("ldr x3, [sp, #24]");                       // pass destination path length to wrapper rename
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_rename");
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add sp, sp, #32");                         // release path scratch storage
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, from, "rename source")?;
            ctx.emitter.instruction("sub rsp, 32");                             // reserve source and destination path scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the source path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the source path length
            load_string_to_result(ctx, to, "rename destination")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the destination path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rdx");           // preserve the destination path length
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass source path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass source path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the source scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered source scheme uses wrapper rename
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // pass source path pointer to native rename
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // pass source path length to native rename
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass destination path pointer to native rename
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass destination path length to native rename
            abi::emit_call_label(ctx.emitter, "__rt_rename");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper rename after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass source path pointer to wrapper rename
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass source path length to wrapper rename
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // pass destination path pointer to wrapper rename
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");           // pass destination path length to wrapper rename
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_rename");
            ctx.emitter.label(&after);
            ctx.emitter.instruction("add rsp, 32");                             // release path scratch storage
        }
    }
    store_if_result(ctx, inst)
}
