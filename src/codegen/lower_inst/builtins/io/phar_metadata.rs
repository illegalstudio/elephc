//! Purpose:
//! PHAR per-file metadata, archive signing, and entry listing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `__elephc_phar_get_file_metadata()` into the per-file metadata-read bridge.
pub(crate) fn lower_elephc_phar_get_file_metadata(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_file_metadata_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_get_file_metadata",
        "_elephc_phar_get_file_metadata_fn",
    )
}

/// Lowers `__elephc_phar_archive_valid(path, is_data)` into the archive-validity bridge.
///
/// The bridge takes `(path_ptr, path_len, is_data)` and returns 1 when the archive can
/// be opened, so it reuses the same `(string, int) -> bool` shape as the compression
/// setters.
pub(crate) fn lower_elephc_phar_archive_valid(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_archive_valid_function_pointer(ctx);
    emit_phar_path_int_to_bool_bridge(
        ctx,
        inst,
        "__elephc_phar_archive_valid",
        "_elephc_phar_archive_valid_fn",
    )
}

/// Lowers `__elephc_phar_set_file_metadata()` into the per-file metadata-write bridge.
/// The single `phar://archive/entry` URL argument is split by the bridge, so this
/// reuses the same `(url, data) -> bool` shape as the archive-level metadata writer.
pub(crate) fn lower_elephc_phar_set_file_metadata(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_set_file_metadata_function_pointer(ctx);
    emit_phar_set_string_bridge(
        ctx,
        inst,
        "__elephc_phar_set_file_metadata",
        "_elephc_phar_set_file_metadata_fn",
    )
}

/// Lowers `__elephc_phar_gzip_archive(src)` into the whole-archive gzip bridge,
/// returning the written destination path (or an empty string on failure).
pub(crate) fn lower_elephc_phar_gzip_archive(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_gzip_archive_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_gzip_archive",
        "_elephc_phar_gzip_archive_fn",
    )
}

/// Lowers `__elephc_phar_bzip2_archive(src)` into the whole-archive bzip2 bridge,
/// returning the written destination path (or an empty string on failure).
pub(crate) fn lower_elephc_phar_bzip2_archive(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_bzip2_archive_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_bzip2_archive",
        "_elephc_phar_bzip2_archive_fn",
    )
}

/// Lowers `__elephc_phar_decompress_archive(src)` into the whole-archive decompression
/// bridge, returning the written destination path (or an empty string on failure).
pub(crate) fn lower_elephc_phar_decompress_archive(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_decompress_archive_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_decompress_archive",
        "_elephc_phar_decompress_archive_fn",
    )
}

/// Lowers `__elephc_phar_sign_openssl(path, keyPem)` into the RSA-SHA1 signing bridge.
pub(crate) fn lower_elephc_phar_sign_openssl(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() == 3 {
        publish_phar_sign_openssl_algo_function_pointer(ctx);
        return emit_phar_sign_openssl_algo_bridge(ctx, inst);
    }
    publish_phar_sign_openssl_function_pointer(ctx);
    emit_phar_set_string_bridge(
        ctx,
        inst,
        "__elephc_phar_sign_openssl",
        "_elephc_phar_sign_openssl_fn",
    )
}

/// Lowers `__elephc_phar_sign_hash(path, algo)` into the hash-based signing bridge.
pub(crate) fn lower_elephc_phar_sign_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_sign_hash_function_pointer(ctx);
    emit_phar_path_int_to_bool_bridge(
        ctx,
        inst,
        "__elephc_phar_sign_hash",
        "_elephc_phar_sign_hash_fn",
    )
}

/// Lowers `__elephc_phar_set_zip_password(password)` into the ZipCrypto password
/// bridge that lets later reads decrypt encrypted ZIP entries.
pub(crate) fn lower_elephc_phar_set_zip_password(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_set_zip_password_function_pointer(ctx);
    emit_phar_string_to_bool_bridge(
        ctx,
        inst,
        "__elephc_phar_set_zip_password",
        "_elephc_phar_set_zip_password_fn",
    )
}

/// Lowers `__elephc_phar_get_signature_hash(path)` into the signature-hash read bridge.
pub(crate) fn lower_elephc_phar_get_signature_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_signature_hash_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_get_signature_hash",
        "_elephc_phar_get_signature_hash_fn",
    )
}

/// Lowers `__elephc_phar_get_signature_type(path)` into the signature-type read bridge.
pub(crate) fn lower_elephc_phar_get_signature_type(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_signature_type_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_get_signature_type",
        "_elephc_phar_get_signature_type_fn",
    )
}

/// Emits a `(path: string, value: int) -> bool` PHAR bridge call. Mirrors the
/// archive-compression bridge: the integer is stashed, the path string is loaded into
/// the path pointer/length registers, then the bridge pointer in `slot` is called and
/// its result normalized to a PHP bool (false when the bridge is unavailable).
pub(super) fn emit_phar_path_int_to_bool_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let fail = ctx.next_label("phar_path_int_fail");
    let done = ctx.next_label("phar_path_int_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_result(value)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, path, "phar path-int bridge path")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_symbol_address(ctx.emitter, "x9", slot);
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));              // missing bridge makes the op fail
            ctx.emitter.instruction("blr x9");                                  // invoke the bridge
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge success flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize to PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.load_value_to_result(value)?;
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, path, "phar path-int bridge path")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", slot, 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the bridge was published
            ctx.emitter.instruction(&format!("jz {}", fail));                   // missing bridge makes the op fail
            ctx.emitter.instruction("call r10");                                // invoke the bridge
            ctx.emitter.instruction("test rax, rax");                           // test the bridge success flag
            ctx.emitter.instruction("setne al");                                // normalize to PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the compiler-internal PHAR entry-list helper into a PHP string array.
pub(crate) fn lower_elephc_phar_list_entries(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_phar_list_entries", 1)?;
    let path = expect_operand(inst, 0)?;
    let empty = ctx.next_label("phar_list_entries_empty");
    let done = ctx.next_label("phar_list_entries_done");
    publish_phar_list_entries_function_pointer(ctx);
    load_string_to_result(ctx, path, "__elephc_phar_list_entries path")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_symbol_address(ctx.emitter, "x2", "_phar_list_len");
            abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_phar_list_entries_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR list bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", empty));             // missing bridge yields an empty entry list
            ctx.emitter.instruction("blr x9");                                  // serialize archive entry names into the bridge buffer
            ctx.emitter.instruction(&format!("cbz x0, {}", empty));             // unreadable archives yield an empty entry list
            emit_phar_list_entries_buffer_to_array_aarch64(ctx);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the empty-array fallback after successful expansion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_symbol_address(ctx.emitter, "rdx", "_phar_list_len");
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                "r10",
                "_elephc_phar_list_entries_fn",
                0,
            );
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR list bridge was published
            ctx.emitter.instruction(&format!("jz {}", empty));                  // missing bridge yields an empty entry list
            ctx.emitter.instruction("call r10");                                // serialize archive entry names into the bridge buffer
            ctx.emitter.instruction("test rax, rax");                           // test whether the bridge returned a serialized buffer
            ctx.emitter.instruction(&format!("jz {}", empty));                  // unreadable archives yield an empty entry list
            emit_phar_list_entries_buffer_to_array_x86_64(ctx);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the empty-array fallback after successful expansion
        }
    }
    ctx.emitter.label(&empty);
    emit_static_string_array(ctx, &[]);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Expands the serialized PHAR entry-name buffer in `x0` into a string array.
pub(super) fn emit_phar_list_entries_buffer_to_array_aarch64(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("phar_list_entries_loop");
    let done_label = ctx.next_label("phar_list_entries_expand_done");
    ctx.emitter.instruction("sub sp, sp, #32");                                 // reserve cursor, end, and array spill slots
    ctx.emitter.instruction("str x0, [sp, #0]");                                // seed the serialized-buffer cursor
    abi::emit_symbol_address(ctx.emitter, "x10", "_phar_list_len");
    ctx.emitter.instruction("ldr x11, [x10]");                                  // load the serialized entry-name byte length
    ctx.emitter.instruction("add x11, x0, x11");                                // compute the end pointer for the serialized buffer
    ctx.emitter.instruction("str x11, [sp, #8]");                               // save the end pointer across array helper calls
    ctx.emitter.instruction("mov x0, #1");                                      // allocate at least one slot for the entry-name array
    ctx.emitter.instruction("mov x1, #16");                                     // entry-name array stores 16-byte string slots
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    ctx.emitter.instruction("str x0, [sp, #16]");                               // save the growing entry-name array pointer
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("ldr x10, [sp, #0]");                               // reload the current serialized-buffer cursor
    ctx.emitter.instruction("ldr x11, [sp, #8]");                               // reload the serialized-buffer end pointer
    ctx.emitter.instruction("cmp x10, x11");                                    // has the cursor reached the serialized-buffer end?
    ctx.emitter.instruction(&format!("b.hs {}", done_label));                   // stop when no complete length header remains
    ctx.emitter.instruction("add x12, x10, #8");                                // compute the entry-name byte pointer after the length header
    ctx.emitter.instruction("cmp x12, x11");                                    // does the length header fit in the serialized buffer?
    ctx.emitter.instruction(&format!("b.hi {}", done_label));                   // stop on malformed trailing length bytes
    ctx.emitter.instruction("ldr x2, [x10]");                                   // load the next entry-name byte length
    ctx.emitter.instruction("add x13, x12, x2");                                // compute the cursor for the following serialized entry
    ctx.emitter.instruction("cmp x13, x11");                                    // does the entry-name payload fit in the serialized buffer?
    ctx.emitter.instruction(&format!("b.hi {}", done_label));                   // stop on malformed trailing entry bytes
    ctx.emitter.instruction("str x13, [sp, #0]");                               // advance the cursor before helper calls clobber scratch registers
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // pass the current string array to array_push_str
    ctx.emitter.instruction("mov x1, x12");                                     // pass the entry-name pointer to array_push_str
    abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
    ctx.emitter.instruction("str x0, [sp, #16]");                               // preserve the possibly-grown string array
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue expanding serialized entry names
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // restore the completed entry-name array as the result
    ctx.emitter.instruction("add sp, sp, #32");                                 // release serialized-buffer expansion spill slots
}

/// Expands the serialized PHAR entry-name buffer in `rax` into a string array.
pub(super) fn emit_phar_list_entries_buffer_to_array_x86_64(ctx: &mut FunctionContext<'_>) {
    let loop_label = ctx.next_label("phar_list_entries_loop");
    let done_label = ctx.next_label("phar_list_entries_expand_done");
    ctx.emitter.instruction("sub rsp, 48");                                     // reserve aligned cursor, end, and array spill slots
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // seed the serialized-buffer cursor
    abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_phar_list_len", 0);
    ctx.emitter.instruction("add r10, rax");                                    // compute the end pointer for the serialized buffer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r10");                    // save the end pointer across array helper calls
    ctx.emitter.instruction("mov edi, 1");                                      // allocate at least one slot for the entry-name array
    ctx.emitter.instruction("mov esi, 16");                                     // entry-name array stores 16-byte string slots
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // save the growing entry-name array pointer
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // reload the current serialized-buffer cursor
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 8]");                    // reload the serialized-buffer end pointer
    ctx.emitter.instruction("cmp r10, r11");                                    // has the cursor reached the serialized-buffer end?
    ctx.emitter.instruction(&format!("jae {}", done_label));                    // stop when no complete length header remains
    ctx.emitter.instruction("lea r8, [r10 + 8]");                               // compute the entry-name byte pointer after the length header
    ctx.emitter.instruction("cmp r8, r11");                                     // does the length header fit in the serialized buffer?
    ctx.emitter.instruction(&format!("ja {}", done_label));                     // stop on malformed trailing length bytes
    ctx.emitter.instruction("mov rdx, QWORD PTR [r10]");                        // load the next entry-name byte length
    ctx.emitter.instruction("lea rcx, [r8 + rdx]");                             // compute the cursor for the following serialized entry
    ctx.emitter.instruction("cmp rcx, r11");                                    // does the entry-name payload fit in the serialized buffer?
    ctx.emitter.instruction(&format!("ja {}", done_label));                     // stop on malformed trailing entry bytes
    ctx.emitter.instruction("mov QWORD PTR [rsp], rcx");                        // advance the cursor before helper calls clobber scratch registers
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                   // pass the current string array to array_push_str
    ctx.emitter.instruction("mov rsi, r8");                                     // pass the entry-name pointer to array_push_str
    abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");                   // preserve the possibly-grown string array
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue expanding serialized entry names
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // restore the completed entry-name array as the result
    ctx.emitter.instruction("add rsp, 48");                                     // release serialized-buffer expansion spill slots
}

