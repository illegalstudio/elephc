//! Purpose:
//! Lowers filesystem metadata builtins for the EIR backend.
//! Reuses the shared runtime stat helpers instead of duplicating platform logic.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Path operands are already evaluated by EIR and are materialized into the
//!   string result registers expected by the legacy runtime helpers.

use crate::codegen::{abi, callable_descriptor, emit_box_current_value_as_mixed, NULL_SENTINEL};
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const STREAM_METADATA_SLOT: usize = 14;
const STREAM_WRAPPER_UNLINK_SLOT: usize = 15;
const STREAM_WRAPPER_MKDIR_SLOT: usize = 17;
const STREAM_WRAPPER_RMDIR_SLOT: usize = 18;
const STREAM_META_TOUCH: usize = 1;
const STREAM_META_OWNER_NAME: usize = 2;
const STREAM_META_OWNER: usize = 3;
const STREAM_META_GROUP_NAME: usize = 4;
const STREAM_META_GROUP: usize = 5;
const STREAM_META_ACCESS: usize = 6;
const STREAM_OPTION_BLOCKING: usize = 1;
const STREAM_OPTION_READ_TIMEOUT: usize = 4;
const TOUCH_ATIME_NOW: u8 = 1;
const TOUCH_MTIME_NOW: u8 = 2;
const TOUCH_BOTH_NOW: u8 = TOUCH_ATIME_NOW | TOUCH_MTIME_NOW;

/// Lowers `file_get_contents(path)` and boxes the runtime string-or-false result.
pub(super) fn lower_file_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "file_get_contents", 1)?;
    let path = expect_operand(inst, 0)?;
    let path_literal = optional_const_string_operand(ctx, path)?;
    if let Some(path_literal) = path_literal.as_deref() {
        if path_literal.starts_with("phar://") {
            return lower_literal_phar_file_get_contents(ctx, inst, path_literal);
        }
        if path_literal == "php://input" {
            // file_get_contents('php://input'): under --web `__rt_php_input` copies
            // the captured request body into an owned string; in a non-web build it
            // returns a null pointer so the result boxes to PHP false.
            abi::emit_call_label(ctx.emitter, "__rt_php_input");
            box_owned_string_or_false_result(ctx, "fgc");
            return store_if_result(ctx, inst);
        }
    }
    if path_literal.is_none() {
        publish_dynamic_phar_function_pointers(ctx);
    }
    load_string_to_result(ctx, path, "file_get_contents filename")?;
    abi::emit_call_label(ctx.emitter, "__rt_file_get_contents_maybe_url");
    box_owned_string_or_false_result(ctx, "fgc");
    store_if_result(ctx, inst)
}

/// Publishes bridge/decompressor entry points into runtime slots used by
/// dynamic `phar://` reads.
fn publish_dynamic_phar_function_pointers(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_phar_extract_url", "_elephc_phar_extract_url_fn"),
        ("inflateInit2_", "_phar_zlib_inflate_init2_fn"),
        ("inflate", "_phar_zlib_inflate_fn"),
        ("inflateEnd", "_phar_zlib_inflate_end_fn"),
        ("BZ2_bzBuffToBuffDecompress", "_phar_bz2_decompress_fn"),
    ];
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = ctx.emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(ctx.emitter, "x9", &extern_sym);
                abi::emit_symbol_address(ctx.emitter, "x10", slot);
                ctx.emitter.instruction("str x9, [x10]");                       // publish the decompressor entry into its runtime slot
            }
        }
        Arch::X86_64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = ctx.emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(ctx.emitter, "r9", &extern_sym);
                abi::emit_store_reg_to_symbol(ctx.emitter, "r9", slot, 0);     // publish the decompressor entry into its runtime slot
            }
        }
    }
}

/// Publishes a list of elephc-phar bridge entry points into runtime slots.
fn publish_phar_bridge_entries(ctx: &mut FunctionContext<'_>, entries: &[(&str, &str)]) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            for (c_name, slot) in entries {
                let extern_sym = ctx.emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(ctx.emitter, "x9", &extern_sym);
                abi::emit_symbol_address(ctx.emitter, "x10", slot);
                ctx.emitter.instruction("str x9, [x10]");                       // publish the PHAR bridge entry into its runtime slot
            }
        }
        Arch::X86_64 => {
            for (c_name, slot) in entries {
                let extern_sym = ctx.emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(ctx.emitter, "r9", &extern_sym);
                abi::emit_store_reg_to_symbol(ctx.emitter, "r9", slot, 0);     // publish the PHAR bridge entry into its runtime slot
            }
        }
    }
}

/// Publishes the native PHAR read-modify-write bridge used by write finalization.
fn publish_phar_write_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_phar_put_entry", "_elephc_phar_put_entry_fn"),
        (
            "elephc_phar_stream_open_entry",
            "_elephc_phar_stream_open_entry_fn",
        ),
        ("elephc_phar_stream_append", "_elephc_phar_stream_append_fn"),
        (
            "elephc_phar_stream_finalize",
            "_elephc_phar_stream_finalize_fn",
        ),
    ];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the native PHAR writer bridge used by runtime-built phar:// URLs.
fn publish_dynamic_phar_write_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_phar_put_url", "_elephc_phar_put_url_fn"),
        (
            "elephc_phar_stream_open_url",
            "_elephc_phar_stream_open_url_fn",
        ),
        ("elephc_phar_stream_append", "_elephc_phar_stream_append_fn"),
        (
            "elephc_phar_stream_finalize",
            "_elephc_phar_stream_finalize_fn",
        ),
    ];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the native PHAR deletion bridge used by `unlink("phar://...")`.
fn publish_phar_delete_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_delete_url",
        "_elephc_phar_delete_url_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the native PHAR compression-control bridge.
fn publish_phar_set_compression_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_set_compression",
        "_elephc_phar_set_compression_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive-entry listing bridge used by PHAR OOP constructors.
fn publish_phar_list_entries_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_list_entries",
        "_elephc_phar_list_entries_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive global-metadata read bridge.
fn publish_phar_get_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_get_metadata", "_elephc_phar_get_metadata_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive global-metadata write bridge.
fn publish_phar_set_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_set_metadata", "_elephc_phar_set_metadata_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive stub read bridge.
fn publish_phar_get_stub_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[("elephc_phar_get_stub", "_elephc_phar_get_stub_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive stub write bridge.
fn publish_phar_set_stub_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[("elephc_phar_set_stub", "_elephc_phar_set_stub_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the per-file metadata read bridge.
fn publish_phar_get_file_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_get_file_metadata",
        "_elephc_phar_get_file_metadata_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the per-file metadata write bridge.
fn publish_phar_set_file_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_set_file_metadata",
        "_elephc_phar_set_file_metadata_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the whole-archive gzip compression bridge.
fn publish_phar_gzip_archive_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_gzip_archive", "_elephc_phar_gzip_archive_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the whole-archive bzip2 compression bridge.
fn publish_phar_bzip2_archive_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_bzip2_archive", "_elephc_phar_bzip2_archive_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the whole-archive decompression bridge.
fn publish_phar_decompress_archive_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_decompress_archive",
        "_elephc_phar_decompress_archive_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the OpenSSL (RSA-SHA1) signing bridge.
fn publish_phar_sign_openssl_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_sign_openssl", "_elephc_phar_sign_openssl_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the hash-based signing bridge.
fn publish_phar_sign_hash_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[("elephc_phar_sign_hash", "_elephc_phar_sign_hash_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the ZipCrypto password bridge used to read encrypted ZIP entries.
fn publish_phar_set_zip_password_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_set_zip_password",
        "_elephc_phar_set_zip_password_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the signature-hash read bridge.
fn publish_phar_get_signature_hash_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_get_signature_hash",
        "_elephc_phar_get_signature_hash_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the signature-type read bridge.
fn publish_phar_get_signature_type_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_get_signature_type",
        "_elephc_phar_get_signature_type_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Lowers `hash_file(algo, filename, binary?)` by reading bytes then hashing them.
pub(super) fn lower_hash_file(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "hash_file", 2, 3)?;
    let fail = ctx.next_label("hash_file_fail");
    let done = ctx.next_label("hash_file_box");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_hash_file_aarch64(ctx, inst, &fail, &done)?,
        Arch::X86_64 => lower_hash_file_x86_64(ctx, inst, &fail, &done)?,
    }
    box_owned_string_or_false_result(ctx, "hash_file");
    store_if_result(ctx, inst)
}

/// Lowers `readfile(path)` and boxes the runtime byte-count-or-false result.
pub(super) fn lower_readfile(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "readfile", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "readfile")?;
    emit_readfile_wrapper_dispatch(ctx);
    box_readfile_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `readline(prompt?)` by optionally writing a prompt and reading stdin.
pub(super) fn lower_readline(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "readline", 0, 1)?;
    if inst.operands.len() == 1 {
        let prompt = expect_operand(inst, 0)?;
        load_string_to_result(ctx, prompt, "readline prompt")?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #1");                          // pass stdout as the destination fd for the readline prompt
                ctx.emitter.syscall(4);                                         // write the prompt before blocking on stdin
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rsi, rax");                        // pass the prompt pointer as write()'s buffer argument
                ctx.emitter.instruction("mov rdi, 1");                          // pass stdout as the destination fd for the readline prompt
                ctx.emitter.instruction("call write");                          // write the prompt before blocking on stdin
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                              // pass stdin fd 0 to the shared line-reader helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor edi, edi");                            // pass stdin fd 0 to the shared line-reader helper
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgets");
    store_if_result(ctx, inst)
}

/// Lowers `fopen(filename, mode)` and boxes stream resources or PHP false.
pub(super) fn lower_fopen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fopen", 2, 4)?;
    let filename = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    let filename_literal = optional_const_string_operand(ctx, filename)?;
    if let Some(path) = filename_literal.as_deref() {
        if path.starts_with("php://filter/") {
            return lower_literal_php_filter_fopen(ctx, inst, path);
        }
        if let Some(fd) = php_standard_stream_fd(path).or_else(|| php_fd_stream(path)) {
            emit_fd_result(ctx, fd);
            box_stream_fd_or_false_result(ctx, "fopen");
            return store_if_result(ctx, inst);
        }
        if is_php_memory_stream(path) {
            abi::emit_call_label(ctx.emitter, "__rt_tmpfile");
            box_stream_fd_or_false_result(ctx, "fopen");
            return store_if_result(ctx, inst);
        }
        if path.starts_with("data://") {
            return lower_literal_data_fopen(ctx, inst, path);
        }
        if path.starts_with("ftp://") {
            return lower_literal_ftp_fopen(ctx, inst, path);
        }
        if path.starts_with("phar://") {
            if literal_fopen_mode_is_write(ctx, mode)? {
                return lower_literal_phar_fopen_write(ctx, inst, path);
            }
            return lower_literal_phar_fopen_read(ctx, inst, path);
        }
        if path.starts_with("http://") {
            return lower_literal_http_fopen(ctx, inst, path);
        }
        if path.starts_with("compress.zlib://") {
            return lower_literal_compress_zlib_fopen(ctx, inst, path);
        }
        if path.starts_with("compress.bzip2://") {
            return lower_literal_compress_bzip2_fopen(ctx, inst, path);
        }
    }
    if filename_literal.is_none() {
        publish_dynamic_phar_function_pointers(ctx);
        publish_dynamic_phar_write_function_pointer(ctx);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, filename, "fopen filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the mode pointer in the runtime helper's secondary string slot
            ctx.emitter.instruction("mov x4, x2");                              // pass the mode length in the runtime helper's secondary string slot
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, filename, "fopen filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the mode pointer while the filename remains on the stack
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the mode length while the filename remains on the stack
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
    box_stream_fd_or_false_result(ctx, "fopen");
    store_if_result(ctx, inst)
}

/// Emits the boxed `fopen()` result for a compile-time literal path without storing it.
fn emit_literal_fopen_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    let mode = expect_operand(inst, 1)?;
    if let Some(fd) = php_standard_stream_fd(path).or_else(|| php_fd_stream(path)) {
        emit_fd_result(ctx, fd);
        box_stream_fd_or_false_result(ctx, "fopen");
        return Ok(());
    }
    if is_php_memory_stream(path) {
        abi::emit_call_label(ctx.emitter, "__rt_tmpfile");
        box_stream_fd_or_false_result(ctx, "fopen");
        return Ok(());
    }
    if path.starts_with("data://") {
        return emit_literal_data_fopen_result(ctx, path);
    }
    if path.starts_with("ftp://") {
        return emit_literal_ftp_fopen_result(ctx, path);
    }
    if path.starts_with("phar://") {
        if literal_fopen_mode_is_write(ctx, mode)? {
            return emit_literal_phar_fopen_write_result(ctx, path);
        }
        return emit_literal_phar_fopen_read_result(ctx, path);
    }
    if path.starts_with("http://") {
        return emit_literal_http_fopen_result(ctx, path);
    }
    emit_runtime_fopen_literal_result(ctx, path, mode)
}

/// Emits a runtime `fopen()` call for a literal path and the caller's mode operand.
fn emit_runtime_fopen_literal_result(
    ctx: &mut FunctionContext<'_>,
    path: &str,
    mode: ValueId,
) -> Result<()> {
    let (path_label, path_len) = ctx.data.add_string(path.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &path_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", path_len));         // pass the literal fopen path byte length
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the fopen mode pointer with the literal path
            ctx.emitter.instruction("mov x4, x2");                              // pass the fopen mode length with the literal path
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &path_label);
            ctx.emitter.instruction(&format!("mov rdx, {}", path_len));         // pass the literal fopen path byte length
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, mode, "fopen mode")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fopen mode pointer with the literal path
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the fopen mode length with the literal path
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
    box_stream_fd_or_false_result(ctx, "fopen");
    Ok(())
}

/// Lowers a literal `fopen("php://filter/...", ...)` by opening and filtering `resource=`.
fn lower_literal_php_filter_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    let Some((mode_bits, filter_id, resource)) = parse_php_filter_url(path) else {
        emit_fd_result(ctx, -1);
        box_stream_fd_or_false_result(ctx, "fopen_php_filter");
        return store_if_result(ctx, inst);
    };
    emit_literal_fopen_result(ctx, inst, &resource)?;
    if mode_bits != 0 {
        emit_php_filter_table_stamps(ctx, mode_bits, filter_id);
    }
    store_if_result(ctx, inst)
}

/// Parses `php://filter/[read=|write=]filter/resource=path` for literal `fopen`.
fn parse_php_filter_url(path: &str) -> Option<(u8, u8, String)> {
    let spec = path.strip_prefix("php://filter/")?;
    let (filter_part, resource) = spec.split_once("/resource=")?;
    if resource.is_empty() || resource.starts_with("php://filter") {
        return None;
    }
    let (mode_bits, filters) = if let Some(filters) = filter_part.strip_prefix("read=") {
        (1u8, filters)
    } else if let Some(filters) = filter_part.strip_prefix("write=") {
        (2u8, filters)
    } else {
        (3u8, filter_part)
    };
    let first_filter = filters.split('|').next().unwrap_or("");
    let filter_id = stream_filter_id(first_filter).unwrap_or(0);
    let mode_bits = if filter_id == 0 { 0 } else { mode_bits };
    Some((mode_bits, filter_id, resource.to_string()))
}

/// Records `php://filter` read/write filter ids on a successfully opened resource.
fn emit_php_filter_table_stamps(ctx: &mut FunctionContext<'_>, mode_bits: u8, filter_id: u8) {
    let done_label = ctx.next_label("php_filter_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the boxed fopen result tag
            ctx.emitter.instruction("cmp x9, #9");                              // test whether fopen returned a resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // leave false results unmodified
            ctx.emitter.instruction("ldr x1, [x0, #8]");                        // load the descriptor payload from the boxed resource
            if mode_bits & 1 != 0 {
                abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
                ctx.emitter.instruction(&format!("mov w10, #{}", filter_id));   // materialize the php://filter read filter id
                ctx.emitter.instruction("strb w10, [x9, x1]");                  // attach the read filter to this descriptor
            }
            if mode_bits & 2 != 0 {
                abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
                ctx.emitter.instruction(&format!("mov w10, #{}", filter_id));   // materialize the php://filter write filter id
                ctx.emitter.instruction("strb w10, [x9, x1]");                  // attach the write filter to this descriptor
            }
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // load the boxed fopen result tag
            ctx.emitter.instruction("cmp r9, 9");                               // test whether fopen returned a resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // leave false results unmodified
            ctx.emitter.instruction("mov rcx, QWORD PTR [rax + 8]");            // load the descriptor payload from the boxed resource
            if mode_bits & 1 != 0 {
                abi::emit_symbol_address(ctx.emitter, "r8", "_stream_read_filters"); // read-filter table base
                ctx.emitter.instruction(&format!("mov BYTE PTR [r8 + rcx], {}", filter_id)); // attach the read filter to this descriptor
            }
            if mode_bits & 2 != 0 {
                abi::emit_symbol_address(ctx.emitter, "r8", "_stream_write_filters"); // write-filter table base
                ctx.emitter.instruction(&format!("mov BYTE PTR [r8 + rcx], {}", filter_id)); // attach the write filter to this descriptor
            }
            ctx.emitter.label(&done_label);
        }
    }
}

/// Lowers a literal `fopen("data://...", ...)` through an in-memory data stream.
fn lower_literal_data_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_data_fopen_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed result for a literal `data://` stream open.
fn emit_literal_data_fopen_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    match decode_data_uri_for_fopen(path) {
        Some(bytes) => {
            let (symbol, len) = ctx.data.add_string(&bytes);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x0", &symbol);
                    ctx.emitter.instruction(&format!("mov x1, #{}", len));      // pass the decoded data:// payload byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rdi", &symbol);
                    ctx.emitter.instruction(&format!("mov rsi, {}", len));      // pass the decoded data:// payload byte length
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_data_stream");
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unparseable data:// URI lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unparseable data:// URI lowers to PHP false
            }
        },
    }
    box_stream_fd_or_false_result(ctx, "fopen_data");
    Ok(())
}

/// Decodes a literal `data://[mediatype][;base64],payload` URL for EIR `fopen`.
fn decode_data_uri_for_fopen(path: &str) -> Option<Vec<u8>> {
    let rest = path.strip_prefix("data://")?;
    let comma = rest.find(',')?;
    let meta = &rest[..comma];
    let payload = &rest[comma + 1..];
    if meta.to_ascii_lowercase().ends_with(";base64") {
        base64_decode_for_data_uri(payload)
    } else {
        Some(percent_decode_for_data_uri(payload))
    }
}

/// Decodes a base64 payload for a compile-time `data://` stream.
fn base64_decode_for_data_uri(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | base64_sextet_for_data_uri(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Converts one base64 byte into its six-bit value for `data://` decoding.
fn base64_sextet_for_data_uri(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some((c - b'A') as u32),
        b'a'..=b'z' => Some((c - b'a') as u32 + 26),
        b'0'..=b'9' => Some((c - b'0') as u32 + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Percent-decodes a `data://` payload for compile-time stream materialization.
fn percent_decode_for_data_uri(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi * 16 + lo) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

/// Lowers a literal `fopen("ftp://...", ...)` through the FTP runtime wrapper.
fn lower_literal_ftp_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_ftp_fopen_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed result for a literal `ftp://` stream open.
fn emit_literal_ftp_fopen_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    match parse_ftp_url_for_fopen(path) {
        Some((ctrl_addr, retr_cmd)) => {
            let (ctrl_sym, ctrl_len) = ctx.data.add_string(ctrl_addr.as_bytes());
            let (retr_sym, retr_len) = ctx.data.add_string(retr_cmd.as_bytes());
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x0", &ctrl_sym);
                    ctx.emitter.instruction(&format!("mov x1, #{}", ctrl_len)); // pass the FTP control address byte length
                    abi::emit_symbol_address(ctx.emitter, "x2", &retr_sym);
                    ctx.emitter.instruction(&format!("mov x3, #{}", retr_len)); // pass the FTP RETR command byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rdi", &ctrl_sym);
                    ctx.emitter.instruction(&format!("mov rsi, {}", ctrl_len)); // pass the FTP control address byte length
                    abi::emit_symbol_address(ctx.emitter, "rdx", &retr_sym);
                    ctx.emitter.instruction(&format!("mov rcx, {}", retr_len)); // pass the FTP RETR command byte length
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_ftp_open");
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unparseable ftp:// URL lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unparseable ftp:// URL lowers to PHP false
            }
        },
    }
    box_stream_fd_or_false_result(ctx, "fopen_ftp");
    Ok(())
}

/// Parses `ftp://[user[:pass]@]host[:port]/path` into runtime FTP open inputs.
fn parse_ftp_url_for_fopen(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("ftp://")?;
    let after_userinfo = match rest.find('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    let slash = after_userinfo.find('/')?;
    let authority = &after_userinfo[..slash];
    let path = &after_userinfo[slash..];
    if authority.is_empty() || path.len() < 2 {
        return None;
    }
    let (host, port) = match authority.rfind(':') {
        Some(colon) => (&authority[..colon], &authority[colon + 1..]),
        None => (authority, "21"),
    };
    if host.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((
        format!("tcp://{}:{}", host, port),
        format!("RETR {}\r\n", path),
    ))
}

/// Lowers a literal `fopen("http://...", ...)` through the HTTP runtime wrapper.
fn lower_literal_http_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_http_fopen_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed result for a literal `http://` stream open.
fn emit_literal_http_fopen_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    match parse_http_url_for_fopen(path) {
        Some(parsed) => {
            let (addr_sym, addr_len) = ctx.data.add_string(parsed.addr.as_bytes());
            let (host_sym, host_len) = ctx.data.add_string(parsed.host.as_bytes());
            let (path_sym, path_len) = ctx.data.add_string(parsed.path.as_bytes());
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x0", &host_sym);
                    ctx.emitter.instruction(&format!("mov x1, #{}", host_len)); // pass the HTTP Host header byte length
                    abi::emit_symbol_address(ctx.emitter, "x2", &path_sym);
                    ctx.emitter.instruction(&format!("mov x3, #{}", path_len)); // pass the request path byte length
                    abi::emit_call_label(ctx.emitter, "__rt_http_build_request");
                    abi::emit_push_reg(ctx.emitter, "x0");
                    abi::emit_symbol_address(ctx.emitter, "x0", &addr_sym);
                    ctx.emitter.instruction(&format!("mov x1, #{}", addr_len)); // pass the TCP address byte length
                    abi::emit_symbol_address(ctx.emitter, "x2", "_http_req_scratch");
                    abi::emit_pop_reg(ctx.emitter, "x3");
                    abi::emit_call_label(ctx.emitter, "__rt_http_open");
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rdi", &host_sym);
                    ctx.emitter.instruction(&format!("mov rsi, {}", host_len)); // pass the HTTP Host header byte length
                    abi::emit_symbol_address(ctx.emitter, "rdx", &path_sym);
                    ctx.emitter.instruction(&format!("mov rcx, {}", path_len)); // pass the request path byte length
                    abi::emit_call_label(ctx.emitter, "__rt_http_build_request");
                    abi::emit_push_reg(ctx.emitter, "rax");
                    abi::emit_symbol_address(ctx.emitter, "rdi", &addr_sym);
                    ctx.emitter.instruction(&format!("mov rsi, {}", addr_len)); // pass the TCP address byte length
                    abi::emit_symbol_address(ctx.emitter, "rdx", "_http_req_scratch");
                    abi::emit_pop_reg(ctx.emitter, "rcx");
                    abi::emit_call_label(ctx.emitter, "__rt_http_open");
                }
            }
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unparseable http:// URL lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unparseable http:// URL lowers to PHP false
            }
        },
    }
    box_stream_fd_or_false_result(ctx, "fopen_http");
    Ok(())
}

/// Parsed pieces needed by the HTTP runtime open helper.
struct ParsedHttpFopenUrl {
    addr: String,
    host: String,
    path: String,
}

/// Parses a literal `http://[user@]host[:port]/path` URL for EIR `fopen`.
fn parse_http_url_for_fopen(url: &str) -> Option<ParsedHttpFopenUrl> {
    let rest = url.strip_prefix("http://")?;
    let after_userinfo = match rest.find('@') {
        Some(at) => &rest[at + 1..],
        None => rest,
    };
    let (authority, path) = match after_userinfo.find('/') {
        Some(slash) => (&after_userinfo[..slash], &after_userinfo[slash..]),
        None => (after_userinfo, "/"),
    };
    if authority.is_empty() {
        return None;
    }
    let (host, port) = match authority.rfind(':') {
        Some(colon) => (&authority[..colon], &authority[colon + 1..]),
        None => (authority, "80"),
    };
    if host.is_empty() || port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(ParsedHttpFopenUrl {
        addr: format!("tcp://{}:{}", host, port),
        host: if port == "80" {
            host.to_string()
        } else {
            format!("{}:{}", host, port)
        },
        path: path.to_string(),
    })
}

/// Lowers a literal `file_get_contents("phar://...")` through compile-time PHAR extraction.
fn lower_literal_phar_file_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    match crate::codegen::builtins::phar_stream::extract_phar_entry(path) {
        Some(payload) => {
            let (symbol, len) = ctx.data.add_string(&payload);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x1", &symbol);
                    ctx.emitter.instruction(&format!("mov x2, #{}", len));      // embedded phar entry byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rax", &symbol);
                    ctx.emitter.instruction(&format!("mov rdx, {}", len));      // embedded phar entry byte length
                }
            }
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x1, #0");                          // null string pointer asks the boxer for PHP false
                ctx.emitter.instruction("mov x2, #0");                          // clear the unused failure length
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor eax, eax");                        // null string pointer asks the boxer for PHP false
                ctx.emitter.instruction("xor edx, edx");                        // clear the unused failure length
            }
        },
    }
    box_owned_string_or_false_result(ctx, "fgc_phar");
    store_if_result(ctx, inst)
}

/// Lowers a literal read-mode `fopen("phar://...", ...)` through embedded entry bytes.
fn lower_literal_phar_fopen_read(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_phar_fopen_read_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed result for a literal read-mode `phar://` stream open.
fn emit_literal_phar_fopen_read_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    match crate::codegen::builtins::phar_stream::extract_phar_entry(path) {
        Some(payload) => {
            let (symbol, len) = ctx.data.add_string(&payload);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x0", &symbol);
                    ctx.emitter.instruction(&format!("mov x1, #{}", len));      // embedded phar entry byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rdi", &symbol);
                    ctx.emitter.instruction(&format!("mov rsi, {}", len));      // embedded phar entry byte length
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_data_stream");
        }
        None => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unresolved phar entry lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unresolved phar entry lowers to PHP false
            }
        },
    }
    box_stream_fd_or_false_result(ctx, "fopen_phar");
    Ok(())
}

/// Lowers a literal write-mode `fopen("phar://...", ...)` through the PHAR writer.
fn lower_literal_phar_fopen_write(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_phar_fopen_write_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed stream result for a literal write-mode `phar://` stream open.
fn emit_literal_phar_fopen_write_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    if !emit_phar_write_open_for_literal(ctx, path)? {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unresolved phar write target lowers to PHP false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unresolved phar write target lowers to PHP false
            }
        }
    }
    box_stream_fd_or_false_result(ctx, "fopen_phar_write");
    Ok(())
}

/// Seeds the PHAR write buffer for a literal target and records the output archive path.
fn emit_phar_write_open_for_literal(ctx: &mut FunctionContext<'_>, url: &str) -> Result<bool> {
    let Some((archive, entry)) = crate::codegen::builtins::phar_stream::resolve_write_target(url)
    else {
        return Ok(false);
    };
    let template = crate::codegen::builtins::phar_stream::build_phar_write_template(&entry);
    let (template_label, template_len) = ctx.data.add_string(&template);
    let (path_label, path_len) = ctx.data.add_string(archive.as_bytes());
    let (entry_label, entry_len) = ctx.data.add_string(entry.as_bytes());
    publish_phar_write_function_pointer(ctx);
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", &path_label);
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_path_ptr");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive path pointer for finalize
            ctx.emitter.instruction(&format!("mov x9, #{}", path_len));         // materialize the archive path byte length
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_path_len");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive path length for finalize
            abi::emit_symbol_address(ctx.emitter, "x9", &entry_label);
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_entry_ptr");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive entry name pointer for finalize
            ctx.emitter.instruction(&format!("mov x9, #{}", entry_len));        // materialize the archive entry name byte length
            abi::emit_symbol_address(ctx.emitter, "x10", "_phar_write_entry_len");
            ctx.emitter.instruction("str x9, [x10]");                           // record the archive entry name length for finalize
            abi::emit_symbol_address(ctx.emitter, "x0", &template_label);
            ctx.emitter.instruction(&format!("mov x1, #{}", template_len));     // pass the single-entry PHAR template length
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_open");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", &path_label);
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_path_ptr");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // record the archive path pointer for finalize
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_path_len");
            ctx.emitter.instruction(&format!("mov QWORD PTR [r10], {}", path_len)); // record the archive path length for finalize
            abi::emit_symbol_address(ctx.emitter, "r9", &entry_label);
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_entry_ptr");
            ctx.emitter.instruction("mov QWORD PTR [r10], r9");                 // record the archive entry name pointer for finalize
            abi::emit_symbol_address(ctx.emitter, "r10", "_phar_write_entry_len");
            ctx.emitter.instruction(&format!("mov QWORD PTR [r10], {}", entry_len)); // record the archive entry name length for finalize
            abi::emit_symbol_address(ctx.emitter, "rdi", &template_label);
            ctx.emitter.instruction(&format!("mov rsi, {}", template_len));     // pass the single-entry PHAR template length
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_open");
        }
    }
    Ok(true)
}

/// Returns true when a literal fopen mode opens a PHAR entry for writing.
fn literal_fopen_mode_is_write(ctx: &FunctionContext<'_>, mode: ValueId) -> Result<bool> {
    Ok(optional_const_string_operand(ctx, mode)?
        .and_then(|mode| mode.as_bytes().first().copied())
        .is_some_and(|first| matches!(first, b'w' | b'a' | b'c' | b'x')))
}

/// Lowers `stream_wrapper_register(protocol, class, flags?)`.
pub(super) fn lower_stream_wrapper_register(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_wrapper_register", 2, 3)?;
    let protocol = expect_operand(inst, 0)?;
    let class = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, protocol, "stream_wrapper_register protocol")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, class, "stream_wrapper_register class")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, protocol, "stream_wrapper_register protocol")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, class, "stream_wrapper_register class")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_register");
    store_if_result(ctx, inst)
}

/// Lowers `stream_wrapper_unregister(protocol)`.
pub(super) fn lower_stream_wrapper_unregister(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_wrapper_unregister", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "stream_wrapper_unregister protocol")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the protocol byte length as the second runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the protocol byte length as the second runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_wrapper_unregister");
    store_if_result(ctx, inst)
}

/// Lowers `stream_wrapper_restore(protocol)` as a successful no-op.
pub(super) fn lower_stream_wrapper_restore(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_wrapper_restore", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "stream_wrapper_restore protocol")?;
    emit_bool_result(ctx, true);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_create(options?, params?)`.
pub(super) fn lower_stream_context_create(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_create", 0, 2)?;
    if let Some(options) = inst.operands.first().copied() {
        store_stream_context_options(ctx, options, true)?;
    }
    capture_stream_notification_callback(ctx, inst.operands.get(1).copied())?;
    emit_fd_result(ctx, 1);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_get_default(options?)`.
pub(super) fn lower_stream_context_get_default(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_get_default", 0, 1)?;
    emit_fd_result(ctx, 0);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_set_default(options)`.
pub(super) fn lower_stream_context_set_default(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_context_set_default", 1)?;
    emit_fd_result(ctx, 0);
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_set_option(context, options)` and the four-argument form.
pub(super) fn lower_stream_context_set_option(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_context_set_option", 2, 4)?;
    match inst.operands.len() {
        2 => {
            let options = expect_operand(inst, 1)?;
            store_stream_context_options(ctx, options, false)?;
            emit_bool_result(ctx, true);
        }
        4 => {
            lower_stream_context_set_option_4(ctx, inst)?;
        }
        _ => emit_bool_result(ctx, true),
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_set_params(context, params)` as an accepted parameter update.
pub(super) fn lower_stream_context_set_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_context_set_params", 2)?;
    capture_stream_notification_callback(ctx, inst.operands.get(1).copied())?;
    emit_bool_result(ctx, true);
    store_if_result(ctx, inst)
}

/// Captures a literal `notification` callable from stream context params into runtime global state.
fn capture_stream_notification_callback(
    ctx: &mut FunctionContext<'_>,
    params: Option<ValueId>,
) -> Result<()> {
    let Some(params) = params else {
        return Ok(());
    };
    let Some(callback) = notification_callback_value(ctx, params)? else {
        clear_stream_notification_callback(ctx);
        return Ok(());
    };
    if !is_capturable_notification_callable(ctx, callback)? {
        clear_stream_notification_callback(ctx);
        return Ok(());
    }
    ctx.load_value_to_result(callback)?;
    callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
    store_current_result_as_stream_notification_callback(ctx);
    Ok(())
}

/// Returns the last literal `notification` value inserted into a static params hash.
fn notification_callback_value(
    ctx: &FunctionContext<'_>,
    params: ValueId,
) -> Result<Option<ValueId>> {
    if !value_is_static_hash_new(ctx, params)? {
        return Ok(None);
    }
    let mut found = None;
    for instruction in &ctx.function.instructions {
        if instruction.op != Op::HashSet || instruction.operands.len() != 3 {
            continue;
        }
        if instruction.operands[0] != params {
            continue;
        }
        if value_is_string_literal(ctx, instruction.operands[1], "notification")? {
            found = Some(instruction.operands[2]);
        }
    }
    Ok(found)
}

/// Returns true when `value` is produced by a literal hash allocation in this function.
fn value_is_static_hash_new(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(inst.op == Op::HashNew)
}

/// Returns true when `value` is a constant string equal to `expected`.
fn value_is_string_literal(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    expected: &str,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst.op != Op::ConstStr {
        return Ok(false);
    }
    let Some(Immediate::Data(data_id)) = inst.immediate else {
        return Ok(false);
    };
    let Some(value) = ctx.module.data.strings.get(data_id.as_raw() as usize) else {
        return Err(CodegenIrError::missing_entry(
            "data string",
            data_id.as_raw(),
        ));
    };
    Ok(value == expected)
}

/// Returns true for literal callables that expose the descriptor invoker slot.
fn is_capturable_notification_callable(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let Some(inst) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    Ok(matches!(inst.op, Op::ClosureNew | Op::FirstClassCallableNew))
}

/// Stores the loaded callable descriptor into `_stream_notification_callback`.
fn store_current_result_as_stream_notification_callback(ctx: &mut FunctionContext<'_>) {
    let addr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, addr_reg, "_stream_notification_callback");
    abi::emit_store_to_address(ctx.emitter, result_reg, addr_reg, 0);
}

/// Clears `_stream_notification_callback` so later transfers do not fire stale callbacks.
fn clear_stream_notification_callback(ctx: &mut FunctionContext<'_>) {
    let addr_reg = abi::symbol_scratch_reg(ctx.emitter);
    let zero_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, addr_reg, "_stream_notification_callback");
    abi::emit_load_int_immediate(ctx.emitter, zero_reg, 0);
    abi::emit_store_to_address(ctx.emitter, zero_reg, addr_reg, 0);
}

/// Lowers `stream_context_get_options(context)`.
pub(super) fn lower_stream_context_get_options(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_context_get_options", 1)?;
    let empty_label = ctx.next_label("scgo_empty");
    let done_label = ctx.next_label("scgo_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the persisted stream-context options pointer
            ctx.emitter.instruction(&format!("cbz x0, {}", empty_label));       // allocate an empty hash when no context options exist
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-hash fallback after retaining options
            ctx.emitter.label(&empty_label);
            ctx.emitter.instruction("mov x0, #1");                              // pass the empty fallback hash capacity
            ctx.emitter.instruction("mov x1, #7");                              // select Mixed values for the fallback hash
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the persisted stream-context options pointer
            ctx.emitter.instruction("test rax, rax");                           // test whether a context options pointer exists
            ctx.emitter.instruction(&format!("jz {}", empty_label));            // allocate an empty hash when no context options exist
            ctx.emitter.instruction("mov rdi, rax");                            // pass the options pointer to incref
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-hash fallback after retaining options
            ctx.emitter.label(&empty_label);
            ctx.emitter.instruction("mov edi, 1");                              // pass the empty fallback hash capacity
            ctx.emitter.instruction("mov esi, 7");                              // select Mixed values for the fallback hash
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_context_get_params(context)` to an empty associative hash.
pub(super) fn lower_stream_context_get_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_context_get_params", 1)?;
    emit_empty_mixed_hash(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_contents(stream, length?, offset?)` to `string|false`.
pub(super) fn lower_stream_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_get_contents", 1, 3)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_get_contents")?;
    if inst.operands.len() == 1 {
        lower_stream_get_contents_read_all(ctx);
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        return store_if_result(ctx, inst);
    }

    let read_all = ctx.next_label("sgc_read_all");
    let skip_seek = ctx.next_label("sgc_skip_seek");
    let wrap_seek = ctx.next_label("sgc_wrap_seek");
    let seek_failed = ctx.next_label("sgc_seek_failed");
    let done = ctx.next_label("sgc_done");

    emit_stream_get_contents_frame_enter(ctx);
    emit_stream_get_contents_save_fd(ctx);
    let length = expect_operand(inst, 1)?;
    require_optional_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_get_contents length",
    )?;
    emit_stream_get_contents_save_length(ctx);

    if inst.operands.len() == 3 {
        let offset = expect_operand(inst, 2)?;
        require_int(
            ctx.load_value_to_result(offset)?.codegen_repr(),
            "stream_get_contents offset",
        )?;
        lower_stream_get_contents_seek(ctx, &skip_seek, &wrap_seek, &seek_failed);
    }

    lower_stream_get_contents_bounded_or_all(ctx, &read_all, &done);
    ctx.emitter.label(&read_all);
    lower_stream_get_contents_reload_fd_and_leave_frame(ctx);
    lower_stream_get_contents_read_all(ctx);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the seek-failure false result after reading successfully
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the seek-failure false result after reading successfully
        }
    }
    ctx.emitter.label(&seek_failed);
    emit_stream_get_contents_frame_leave(ctx);
    emit_bool_result(ctx, false);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `stream_copy_to_stream(from, to, length?, offset?)` through wrapper-aware read/write loops.
pub(super) fn lower_stream_copy_to_stream(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_copy_to_stream", 2, 4)?;
    let source = expect_operand(inst, 0)?;
    let dest = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, source, "stream_copy_to_stream")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_stream_fd_to_result(ctx, dest, "stream_copy_to_stream")?;
    emit_stream_copy_frame_enter(ctx);
    materialize_stream_copy_length(ctx, inst)?;
    if inst.operands.len() >= 4 {
        let offset = expect_operand(inst, 3)?;
        require_int(
            ctx.load_value_to_result(offset)?.codegen_repr(),
            "stream_copy_to_stream offset",
        )?;
        let skip_seek = ctx.next_label("scs_skip_seek");
        let wrap_seek = ctx.next_label("scs_wrap_seek");
        let seek_failed = ctx.next_label("scs_seek_failed");
        let boxed_done = ctx.next_label("scs_boxed_done");
        lower_stream_copy_seek(ctx, &skip_seek, &wrap_seek, &seek_failed);
        lower_stream_copy_loop_and_box(ctx, &seek_failed, &boxed_done);
    } else {
        let seek_failed = ctx.next_label("scs_seek_unreachable");
        let boxed_done = ctx.next_label("scs_boxed_done");
        lower_stream_copy_loop_and_box(ctx, &seek_failed, &boxed_done);
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_line(stream, length, ending?)`.
pub(super) fn lower_stream_get_line(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_get_line", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_get_line")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_get_line length",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if inst.operands.len() == 3 {
        let ending = expect_operand(inst, 2)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                load_string_to_result(ctx, ending, "stream_get_line ending")?;
                ctx.emitter.instruction("mov x3, x2");                          // pass the ending-delimiter byte length to the runtime helper
                ctx.emitter.instruction("mov x2, x1");                          // pass the ending-delimiter pointer to the runtime helper
                abi::emit_pop_reg(ctx.emitter, "x1");
                abi::emit_pop_reg(ctx.emitter, "x0");
            }
            Arch::X86_64 => {
                load_string_to_result(ctx, ending, "stream_get_line ending")?;
                ctx.emitter.instruction("mov rcx, rdx");                        // pass the ending-delimiter byte length to the runtime helper
                ctx.emitter.instruction("mov rdx, rax");                        // pass the ending-delimiter pointer to the runtime helper
                abi::emit_pop_reg(ctx.emitter, "rsi");
                abi::emit_pop_reg(ctx.emitter, "rdi");
            }
        }
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x2, #0");                          // signal that no ending delimiter was supplied
                ctx.emitter.instruction("mov x3, #0");                          // signal a zero-length ending delimiter
                abi::emit_pop_reg(ctx.emitter, "x1");
                abi::emit_pop_reg(ctx.emitter, "x0");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor edx, edx");                        // signal that no ending delimiter was supplied
                ctx.emitter.instruction("xor ecx, ecx");                        // signal a zero-length ending delimiter
                abi::emit_pop_reg(ctx.emitter, "rsi");
                abi::emit_pop_reg(ctx.emitter, "rdi");
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_line");
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_meta_data(stream)` through the metadata runtime helper.
pub(super) fn lower_stream_get_meta_data(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_get_meta_data", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_get_meta_data")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the stream metadata helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_meta_data");
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_wrappers()` to the static built-in wrapper list.
pub(super) fn lower_stream_get_wrappers(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_get_wrappers", 0)?;
    emit_static_string_array(
        ctx,
        &[
            "file", "php", "data", "ftp", "http", "https", "ftps",
            "compress.zlib", "compress.bzip2", "phar", "glob",
        ],
    );
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_transports()` to the static transport list.
pub(super) fn lower_stream_get_transports(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_get_transports", 0)?;
    emit_static_string_array(
        ctx,
        &[
            "tcp", "udp", "unix", "udg", "tls", "ssl", "sslv2", "sslv3",
            "tlsv1.0", "tlsv1.1", "tlsv1.2", "tlsv1.3",
        ],
    );
    store_if_result(ctx, inst)
}

/// Lowers `stream_get_filters()` to the static built-in filter list.
pub(super) fn lower_stream_get_filters(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_get_filters", 0)?;
    emit_static_string_array(
        ctx,
        &[
            "string.toupper",
            "string.tolower",
            "string.rot13",
            "string.strip_tags",
            "convert.base64-encode",
            "convert.base64-decode",
            "convert.quoted-printable-encode",
            "convert.quoted-printable-decode",
            "convert.iconv.*",
            "dechunk",
            "zlib.deflate",
            "zlib.inflate",
            "bzip2.compress",
            "bzip2.decompress",
        ],
    );
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_register(filter_name, class)` into the user-filter registry helper.
pub(super) fn lower_stream_filter_register(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_filter_register", 2)?;
    let filter_name = expect_operand(inst, 0)?;
    let class_name = expect_operand(inst, 1)?;
    load_string_to_result(ctx, filter_name, "stream_filter_register filter_name")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, class_name, "stream_filter_register class")?;
            ctx.emitter.instruction("mov x3, x2");                              // pass the class-name byte length as the fourth registry argument
            ctx.emitter.instruction("mov x2, x1");                              // pass the class-name pointer as the third registry argument
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, class_name, "stream_filter_register class")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the class-name byte length as the fourth registry argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the class-name pointer as the third registry argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_filter_register");
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_append` and `stream_filter_prepend`.
pub(super) fn lower_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 2, 4)?;
    let filter = expect_operand(inst, 1)?;
    if let Some(filter_name) = optional_const_string_operand(ctx, filter)? {
        if filter_name == "zlib.deflate" {
            return lower_zlib_deflate_stream_filter_attach(ctx, inst);
        }
        if filter_name == "zlib.inflate" {
            return lower_zlib_inflate_stream_filter_attach(ctx, inst);
        }
        if filter_name == "bzip2.compress" {
            return lower_bzip2_compress_stream_filter_attach(ctx, inst);
        }
        if filter_name == "bzip2.decompress" {
            return lower_bzip2_decompress_stream_filter_attach(ctx, inst);
        }
        if let Some(spec) = filter_name.strip_prefix("convert.iconv.") {
            return lower_iconv_stream_filter_attach(ctx, inst, spec);
        }
        if let Some(id) = stream_filter_id(&filter_name) {
            return lower_builtin_stream_filter_attach(ctx, inst, id);
        }
    }
    lower_user_stream_filter_attach(ctx, inst)
}

/// Lowers `stream_filter_append($stream, "zlib.deflate", ...)`.
fn lower_zlib_deflate_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    let level = const_int_filter_param(ctx, inst, "level", true, -1, 9)?.unwrap_or(-1);
    let fwrite_label = ctx.next_label("zlib_deflate_fwrite");
    let close_label = ctx.next_label("zlib_deflate_close");
    let skip_label = ctx.next_label("zlib_deflate_skip_helpers");
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::builtins::stream_filter_zlib::emit_arm64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            level,
        ),
        Arch::X86_64 => crate::codegen::builtins::stream_filter_zlib::emit_x86_64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            level,
        ),
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_append($stream, "zlib.inflate", ...)`.
fn lower_zlib_inflate_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    emit_zlib_inflate_attach_in_place(ctx);
    store_if_result(ctx, inst)
}

/// Attaches the `zlib.inflate` read filter to the stream descriptor already held
/// in the integer result register, leaving a resource-boxed `Mixed` in that
/// register. Shared by `stream_filter_append("zlib.inflate")` and the
/// `compress.zlib://` fopen wrapper.
fn emit_zlib_inflate_attach_in_place(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let labels = vec![
                ctx.next_label("zlib_inflate_slurp"),
                ctx.next_label("zlib_inflate_slurped"),
                ctx.next_label("zlib_inflate_zero"),
                ctx.next_label("zlib_inflate_zeroed"),
                ctx.next_label("zlib_inflate_write"),
                ctx.next_label("zlib_inflate_written"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::builtins::stream_filter_inflate::emit_arm64(ctx.emitter, |_| {
                labels.next().expect("zlib inflate ARM64 label")
            });
        }
        Arch::X86_64 => {
            let labels = vec![
                ctx.next_label("zlib_inflate_slurp"),
                ctx.next_label("zlib_inflate_slurped"),
                ctx.next_label("zlib_inflate_sized"),
                ctx.next_label("zlib_inflate_zero"),
                ctx.next_label("zlib_inflate_zeroed"),
                ctx.next_label("zlib_inflate_write"),
                ctx.next_label("zlib_inflate_written"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::builtins::stream_filter_inflate::emit_x86_64(ctx.emitter, |_| {
                labels.next().expect("zlib inflate x86_64 label")
            });
        }
    }
}

/// Lowers `stream_filter_append($stream, "bzip2.compress", ...)`.
fn lower_bzip2_compress_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    let block_size = const_int_filter_param(ctx, inst, "blocks", true, 1, 9)?.unwrap_or(9);
    let work_factor = const_int_filter_param(ctx, inst, "work", false, 0, 250)?.unwrap_or(0);
    let fwrite_label = ctx.next_label("bz2_compress_fwrite");
    let close_label = ctx.next_label("bz2_compress_close");
    let skip_label = ctx.next_label("bz2_compress_skip_helpers");
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::builtins::stream_filter_bzip2::emit_compress_arm64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            block_size,
            work_factor,
        ),
        Arch::X86_64 => crate::codegen::builtins::stream_filter_bzip2::emit_compress_x86_64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            block_size,
            work_factor,
        ),
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_append($stream, "bzip2.decompress", ...)`.
fn lower_bzip2_decompress_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    emit_bzip2_decompress_attach_in_place(ctx);
    store_if_result(ctx, inst)
}

/// Attaches the `bzip2.decompress` read filter to the stream descriptor already
/// held in the integer result register, leaving a resource-boxed `Mixed` in that
/// register. Shared by `stream_filter_append("bzip2.decompress")` and the
/// `compress.bzip2://` fopen wrapper.
fn emit_bzip2_decompress_attach_in_place(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let labels = vec![
                ctx.next_label("bz2_slurp"),
                ctx.next_label("bz2_slurped"),
                ctx.next_label("bz2_write"),
                ctx.next_label("bz2_written"),
                ctx.next_label("bz2_decompress_fail"),
                ctx.next_label("bz2_done_arm"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::builtins::stream_filter_bzip2::emit_decompress_arm64(ctx.emitter, |_| {
                labels.next().expect("bzip2 decompress ARM64 label")
            });
        }
        Arch::X86_64 => {
            let labels = vec![
                ctx.next_label("bz2_slurp_x"),
                ctx.next_label("bz2_slurped_x"),
                ctx.next_label("bz2_write_x"),
                ctx.next_label("bz2_written_x"),
                ctx.next_label("bz2_decompress_fail_x"),
                ctx.next_label("bz2_done_x"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::builtins::stream_filter_bzip2::emit_decompress_x86_64(ctx.emitter, |_| {
                labels.next().expect("bzip2 decompress x86_64 label")
            });
        }
    }
}

/// Lowers `fopen("compress.zlib://<path>", ...)` for a compile-time literal path.
/// Opens the underlying file read-only and attaches the `zlib.inflate` filter so
/// reads see decompressed bytes; an empty or unopenable path boxes PHP false.
fn lower_literal_compress_zlib_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    let underlying = path.strip_prefix("compress.zlib://").unwrap_or("");
    emit_literal_compress_wrapper_fopen(ctx, inst, underlying, CompressWrapper::Zlib)
}

/// Lowers `fopen("compress.bzip2://<path>", ...)` for a compile-time literal path.
/// Opens the underlying file read-only and attaches the `bzip2.decompress` filter
/// so reads see decompressed bytes; an empty or unopenable path boxes PHP false.
fn lower_literal_compress_bzip2_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    let underlying = path.strip_prefix("compress.bzip2://").unwrap_or("");
    emit_literal_compress_wrapper_fopen(ctx, inst, underlying, CompressWrapper::Bzip2)
}

/// Selects which read-direction decompressor a `compress.*://` fopen wrapper attaches.
#[derive(Clone, Copy)]
enum CompressWrapper {
    Zlib,
    Bzip2,
}

/// Opens `underlying` read-only through `__rt_fopen` and attaches the matching
/// decompressor so subsequent reads see plain bytes, boxing the filtered
/// descriptor as a resource. An empty path, or a failed open, boxes PHP false —
/// matching PHP's `compress.zlib://` / `compress.bzip2://` wrapper behavior.
fn emit_literal_compress_wrapper_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    underlying: &str,
    kind: CompressWrapper,
) -> Result<()> {
    if underlying.is_empty() {
        emit_fd_result(ctx, -1);
        box_stream_fd_or_false_result(ctx, "fopen");
        return store_if_result(ctx, inst);
    }
    let (path_label, path_len) = ctx.data.add_string(underlying.as_bytes());
    let (mode_label, mode_len) = ctx.data.add_string(b"r");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &path_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", path_len));         // pass the underlying path byte length
            abi::emit_symbol_address(ctx.emitter, "x3", &mode_label);
            ctx.emitter.instruction(&format!("mov x4, #{}", mode_len));         // pass the read-mode string byte length
            abi::emit_call_label(ctx.emitter, "__rt_fopen");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &path_label);
            ctx.emitter.instruction(&format!("mov rdx, {}", path_len));         // pass the underlying path byte length
            abi::emit_symbol_address(ctx.emitter, "rdi", &mode_label);
            ctx.emitter.instruction(&format!("mov rsi, {}", mode_len));         // pass the read-mode string byte length
            abi::emit_call_label(ctx.emitter, "__rt_fopen");
        }
    }
    let false_label = ctx.next_label("compress_fopen_false");
    let done_label = ctx.next_label("compress_fopen_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // negative descriptor means the underlying open failed
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false when the source could not be opened
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // negative descriptor means the underlying open failed
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false when the source could not be opened
        }
    }
    match kind {
        CompressWrapper::Zlib => emit_zlib_inflate_attach_in_place(ctx),
        CompressWrapper::Bzip2 => emit_bzip2_decompress_attach_in_place(ctx),
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done_label)), // skip false boxing after attaching the decompressor
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done_label)),// skip false boxing after attaching the decompressor
    }
    ctx.emitter.label(&false_label);
    box_stream_fd_or_false_result(ctx, "fopen");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_append($stream, "convert.iconv.<from>/<to>", ...)`.
fn lower_iconv_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    spec: &str,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    let Some((from, to)) = spec.split_once('/') else {
        emit_boxed_stream_resource(ctx);
        return store_if_result(ctx, inst);
    };
    if from.is_empty() || to.is_empty() {
        emit_boxed_stream_resource(ctx);
        return store_if_result(ctx, inst);
    }
    let from_cstr = format!("{}\0", from);
    let to_cstr = format!("{}\0", to);
    let (from_sym, _) = ctx.data.add_string(from_cstr.as_bytes());
    let (to_sym, _) = ctx.data.add_string(to_cstr.as_bytes());
    let write_label = ctx.next_label("iconv_mode_write");
    let after_label = ctx.next_label("iconv_mode_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // preserve the descriptor across mode evaluation
            materialize_stream_filter_mode(ctx, inst)?;
            ctx.emitter.instruction("mov x9, x0");                              // hold the selected stream-filter mode
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // restore the stream descriptor
            ctx.emitter.instruction("cmp x9, #2");                              // test for STREAM_FILTER_WRITE-only mode
            ctx.emitter.instruction(&format!("b.eq {}", write_label));          // install the streaming write transcoder
            emit_iconv_read_transform_for_current_fd(ctx, &from_sym, &to_sym);
            ctx.emitter.instruction(&format!("b {}", after_label));             // skip the write-filter attach path
            ctx.emitter.label(&write_label);
            emit_iconv_write_transform_for_current_fd(ctx, &from_sym, &to_sym);
            ctx.emitter.label(&after_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            materialize_stream_filter_mode(ctx, inst)?;
            ctx.emitter.instruction("mov r9, rax");                             // hold the selected stream-filter mode
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("cmp r9, 2");                               // test for STREAM_FILTER_WRITE-only mode
            ctx.emitter.instruction(&format!("je {}", write_label));            // install the streaming write transcoder
            emit_iconv_read_transform_for_current_fd(ctx, &from_sym, &to_sym);
            ctx.emitter.instruction(&format!("jmp {}", after_label));           // skip the write-filter attach path
            ctx.emitter.label(&write_label);
            emit_iconv_write_transform_for_current_fd(ctx, &from_sym, &to_sym);
            ctx.emitter.label(&after_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Emits the attach-time READ transform for the current iconv stream descriptor.
fn emit_iconv_read_transform_for_current_fd(
    ctx: &mut FunctionContext<'_>,
    from_sym: &str,
    to_sym: &str,
) {
    let labels = vec![
        ctx.next_label("iconv_slurp"),
        ctx.next_label("iconv_slurped"),
        ctx.next_label("iconv_sized"),
        ctx.next_label("iconv_skip"),
        ctx.next_label("iconv_write"),
        ctx.next_label("iconv_written"),
    ];
    let mut labels = labels.into_iter();
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::builtins::stream_filter_iconv::emit_read_arm64(
            ctx.emitter,
            from_sym,
            to_sym,
            |_| labels.next().expect("iconv read transform label"),
        ),
        Arch::X86_64 => crate::codegen::builtins::stream_filter_iconv::emit_read_x86_64(
            ctx.emitter,
            from_sym,
            to_sym,
            |_| labels.next().expect("iconv read transform label"),
        ),
    }
}

/// Emits the WRITE transform attachment for the current iconv stream descriptor.
fn emit_iconv_write_transform_for_current_fd(
    ctx: &mut FunctionContext<'_>,
    from_sym: &str,
    to_sym: &str,
) {
    let labels = vec![
        ctx.next_label("iconv_w_fwrite"),
        ctx.next_label("iconv_w_close"),
        ctx.next_label("iconv_w_skip_helpers"),
        ctx.next_label("iconv_w_loop"),
        ctx.next_label("iconv_w_after_write"),
        ctx.next_label("iconv_w_done"),
        ctx.next_label("iconv_w_skip_store"),
    ];
    let mut labels = labels.into_iter();
    crate::codegen::builtins::stream_filter_iconv_write::emit_iconv_write_attach_with_labels(
        ctx.emitter,
        from_sym,
        to_sym,
        |_| labels.next().expect("iconv write transform label"),
    );
}

/// Lowers `stream_filter_remove(filter)` and clears both direction tables for the fd.
pub(super) fn lower_stream_filter_remove(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_filter_remove", 1)?;
    let filter = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, filter, "stream_filter_remove")?;
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the user-filter teardown helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_filter_release_fd");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear the read-direction filter slot for this descriptor
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear the write-direction filter slot for this descriptor
            ctx.emitter.instruction("mov x0, #1");                              // return true after removing the filter state
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters"); // read-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear the read-direction filter slot for this descriptor
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters"); // write-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear the write-direction filter slot for this descriptor
            ctx.emitter.instruction("mov eax, 1");                              // return true after removing the filter state
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_bucket_new(stream, data)` into a stdClass-backed bucket object.
pub(super) fn lower_stream_bucket_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_bucket_new", 2)?;
    let stream = expect_operand(inst, 0)?;
    let data_value = expect_operand(inst, 1)?;
    ctx.load_value_to_result(stream)?;
    load_string_to_result(ctx, data_value, "stream_bucket_new buffer")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_stream_bucket_new_aarch64(ctx),
        Arch::X86_64 => lower_stream_bucket_new_x86_64(ctx),
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_bucket_make_writeable(brigade)` by popping the brigade head.
pub(super) fn lower_stream_bucket_make_writeable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_bucket_make_writeable", 1)?;
    let brigade = expect_operand(inst, 0)?;
    let arg_ty = ctx.load_value_to_result(brigade)?;
    let arg_is_mixed = matches!(arg_ty, PhpType::Mixed | PhpType::Union(_));
    let (buckets_sym, buckets_len) = ctx.data.add_string(b"_buckets");
    let return_null = ctx.next_label("sbmw_null");
    let done = ctx.next_label("sbmw_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if arg_is_mixed {
                ctx.emitter.instruction(&format!("cbz x0, {}", return_null));   // null Mixed means there is no brigade object
                ctx.emitter.instruction("ldr x9, [x0]");                        // load the Mixed runtime tag
                ctx.emitter.instruction("cmp x9, #6");                          // tag 6 identifies object values
                ctx.emitter.instruction(&format!("b.ne {}", return_null));      // non-object brigades are empty
                ctx.emitter.instruction("ldr x0, [x0, #8]");                    // unbox the stdClass object pointer
            }
            ctx.emitter.instruction(&format!("cbz x0, {}", return_null));       // missing brigade object returns null
            abi::emit_symbol_address(ctx.emitter, "x1", &buckets_sym);
            ctx.emitter.instruction(&format!("mov x2, #{}", buckets_len));      // pass the `_buckets` property-name length
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", return_null));       // missing `_buckets` property returns null
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the property Mixed tag
            ctx.emitter.instruction("cmp x9, #4");                              // tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("b.ne {}", return_null));          // non-array `_buckets` is treated as empty
            ctx.emitter.instruction("ldr x9, [x0, #8]");                        // unbox the indexed-array pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", return_null));       // null array payload returns null
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the indexed-array length
            ctx.emitter.instruction(&format!("cbz x10, {}", return_null));      // an empty brigade returns null
            ctx.emitter.instruction("mov x0, x9");                              // pass the array pointer to array_shift
            abi::emit_call_label(ctx.emitter, "__rt_array_shift");
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the null-result path
            ctx.emitter.label(&return_null);
            emit_null_mixed(ctx);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            if arg_is_mixed {
                ctx.emitter.instruction("test rax, rax");                       // null Mixed means there is no brigade object
                ctx.emitter.instruction(&format!("jz {}", return_null));        // branch to the PHP null result
                ctx.emitter.instruction("mov r10, QWORD PTR [rax]");            // load the Mixed runtime tag
                ctx.emitter.instruction("cmp r10, 6");                          // tag 6 identifies object values
                ctx.emitter.instruction(&format!("jne {}", return_null));       // non-object brigades are empty
                ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");        // unbox the stdClass object pointer
            }
            ctx.emitter.instruction("test rax, rax");                           // missing brigade object returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov rdi, rax");                            // pass the brigade object to stdClass lookup
            abi::emit_symbol_address(ctx.emitter, "rsi", &buckets_sym);
            ctx.emitter.instruction(&format!("mov rdx, {}", buckets_len));      // pass the `_buckets` property-name length
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
            ctx.emitter.instruction("test rax, rax");                           // missing `_buckets` property returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // load the property Mixed tag
            ctx.emitter.instruction("cmp r10, 4");                              // tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("jne {}", return_null));           // non-array `_buckets` is treated as empty
            ctx.emitter.instruction("mov r10, QWORD PTR [rax + 8]");            // unbox the indexed-array pointer
            ctx.emitter.instruction("test r10, r10");                           // null array payload returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // load the indexed-array length
            ctx.emitter.instruction("test r11, r11");                           // an empty brigade returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov rdi, r10");                            // pass the array pointer to array_shift
            abi::emit_call_label(ctx.emitter, "__rt_array_shift");
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the null-result path
            ctx.emitter.label(&return_null);
            emit_null_mixed(ctx);
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_bucket_append` and `stream_bucket_prepend` over the `_buckets` array.
pub(super) fn lower_stream_bucket_append_or_prepend(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_bucket_append/prepend", 2)?;
    let brigade = expect_operand(inst, 0)?;
    let bucket = expect_operand(inst, 1)?;
    let brigade_ty = ctx.load_value_to_result(brigade)?;
    let brigade_is_mixed = matches!(brigade_ty, PhpType::Mixed | PhpType::Union(_));
    let (buckets_sym, buckets_len) = ctx.data.add_string(b"_buckets");
    let done = ctx.next_label("sba_done");
    let init = ctx.next_label("sba_init");
    let existing = ctx.next_label("sba_existing");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_stream_bucket_append_aarch64(
            ctx,
            bucket,
            brigade_is_mixed,
            &buckets_sym,
            buckets_len,
            &done,
            &init,
            &existing,
        )?,
        Arch::X86_64 => lower_stream_bucket_append_x86_64(
            ctx,
            bucket,
            brigade_is_mixed,
            &buckets_sym,
            buckets_len,
            &done,
            &init,
            &existing,
        )?,
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_is_local(stream)` as a true predicate after evaluating its argument.
pub(super) fn lower_stream_is_local(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_is_local", 1)?;
    let stream = expect_operand(inst, 0)?;
    ctx.load_value_to_result(stream)?;
    emit_bool_result(ctx, true);
    store_if_result(ctx, inst)
}

/// Lowers `stream_supports_lock(stream)` as true after resource unboxing.
pub(super) fn lower_stream_supports_lock(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_supports_lock", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_supports_lock")?;
    emit_bool_result(ctx, true);
    store_if_result(ctx, inst)
}

/// Lowers `stream_isatty(stream)`.
pub(super) fn lower_stream_isatty(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_isatty", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_isatty")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the runtime terminal probe
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_isatty");
    store_if_result(ctx, inst)
}

/// Lowers `stream_set_blocking(stream, enable)`.
pub(super) fn lower_stream_set_blocking(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_set_blocking", 2)?;
    let stream = expect_operand(inst, 0)?;
    let enable = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_set_blocking")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int_or_bool(
        ctx.load_value_to_result(enable)?.codegen_repr(),
        "stream_set_blocking enable",
    )?;
    let wrapper = ctx.next_label("set_blocking_wrapper");
    let after = ctx.next_label("set_blocking_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the blocking flag as the native helper's second argument
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrapper));              // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_blocking");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov x2, x1");                              // pass the blocking flag as wrapper option arg1
            ctx.emitter.instruction(&format!("mov x1, #{}", STREAM_OPTION_BLOCKING)); // select STREAM_OPTION_BLOCKING
            ctx.emitter.instruction("mov x3, #0");                              // pass zero as wrapper option arg2
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the blocking flag as the native helper's second argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrapper));               // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_blocking");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rdx, rsi");                            // pass the blocking flag as wrapper option arg1
            ctx.emitter.instruction(&format!("mov rsi, {}", STREAM_OPTION_BLOCKING)); // select STREAM_OPTION_BLOCKING
            ctx.emitter.instruction("xor ecx, ecx");                            // pass zero as wrapper option arg2
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_set_chunk_size(stream, size)` and returns the previous size.
pub(super) fn lower_stream_set_chunk_size(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_set_chunk_size", 2)?;
    let stream = expect_operand(inst, 0)?;
    let size = expect_operand(inst, 1)?;
    let default_label = ctx.next_label("stream_chunk_default");
    let have_old_label = ctx.next_label("stream_chunk_have_old");
    let done_label = ctx.next_label("stream_chunk_done");
    load_stream_fd_to_result(ctx, stream, "stream_set_chunk_size")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(size)?.codegen_repr(),
        "stream_set_chunk_size size",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // keep the new chunk size while restoring the stream fd
            abi::emit_pop_reg(ctx.emitter, "x2");
            ctx.emitter.instruction("cmp x2, #0");                              // negative descriptors cannot index the chunk-size table
            ctx.emitter.instruction(&format!("b.lt {}", default_label));        // out-of-range descriptors report the default
            ctx.emitter.instruction("cmp x2, #256");                            // descriptors above the fixed table are ignored
            ctx.emitter.instruction(&format!("b.ge {}", default_label));        // out-of-range descriptors report the default
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_chunk_size");
            ctx.emitter.instruction("ldr x10, [x9, x2, lsl #3]");               // load the previous per-fd chunk size
            ctx.emitter.instruction(&format!("cbnz x10, {}", have_old_label));  // keep a previously stored size when present
            ctx.emitter.instruction("mov x10, #8192");                          // use PHP's default stream chunk size
            ctx.emitter.label(&have_old_label);
            ctx.emitter.instruction("str x1, [x9, x2, lsl #3]");                // store the new chunk size for this fd
            ctx.emitter.instruction("mov x0, x10");                             // return the previous chunk size
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the default-only path
            ctx.emitter.label(&default_label);
            ctx.emitter.instruction("mov x0, #8192");                           // report PHP's default chunk size
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // keep the new chunk size while restoring the stream fd
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("cmp rdi, 0");                              // negative descriptors cannot index the chunk-size table
            ctx.emitter.instruction(&format!("jl {}", default_label));          // out-of-range descriptors report the default
            ctx.emitter.instruction("cmp rdi, 256");                            // descriptors above the fixed table are ignored
            ctx.emitter.instruction(&format!("jge {}", default_label));         // out-of-range descriptors report the default
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_chunk_size");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9 + rdi * 8]");       // load the previous per-fd chunk size
            ctx.emitter.instruction("test rax, rax");                           // check whether a previous size exists
            ctx.emitter.instruction(&format!("jnz {}", have_old_label));        // keep a previously stored size when present
            ctx.emitter.instruction("mov eax, 8192");                           // use PHP's default stream chunk size
            ctx.emitter.label(&have_old_label);
            ctx.emitter.instruction("mov QWORD PTR [r9 + rdi * 8], rsi");       // store the new chunk size for this fd
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the default-only path
            ctx.emitter.label(&default_label);
            ctx.emitter.instruction("mov eax, 8192");                           // report PHP's default chunk size
            ctx.emitter.label(&done_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers stream read/write buffer setters as successful no-ops.
pub(super) fn lower_stream_set_buffer(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_set_buffer", 2, 2)?;
    for operand in &inst.operands {
        ctx.load_value_to_result(*operand)?;
    }
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    store_if_result(ctx, inst)
}

/// Lowers `stream_set_timeout(stream, seconds, microseconds?)`.
pub(super) fn lower_stream_set_timeout(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_set_timeout", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let seconds = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_set_timeout")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(seconds)?.codegen_repr(),
        "stream_set_timeout seconds",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if inst.operands.len() == 3 {
                let usec = expect_operand(inst, 2)?;
                require_int(
                    ctx.load_value_to_result(usec)?.codegen_repr(),
                    "stream_set_timeout microseconds",
                )?;
                ctx.emitter.instruction("mov x2, x0");                          // pass explicit microseconds as the third runtime argument
            } else {
                ctx.emitter.instruction("mov x2, #0");                          // default omitted microseconds to zero
            }
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            if inst.operands.len() == 3 {
                let usec = expect_operand(inst, 2)?;
                require_int(
                    ctx.load_value_to_result(usec)?.codegen_repr(),
                    "stream_set_timeout microseconds",
                )?;
                ctx.emitter.instruction("mov rdx, rax");                        // pass explicit microseconds as the third runtime argument
            } else {
                ctx.emitter.instruction("xor edx, edx");                        // default omitted microseconds to zero
            }
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    lower_stream_timeout_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stream_select(read, write, except, seconds, microseconds?)`.
pub(super) fn lower_stream_select(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_select", 4, 5)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    for idx in 0..4 {
        let value = expect_operand(inst, idx)?;
        ctx.load_value_to_result(value)?;
        abi::emit_push_reg(ctx.emitter, result_reg);
    }
    if inst.operands.len() == 5 {
        let microseconds = expect_operand(inst, 4)?;
        ctx.load_value_to_result(microseconds)?;
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #0");                          // default omitted microseconds to zero
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor eax, eax");                        // default omitted microseconds to zero
            }
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x4, x0");                              // pass microseconds as the fifth runtime argument
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, rax");                             // pass microseconds as the fifth runtime argument
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_select");
    store_if_result(ctx, inst)
}

/// Lowers `stream_resolve_include_path(filename)` as realpath-backed `string|false`.
pub(super) fn lower_stream_resolve_include_path(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_resolve_include_path", 1)?;
    let filename = expect_operand(inst, 0)?;
    load_string_to_result(ctx, filename, "stream_resolve_include_path")?;
    abi::emit_call_label(ctx.emitter, "__rt_realpath");
    box_owned_string_or_false_result(ctx, "stream_resolve_include_path");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_server(address)` and boxes `resource|false`.
pub(super) fn lower_stream_socket_server(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_socket_server", 1)?;
    let address = expect_operand(inst, 0)?;
    load_string_to_result(ctx, address, "stream_socket_server address")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the socket address byte length as the second runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the socket address byte length as the second runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_server");
    box_stream_fd_or_false_result(ctx, "stream_socket_server");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_client(address)` and records the connected host for TLS defaults.
pub(super) fn lower_stream_socket_client(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_socket_client", 1)?;
    let address = expect_operand(inst, 0)?;
    load_string_to_result(ctx, address, "stream_socket_client address")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve scratch storage for the original address string
            ctx.emitter.instruction("str x1, [sp, #0]");                        // save the address pointer across connect
            ctx.emitter.instruction("str x2, [sp, #8]");                        // save the address byte length across connect
            ctx.emitter.instruction("mov x0, x1");                              // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the socket address byte length as the second runtime argument
            abi::emit_call_label(ctx.emitter, "__rt_stream_socket_client");
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // reload the address pointer for host stashing
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // reload the address byte length for host stashing
            ctx.emitter.instruction("add sp, sp, #16");                         // release the address scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve scratch storage for the original address string
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // save the address pointer across connect
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // save the address byte length across connect
            ctx.emitter.instruction("mov rdi, rax");                            // pass the socket address pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the socket address byte length as the second runtime argument
            abi::emit_call_label(ctx.emitter, "__rt_stream_socket_client");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the connected fd to the host-stash helper
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");            // reload the address pointer for host stashing
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // reload the address byte length for host stashing
            ctx.emitter.instruction("add rsp, 16");                             // release the address scratch storage
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
        }
    }
    box_stream_fd_or_false_result(ctx, "stream_socket_client");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_accept(server, timeout?, peer_name?)`.
pub(super) fn lower_stream_socket_accept(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_accept", 1, 3)?;
    let server = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, server, "stream_socket_accept")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    lower_stream_socket_accept_timeout(ctx, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass timeout microseconds as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass timeout microseconds as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_accept");
    box_stream_fd_or_false_result(ctx, "stream_socket_accept");
    if inst.operands.len() == 3 {
        let peer = expect_operand(inst, 2)?;
        store_accept_peer_name(ctx, peer)?;
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_pair(domain, type, protocol)` and boxes `array|false`.
pub(super) fn lower_stream_socket_pair(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_socket_pair", 3)?;
    let domain = expect_operand(inst, 0)?;
    let socket_type = expect_operand(inst, 1)?;
    let protocol = expect_operand(inst, 2)?;
    ctx.load_value_to_result(domain)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(socket_type)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(protocol)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x2, x0");                              // pass protocol as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // pass protocol as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_pair");
    box_stream_socket_pair_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_get_name(socket, remote)` and boxes `string|false`.
pub(super) fn lower_stream_socket_get_name(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_socket_get_name", 2)?;
    let socket = expect_operand(inst, 0)?;
    let remote = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, socket, "stream_socket_get_name")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(remote)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the remote flag as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the remote flag as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_get_name");
    box_owned_string_or_false_result(ctx, "stream_socket_get_name");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_shutdown(stream, mode)`.
pub(super) fn lower_stream_socket_shutdown(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "stream_socket_shutdown", 2)?;
    let stream = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_socket_shutdown")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    ctx.load_value_to_result(mode)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the shutdown mode as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the shutdown mode as the second runtime argument
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_shutdown");
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_enable_crypto(stream, enable, method?, session_stream?)`.
pub(super) fn lower_stream_socket_enable_crypto(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_enable_crypto", 2, 4)?;
    let stream = expect_operand(inst, 0)?;
    let enable = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_socket_enable_crypto")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int_or_bool(
        ctx.load_value_to_result(enable)?.codegen_repr(),
        "stream_socket_enable_crypto enable",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for index in 2..inst.operands.len() {
        let operand = expect_operand(inst, index)?;
        ctx.load_value_to_result(operand)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg(ctx.emitter, "x0"),
        Arch::X86_64 => abi::emit_pop_reg(ctx.emitter, "rax"),
    }
    let enable_label = ctx.next_label("ssec_enable");
    let done_label = ctx.next_label("ssec_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", enable_label));     // enable=true enters the TLS attach path
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the stashed descriptor for TLS teardown
            emit_tls_session_teardown_for_current_fd(ctx);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction("mov x0, #1");                              // disabling crypto succeeds even when no session exists
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the TLS attach path
            ctx.emitter.label(&enable_label);
            lower_stream_socket_enable_crypto_attach_aarch64(ctx, &done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the caller request TLS enablement?
            ctx.emitter.instruction(&format!("jnz {}", enable_label));          // enable=true enters the TLS attach path
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // reload the stashed descriptor for TLS teardown
            emit_tls_session_teardown_for_current_fd(ctx);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction("mov eax, 1");                              // disabling crypto succeeds even when no session exists
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the TLS attach path
            ctx.emitter.label(&enable_label);
            lower_stream_socket_enable_crypto_attach_x86_64(ctx, &done_label);
        }
    }
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_recvfrom(socket, length, flags?, address?)`.
pub(super) fn lower_stream_socket_recvfrom(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_recvfrom", 2, 4)?;
    let socket = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, socket, "stream_socket_recvfrom")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(
        ctx.load_value_to_result(length)?.codegen_repr(),
        "stream_socket_recvfrom length",
    )?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if inst.operands.len() >= 3 {
        let flags = expect_operand(inst, 2)?;
        ctx.load_value_to_result(flags)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x2, x0");                              // pass receive flags as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // pass receive flags as the third runtime argument
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_recvfrom");
    box_owned_string_or_false_result(ctx, "stream_socket_recvfrom");
    if inst.operands.len() == 4 {
        let address = expect_operand(inst, 3)?;
        store_recvfrom_address(ctx, address)?;
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_socket_sendto(socket, data, flags?, address?)` and boxes `int|false`.
pub(super) fn lower_stream_socket_sendto(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "stream_socket_sendto", 2, 4)?;
    let socket = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, socket, "stream_socket_sendto")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, data, "stream_socket_sendto data")?;
            abi::emit_push_reg(ctx.emitter, "x1");
            abi::emit_push_reg(ctx.emitter, "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, data, "stream_socket_sendto data")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_push_reg(ctx.emitter, "rdx");
        }
    }
    if inst.operands.len() >= 3 {
        let flags = expect_operand(inst, 2)?;
        ctx.load_value_to_result(flags)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if inst.operands.len() >= 4 {
                let address = expect_operand(inst, 3)?;
                load_string_to_result(ctx, address, "stream_socket_sendto address")?;
                ctx.emitter.instruction("mov x4, x1");                          // pass the destination address pointer as the fifth runtime argument
                ctx.emitter.instruction("mov x5, x2");                          // pass the destination address length as the sixth runtime argument
            } else {
                ctx.emitter.instruction("mov x4, #0");                          // omitted destination address uses the connected peer
                ctx.emitter.instruction("mov x5, #0");                          // omitted destination address has zero byte length
            }
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_pop_reg(ctx.emitter, "x1");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            if inst.operands.len() >= 4 {
                let address = expect_operand(inst, 3)?;
                load_string_to_result(ctx, address, "stream_socket_sendto address")?;
                ctx.emitter.instruction("mov r8, rax");                         // pass the destination address pointer as the fifth runtime argument
                ctx.emitter.instruction("mov r9, rdx");                         // pass the destination address length as the sixth runtime argument
            } else {
                ctx.emitter.instruction("xor r8d, r8d");                        // omitted destination address uses the connected peer
                ctx.emitter.instruction("xor r9d, r9d");                        // omitted destination address has zero byte length
            }
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_pop_reg(ctx.emitter, "rsi");
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_socket_sendto");
    box_negative_int_or_false_result(ctx, "stream_socket_sendto");
    store_if_result(ctx, inst)
}

/// Lowers `fclose(stream)` after validating and unboxing the stream handle.
pub(super) fn lower_fclose(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fclose", 1)?;
    let stream = expect_operand(inst, 0)?;
    let captured = capture_resource_box_for_release(ctx, stream)?;
    load_stream_fd_to_result(ctx, stream, "fclose")?;
    apply_resource_release_sentinel(ctx, captured);
    let success_label = ctx.next_label("fclose_ok");
    let done_label = ctx.next_label("fclose_done");
    let user_wrapper_label = ctx.next_label("fclose_user_wrapper");
    let phar_label = ctx.next_label("fclose_phar");
    let not_phar_label = ctx.next_label("fclose_not_phar");
    let after_dispatch_label = ctx.next_label("fclose_after_dispatch");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x5000");                         // low half of the phar-write descriptor base 0x50000000
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the phar-write synthetic descriptor base
            ctx.emitter.instruction("cmp x0, x9");                              // is the descriptor below the phar-write range?
            ctx.emitter.instruction(&format!("b.lt {}", not_phar_label));       // below the PHAR range: continue with normal dispatch
            ctx.emitter.instruction("add x10, x9, #32");                        // upper bound for the 32 buffered PHAR write descriptors
            ctx.emitter.instruction("cmp x0, x10");                             // is this inside the phar-write descriptor range?
            ctx.emitter.instruction(&format!("b.lt {}", phar_label));           // finalize phar writes instead of closing a real fd
            ctx.emitter.label(&not_phar_label);
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this is a userspace-wrapper stream
            ctx.emitter.instruction(&format!("b.ge {}", user_wrapper_label));   // dispatch synthetic handles without indexing fd tables
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x50000000");                     // materialize the phar-write synthetic descriptor base
            ctx.emitter.instruction("cmp rax, r9");                             // is the descriptor below the phar-write range?
            ctx.emitter.instruction(&format!("jl {}", not_phar_label));         // below the PHAR range: continue with normal dispatch
            ctx.emitter.instruction("lea r10, [r9 + 32]");                      // upper bound for the 32 buffered PHAR write descriptors
            ctx.emitter.instruction("cmp rax, r10");                            // is this inside the phar-write descriptor range?
            ctx.emitter.instruction(&format!("jl {}", phar_label));             // finalize phar writes instead of closing a real fd
            ctx.emitter.label(&not_phar_label);
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this is a userspace-wrapper stream
            ctx.emitter.instruction(&format!("jge {}", user_wrapper_label));    // dispatch synthetic handles without indexing fd tables
        }
    }
    emit_zlib_flush_on_close_for_current_fd(ctx);
    emit_bz2_flush_on_close_for_current_fd(ctx);
    emit_iconv_flush_on_close_for_current_fd(ctx);
    emit_tls_session_teardown_for_current_fd(ctx);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the user-filter teardown helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_filter_release_fd");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear any read filter before the descriptor can be reused
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear any write filter before the descriptor can be reused
            ctx.emitter.syscall(6);
            ctx.emitter.instruction("cmp x0, #0");                              // test whether close() reported success
            ctx.emitter.instruction(&format!("b.eq {}", success_label));        // branch to the true result when the stream closed cleanly
            ctx.emitter.instruction("mov x0, #0");                              // return false when the stream close failed
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the success result on the failure path
            ctx.emitter.label(&success_label);
            ctx.emitter.instruction("mov x0, #1");                              // return true when the stream close succeeded
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters"); // read-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear any read filter before the descriptor can be reused
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters"); // write-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear any write filter before the descriptor can be reused
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream fd to libc close()
            ctx.emitter.instruction("call close");                              // close the requested stream descriptor
            ctx.emitter.instruction("cmp rax, 0");                              // test whether close() reported success
            ctx.emitter.instruction(&format!("je {}", success_label));          // branch to the true result when the stream closed cleanly
            ctx.emitter.instruction("xor eax, eax");                            // return false when the stream close failed
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the success result on the failure path
            ctx.emitter.label(&success_label);
            ctx.emitter.instruction("mov rax, 1");                              // return true when the stream close succeeded
        }
    }
    ctx.emitter.label(&done_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip synthetic close handlers after the native fd path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip synthetic close handlers after the native fd path
        }
    }
    ctx.emitter.label(&user_wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the close helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fclose");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip phar finalization after wrapper close dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip phar finalization after wrapper close dispatch
        }
    }
    ctx.emitter.label(&phar_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the PHAR write descriptor to the finalizer
    }
    abi::emit_call_label(ctx.emitter, "__rt_phar_write_finalize");
    ctx.emitter.label(&after_dispatch_label);
    store_if_result(ctx, inst)
}

/// Lowers `fread(stream, length)` using the shared runtime file-read helper.
pub(super) fn lower_fread(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fread", 2)?;
    let stream = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fread")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(length)?.codegen_repr(), "fread length")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the requested byte count to the fread runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the requested byte count to the fread runtime helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fread");
    store_if_result(ctx, inst)
}

/// Lowers `fwrite(stream, data)` and returns the number of bytes written.
pub(super) fn lower_fwrite(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fwrite", 2)?;
    let stream = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fwrite")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, data, "fwrite data")?;
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, data, "fwrite data")?;
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string pointer to the runtime fwrite helper
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fprintf(stream, format, values...)` as `sprintf()` plus stream write.
pub(super) fn lower_fprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fprintf", 2, usize::MAX)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    let spec_cats = super::strings::sprintf_spec_cats_for_format(ctx, format)?;
    load_stream_fd_to_result(ctx, stream, "fprintf")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for index in (2..inst.operands.len()).rev() {
        let value = expect_operand(inst, index)?;
        let spec_cat = spec_cats.get(index - 2).copied();
        super::strings::pack_sprintf_like_arg(ctx, value, spec_cat, "fprintf")?;
    }
    load_string_to_result(ctx, format, "fprintf format")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, #{}", inst.operands.len() - 2)); // pass the number of packed fprintf operands
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", (inst.operands.len() - 2) as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_sprintf");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the formatted string pointer to fwrite
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fwrite");
    store_if_result(ctx, inst)
}

/// Lowers `vfprintf(stream, format, values)` through `__rt_vsprintf` then fwrite.
pub(super) fn lower_vfprintf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "vfprintf", 3)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    let values = expect_operand(inst, 2)?;
    load_stream_fd_to_result(ctx, stream, "vfprintf")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve fd and format scratch storage
            ctx.emitter.instruction("str x0, [sp, #0]");                        // save the descriptor across formatting
            load_string_to_result(ctx, format, "vfprintf format")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #8]");                    // save the format pointer and length
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("ldp x1, x2, [sp, #8]");                    // restore the format pointer and length
            abi::emit_call_label(ctx.emitter, "__rt_vsprintf");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the destination descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
            ctx.emitter.instruction("add sp, sp, #32");                         // release vfprintf scratch storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve fd and format scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save the descriptor across formatting
            load_string_to_result(ctx, format, "vfprintf format")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // save the format pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdx");           // save the format byte length
            ctx.load_value_to_result(values)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the values array to vsprintf
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // restore the format pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // restore the format byte length
            abi::emit_call_label(ctx.emitter, "__rt_vsprintf");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the formatted string pointer to fwrite
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the destination descriptor
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
            ctx.emitter.instruction("add rsp, 32");                             // release vfprintf scratch storage
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fscanf(stream, format)` through `__rt_fgets` and `__rt_sscanf`.
pub(super) fn lower_fscanf(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fscanf", 2, usize::MAX)?;
    let stream = expect_operand(inst, 0)?;
    let format = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fscanf")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_fgets");
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, format, "fscanf format")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the format pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the format length as the secondary string argument
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the descriptor to fgets
            abi::emit_call_label(ctx.emitter, "__rt_fgets");
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, format, "fscanf format")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the format pointer as the secondary string argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the format length as the secondary string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_sscanf");
    store_if_result(ctx, inst)
}

/// Lowers `fgets(stream)` through the shared line-read runtime helper.
pub(super) fn lower_fgets(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fgets", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fgets")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 fgets runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgets");
    box_stream_string_or_false_on_empty_result(ctx, "fgets");
    store_if_result(ctx, inst)
}

/// Lowers `fgetc(stream)` and boxes the one-byte string or PHP false result.
pub(super) fn lower_fgetc(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fgetc", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fgetc")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 fgetc runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgetc");
    box_stream_string_or_false_on_empty_result(ctx, "fgetc");
    store_if_result(ctx, inst)
}

/// Lowers `fgetcsv(stream, separator?, enclosure?)` through the CSV row runtime helper.
pub(super) fn lower_fgetcsv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fgetcsv", 1, 3)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fgetcsv")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 fgetcsv runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_fgetcsv");
    store_if_result(ctx, inst)
}

/// Lowers `fputcsv(stream, fields, separator?, enclosure?)` for string arrays.
pub(super) fn lower_fputcsv(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fputcsv", 2, 4)?;
    let stream = expect_operand(inst, 0)?;
    let fields = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fputcsv")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_string_array(ctx.load_value_to_result(fields)?.codegen_repr(), "fputcsv fields")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the string-array pointer to the fputcsv runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string-array pointer to the fputcsv runtime helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fputcsv");
    store_if_result(ctx, inst)
}

/// Lowers `fpassthru(stream)` through the remaining-bytes stream runtime helper.
pub(super) fn lower_fpassthru(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fpassthru", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fpassthru")?;
    emit_fpassthru_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Emits native or userspace-wrapper streaming for a loaded `fpassthru()` handle.
fn emit_fpassthru_dispatch(ctx: &mut FunctionContext<'_>) {
    let wrapper_label = ctx.next_label("fpt_wrapper");
    let loop_label = ctx.next_label("fpt_loop");
    let release_eof_label = ctx.next_label("fpt_release_eof");
    let wrapper_done_label = ctx.next_label("fpt_done");
    let done_label = ctx.next_label("fpt_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // stream wrapper handles through the userspace read loop
            abi::emit_call_label(ctx.emitter, "__rt_fpassthru");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the wrapper read loop after native streaming
            ctx.emitter.label(&wrapper_label);
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the synthetic wrapper fd
            ctx.emitter.instruction("str xzr, [sp, #8]");                       // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the wrapper fd for EOF probing
            abi::emit_call_label(ctx.emitter, "__rt_feof");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper_done_label)); // stop streaming when stream_eof reports EOF
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the wrapper fd for reading
            ctx.emitter.instruction("mov x1, #4096");                           // request a bounded wrapper read chunk
            abi::emit_call_label(ctx.emitter, "__rt_fread");
            ctx.emitter.instruction(&format!("cbz x2, {}", release_eof_label)); // stop defensively on empty wrapper reads
            ctx.emitter.instruction("str x1, [sp, #16]");                       // preserve the owned chunk pointer for release
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // load the current copied byte total
            ctx.emitter.instruction("add x9, x9, x2");                          // add this chunk's byte length
            ctx.emitter.instruction("str x9, [sp, #8]");                        // store the updated copied byte total
            ctx.emitter.instruction("mov x0, #1");                              // write wrapper bytes to stdout
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the owned chunk pointer
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("b {}", loop_label));              // continue draining the wrapper stream
            ctx.emitter.label(&release_eof_label);
            ctx.emitter.instruction("mov x0, x1");                              // pass the final empty chunk pointer to decref
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&wrapper_done_label);
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // return the copied byte total
            ctx.emitter.instruction("add sp, sp, #32");                         // release wrapper streaming scratch storage
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // stream wrapper handles through the userspace read loop
            ctx.emitter.instruction("mov rdi, rax");                            // pass the native fd to fpassthru
            abi::emit_call_label(ctx.emitter, "__rt_fpassthru");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the wrapper read loop after native streaming
            ctx.emitter.label(&wrapper_label);
            ctx.emitter.instruction("sub rsp, 32");                             // reserve fd, byte total, and chunk scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the synthetic wrapper fd
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], 0");              // initialize copied byte total to zero
            ctx.emitter.label(&loop_label);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the wrapper fd for EOF probing
            abi::emit_call_label(ctx.emitter, "__rt_feof");
            ctx.emitter.instruction("test rax, rax");                           // test whether stream_eof reported EOF
            ctx.emitter.instruction(&format!("jnz {}", wrapper_done_label));    // stop streaming when stream_eof reports EOF
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the wrapper fd for reading
            ctx.emitter.instruction("mov rsi, 4096");                           // request a bounded wrapper read chunk
            abi::emit_call_label(ctx.emitter, "__rt_fread");
            ctx.emitter.instruction("test rdx, rdx");                           // test whether the wrapper returned an empty chunk
            ctx.emitter.instruction(&format!("jz {}", release_eof_label));      // stop defensively on empty wrapper reads
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the owned chunk pointer for release
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 8]");             // load the current copied byte total
            ctx.emitter.instruction("add r8, rdx");                             // add this chunk's byte length
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r8");             // store the updated copied byte total
            ctx.emitter.instruction("mov rsi, rax");                            // pass the chunk pointer to write()
            ctx.emitter.instruction("mov edi, 1");                              // write wrapper bytes to stdout
            abi::emit_call_label(ctx.emitter, "write");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the owned chunk pointer
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.instruction(&format!("jmp {}", loop_label));            // continue draining the wrapper stream
            ctx.emitter.label(&release_eof_label);
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            ctx.emitter.label(&wrapper_done_label);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // return the copied byte total
            ctx.emitter.instruction("add rsp, 32");                             // release wrapper streaming scratch storage
            ctx.emitter.label(&done_label);
        }
    }
}

/// Lowers `feof(stream)` through the runtime EOF-flag table helper.
pub(super) fn lower_feof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "feof", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "feof")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream fd to the x86_64 feof runtime helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_feof");
    store_if_result(ctx, inst)
}

/// Lowers `ftell(stream)` as `lseek(fd, 0, SEEK_CUR)`.
pub(super) fn lower_ftell(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "ftell", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "ftell")?;
    let wrapper_label = ctx.next_label("ftell_user_wrapper");
    let after_dispatch_label = ctx.next_label("ftell_after_dispatch");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_tell
            ctx.emitter.instruction("mov x1, #0");                              // use offset 0 for the ftell lseek probe
            ctx.emitter.instruction("mov x2, #1");                              // use SEEK_CUR for the ftell lseek probe
            ctx.emitter.syscall(199);
            ctx.emitter.instruction(&format!("b {}", after_dispatch_label));    // skip wrapper stream_tell after the native probe
            ctx.emitter.label(&wrapper_label);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_ftell");
            ctx.emitter.label(&after_dispatch_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_tell
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream fd to libc lseek()
            ctx.emitter.instruction("xor esi, esi");                            // use offset 0 for the ftell lseek probe
            ctx.emitter.instruction("mov edx, 1");                              // use SEEK_CUR for the ftell lseek probe
            ctx.emitter.instruction("call lseek");                              // query the current stream position
            ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));  // skip wrapper stream_tell after the native probe
            ctx.emitter.label(&wrapper_label);
            ctx.emitter.instruction("mov rdi, rax");                            // pass the synthetic wrapper descriptor to the tell helper
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_ftell");
            ctx.emitter.label(&after_dispatch_label);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fseek(stream, offset, whence?)` and clears EOF state on success.
pub(super) fn lower_fseek(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fseek", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let offset = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "fseek")?;
    let success_label = ctx.next_label("fseek_success");
    let done_label = ctx.next_label("fseek_done");
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(offset)?.codegen_repr(), "fseek offset")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    if inst.operands.len() == 3 {
        let whence = expect_operand(inst, 2)?;
        require_int(ctx.load_value_to_result(whence)?.codegen_repr(), "fseek whence")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_fseek_aarch64(ctx, &success_label, &done_label),
        Arch::X86_64 => lower_fseek_x86_64(ctx, &success_label, &done_label),
    }
    store_if_result(ctx, inst)
}

/// Lowers `rewind(stream)` as `lseek(fd, 0, SEEK_SET)` and clears EOF state on success.
pub(super) fn lower_rewind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "rewind", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "rewind")?;
    let success_label = ctx.next_label("rewind_success");
    let done_label = ctx.next_label("rewind_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_rewind_aarch64(ctx, &success_label, &done_label),
        Arch::X86_64 => lower_rewind_x86_64(ctx, &success_label, &done_label),
    }
    store_if_result(ctx, inst)
}

/// Lowers `ftruncate(stream, size)` through the shared fd truncate runtime helper.
pub(super) fn lower_ftruncate(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "ftruncate", 2)?;
    let stream = expect_operand(inst, 0)?;
    let size = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "ftruncate")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(size)?.codegen_repr(), "ftruncate size")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the target file size to the ftruncate runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the target file size to the ftruncate runtime helper
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    let wrapper_label = ctx.next_label("ftruncate_user_wrapper");
    let done_label = ctx.next_label("ftruncate_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_truncate
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_truncate
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ftruncate");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper truncation after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper truncation after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the truncate helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_ftruncate");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `fsync(stream)` through the shared fd sync runtime helper.
pub(super) fn lower_fsync(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_stream_bool_runtime(ctx, inst, "fsync", "__rt_fsync")
}

/// Lowers `fflush(stream)` through the shared fd flush runtime helper.
pub(super) fn lower_fflush(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fflush", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fflush")?;
    let wrapper_label = ctx.next_label("fflush_user_wrapper");
    let done_label = ctx.next_label("fflush_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_flush
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_flush
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fflush");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper flushing after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper flushing after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the flush helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fflush");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `fdatasync(stream)` through the shared fd data-sync runtime helper.
pub(super) fn lower_fdatasync(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_stream_bool_runtime(ctx, inst, "fdatasync", "__rt_fdatasync")
}

/// Lowers `flock(stream, operation, would_block?)` through the libc flock wrapper.
pub(super) fn lower_flock(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "flock", 2, 3)?;
    let stream = expect_operand(inst, 0)?;
    let operation = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "flock")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    require_int(ctx.load_value_to_result(operation)?.codegen_repr(), "flock operation")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the lock operation to the flock runtime helper
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // pass the lock operation to the flock runtime helper
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    let wrapper_label = ctx.next_label("flock_user_wrapper");
    let done_label = ctx.next_label("flock_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_lock
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_lock
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_flock");
    if inst.operands.len() == 3 {
        let would_block = expect_operand(inst, 2)?;
        let Some(slot) = source_load_local_slot(ctx, would_block)? else {
            return Err(CodegenIrError::unsupported(
                "flock would_block output for non-local arguments",
            ));
        };
        store_flock_would_block(ctx, slot)?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper locking after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper locking after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the lock helper
        ctx.emitter.instruction("mov rsi, rdx");                                // pass the lock operation to the wrapper method
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_flock");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `disk_free_space(path)` through the shared disk-space runtime helper.
pub(super) fn lower_disk_free_space(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_disk_space(ctx, inst, "disk_free_space", 0)
}

/// Lowers `disk_total_space(path)` through the shared disk-space runtime helper.
pub(super) fn lower_disk_total_space(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_disk_space(ctx, inst, "disk_total_space", 1)
}

/// Loads a path and disk-space mode into `__rt_disk_space`.
fn lower_disk_space(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    mode: i64,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", mode);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the path pointer as the second disk-space argument
            abi::emit_load_int_immediate(ctx.emitter, "rdi", mode);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_disk_space");
    store_if_result(ctx, inst)
}

/// Lowers `gethostname()` through the shared runtime helper.
pub(super) fn lower_gethostname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "gethostname", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_gethostname");
    store_if_result(ctx, inst)
}

/// Lowers `gethostbyname(hostname)` through the shared runtime resolver.
pub(super) fn lower_gethostbyname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "gethostbyname", 1)?;
    let host = expect_operand(inst, 0)?;
    load_string_to_result(ctx, host, "gethostbyname host")?;
    abi::emit_call_label(ctx.emitter, "__rt_gethostbyname");
    store_if_result(ctx, inst)
}

/// Lowers `gethostbyaddr(address)` and boxes malformed addresses as PHP `false`.
pub(super) fn lower_gethostbyaddr(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "gethostbyaddr", 1)?;
    let address = expect_operand(inst, 0)?;
    load_string_to_result(ctx, address, "gethostbyaddr address")?;
    abi::emit_call_label(ctx.emitter, "__rt_gethostbyaddr");
    box_owned_string_or_false_result(ctx, "gethostbyaddr");
    store_if_result(ctx, inst)
}

/// Lowers `getprotobyname(protocol)` and boxes a missing entry as PHP `false`.
pub(super) fn lower_getprotobyname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getprotobyname", 1)?;
    let protocol = expect_operand(inst, 0)?;
    load_string_to_result(ctx, protocol, "getprotobyname protocol")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass the protocol byte length as the second runtime argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the protocol pointer as the first runtime argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the protocol byte length as the second runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_getprotobyname");
    box_negative_int_or_false_result(ctx, "getprotobyname");
    store_if_result(ctx, inst)
}

/// Lowers `getprotobynumber(number)` and boxes a missing entry as PHP `false`.
pub(super) fn lower_getprotobynumber(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getprotobynumber", 1)?;
    let protocol = expect_operand(inst, 0)?;
    require_int(
        ctx.load_value_to_result(protocol)?.codegen_repr(),
        "getprotobynumber number",
    )?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the protocol number as the runtime argument
    }
    abi::emit_call_label(ctx.emitter, "__rt_getprotobynumber");
    box_owned_string_or_false_result(ctx, "getprotobynumber");
    store_if_result(ctx, inst)
}

/// Lowers `getservbyname(service, protocol)` and boxes a missing entry as PHP `false`.
pub(super) fn lower_getservbyname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getservbyname", 2)?;
    let service = expect_operand(inst, 0)?;
    let protocol = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, service, "getservbyname service")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, protocol, "getservbyname protocol")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the protocol pointer as the third runtime argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the protocol byte length as the fourth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, service, "getservbyname service")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, protocol, "getservbyname protocol")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the protocol byte length as the fourth runtime argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the protocol pointer as the third runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_getservbyname");
    box_negative_int_or_false_result(ctx, "getservbyname");
    store_if_result(ctx, inst)
}

/// Lowers `getservbyport(port, protocol)` and boxes a missing entry as PHP `false`.
pub(super) fn lower_getservbyport(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getservbyport", 2)?;
    let port = expect_operand(inst, 0)?;
    let protocol = expect_operand(inst, 1)?;
    require_int(
        ctx.load_value_to_result(port)?.codegen_repr(),
        "getservbyport port",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x0", "x0");
            load_string_to_result(ctx, protocol, "getservbyport protocol")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x9");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rax");
            load_string_to_result(ctx, protocol, "getservbyport protocol")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass the protocol pointer as the second runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_getservbyport");
    box_owned_string_or_false_result(ctx, "getservbyport");
    store_if_result(ctx, inst)
}

/// Lowers `opendir(path)` and boxes the directory stream as `resource|false`.
pub(super) fn lower_opendir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "opendir", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "opendir path")?;
    abi::emit_call_label(ctx.emitter, "__rt_opendir");
    box_stream_fd_or_false_result_kind(ctx, "opendir", 4);
    store_if_result(ctx, inst)
}

/// Lowers `readdir(dir_handle)` for libc, glob, and userspace-wrapper handles.
pub(super) fn lower_readdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "readdir", 1)?;
    let handle = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, handle, "readdir")?;
    lower_directory_handle_dispatch(
        ctx,
        "__rt_readdir",
        "__rt_user_wrapper_dir_readdir",
        "readdir",
    );
    box_owned_string_or_false_result(ctx, "readdir");
    store_if_result(ctx, inst)
}

/// Lowers `closedir(dir_handle)` for libc, glob, and userspace-wrapper handles.
pub(super) fn lower_closedir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "closedir", 1)?;
    let handle = expect_operand(inst, 0)?;
    let captured = capture_resource_box_for_release(ctx, handle)?;
    load_stream_fd_to_result(ctx, handle, "closedir")?;
    apply_resource_release_sentinel(ctx, captured);
    lower_directory_handle_dispatch(
        ctx,
        "__rt_closedir",
        "__rt_user_wrapper_dir_closedir",
        "closedir",
    );
    store_if_result(ctx, inst)
}

/// Lowers `rewinddir(dir_handle)` for libc, glob, and userspace-wrapper handles.
pub(super) fn lower_rewinddir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "rewinddir", 1)?;
    let handle = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, handle, "rewinddir")?;
    lower_directory_handle_dispatch(
        ctx,
        "__rt_rewinddir",
        "__rt_user_wrapper_dir_rewinddir",
        "rewinddir",
    );
    store_if_result(ctx, inst)
}

/// Lowers `popen(command, mode)` and boxes the process pipe as `resource|false`.
pub(super) fn lower_popen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "popen", 2)?;
    let command = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, command, "popen command")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, mode, "popen mode")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the mode pointer as the third runtime argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the mode byte length as the fourth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, command, "popen command")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, mode, "popen mode")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the mode byte length as the fourth runtime argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the mode pointer as the third runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_popen");
    box_stream_fd_or_false_result_kind(ctx, "popen", 3);
    store_if_result(ctx, inst)
}

/// Lowers `pclose(handle)` and returns the child process status.
pub(super) fn lower_pclose(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "pclose", 1)?;
    let handle = expect_operand(inst, 0)?;
    let captured = capture_resource_box_for_release(ctx, handle)?;
    load_stream_fd_to_result(ctx, handle, "pclose")?;
    apply_resource_release_sentinel(ctx, captured);
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the pipe descriptor to the runtime close helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_pclose");
    store_if_result(ctx, inst)
}

/// Lowers `fsockopen(host, port, errno?, errstr?, timeout?)`.
pub(super) fn lower_fsockopen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fsockopen", 2, 5)?;
    let host = expect_operand(inst, 0)?;
    let port = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, host, "fsockopen host")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(port)?.codegen_repr(), "fsockopen port")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            if inst.operands.len() >= 5 {
                let timeout = expect_operand(inst, 4)?;
                ctx.load_value_to_result(timeout)?;
            }
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, x1");                              // pass hostname pointer as the first runtime argument
            ctx.emitter.instruction("mov x1, x2");                              // pass hostname byte length as the second runtime argument
            ctx.emitter.instruction("mov x2, x9");                              // pass TCP port as the third runtime argument
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, host, "fsockopen host")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(port)?.codegen_repr(), "fsockopen port")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            if inst.operands.len() >= 5 {
                let timeout = expect_operand(inst, 4)?;
                ctx.load_value_to_result(timeout)?;
            }
            abi::emit_pop_reg(ctx.emitter, "r8");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            ctx.emitter.instruction("mov rdx, r8");                             // pass TCP port as the third runtime argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fsockopen");
    store_fsockopen_error_outputs(ctx, inst)?;
    box_stream_fd_or_false_result(ctx, "fsockopen");
    store_if_result(ctx, inst)
}

/// Lowers `file(path)` through the target-aware runtime line-array helper.
pub(super) fn lower_file(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_array(ctx, inst, "file", "__rt_file")
}

/// Lowers `realpath(path)` and boxes the owned runtime string-or-false result.
pub(super) fn lower_realpath(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "realpath", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "realpath")?;
    abi::emit_call_label(ctx.emitter, "__rt_realpath");
    box_owned_string_or_false_result(ctx, "realpath");
    store_if_result(ctx, inst)
}

/// Lowers `realpath_cache_get()` to elephc's empty realpath-cache view.
pub(super) fn lower_realpath_cache_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "realpath_cache_get", 0)?;
    emit_empty_mixed_hash(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `realpath_cache_size()` to zero because elephc has no realpath cache.
pub(super) fn lower_realpath_cache_size(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "realpath_cache_size", 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                              // report an empty realpath cache size
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor rax, rax");                            // report an empty realpath cache size
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `file_put_contents(path, data)` through the target-aware runtime writer.
pub(super) fn lower_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "file_put_contents", 2)?;
    let path = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    let path_literal = optional_const_string_operand(ctx, path)?;
    if let Some(path_literal) = path_literal.as_deref() {
        if path_literal.starts_with("phar://") {
            return lower_literal_phar_file_put_contents(ctx, inst, path_literal, data);
        }
    }
    let helper = if path_literal.is_none() {
        publish_dynamic_phar_write_function_pointer(ctx);
        "__rt_file_put_contents_maybe_phar"
    } else {
        "__rt_file_put_contents"
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_file_put_contents_arm64(ctx, path, data, helper)?,
        Arch::X86_64 => lower_file_put_contents_x86_64(ctx, path, data, helper)?,
    }
    store_if_result(ctx, inst)
}

/// Lowers one-shot `file_put_contents("phar://archive/entry", data)`.
fn lower_literal_phar_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
    data: ValueId,
) -> Result<()> {
    if !emit_phar_write_open_for_literal(ctx, path)? {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #-1");                         // unresolved phar write target returns failure
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, -1");                         // unresolved phar write target returns failure
            }
        }
        return store_if_result(ctx, inst);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, data, "file_put_contents phar data")?;
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_append");
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, x9");                              // pass the PHAR write descriptor to finalize
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_finalize");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, data, "file_put_contents phar data")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass the entry payload pointer to the phar writer
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_append");
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_phar_write_finalize");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the compiler-internal native PHAR compression-control helper.
pub(super) fn lower_elephc_phar_set_compression(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_phar_set_compression", 2)?;
    let path = expect_operand(inst, 0)?;
    let compression = expect_operand(inst, 1)?;
    let fail = ctx.next_label("phar_set_compression_fail");
    let done = ctx.next_label("phar_set_compression_done");
    publish_phar_set_compression_function_pointer(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_result(compression)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_string_to_result(ctx, path, "__elephc_phar_set_compression path")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_pop_reg(ctx.emitter, "x2");
            abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_phar_set_compression_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR compression bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));              // missing bridge makes compression control fail
            ctx.emitter.instruction("blr x9");                                  // rewrite native-PHAR entry compression flags
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge success flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize bridge result to PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.load_value_to_result(compression)?;
            abi::emit_push_reg(ctx.emitter, "rax");
            load_string_to_result(ctx, path, "__elephc_phar_set_compression path")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_pop_reg(ctx.emitter, "rdx");
            abi::emit_load_symbol_to_reg(
                ctx.emitter,
                "r10",
                "_elephc_phar_set_compression_fn",
                0,
            );
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR compression bridge was published
            ctx.emitter.instruction(&format!("jz {}", fail));                   // missing bridge makes compression control fail
            ctx.emitter.instruction("call r10");                                // rewrite native-PHAR entry compression flags
            ctx.emitter.instruction("test rax, rax");                           // test the bridge success flag
            ctx.emitter.instruction("setne al");                                // normalize bridge result to PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_phar_get_metadata()` into the metadata-read bridge call.
pub(super) fn lower_elephc_phar_get_metadata(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_metadata_function_pointer(ctx);
    emit_phar_get_string_bridge(
        ctx,
        inst,
        "__elephc_phar_get_metadata",
        "_elephc_phar_get_metadata_fn",
    )
}

/// Lowers `__elephc_phar_get_stub()` into the stub-read bridge call.
pub(super) fn lower_elephc_phar_get_stub(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_stub_function_pointer(ctx);
    emit_phar_get_string_bridge(ctx, inst, "__elephc_phar_get_stub", "_elephc_phar_get_stub_fn")
}

/// Lowers `__elephc_phar_set_metadata()` into the metadata-write bridge call.
pub(super) fn lower_elephc_phar_set_metadata(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_set_metadata_function_pointer(ctx);
    emit_phar_set_string_bridge(
        ctx,
        inst,
        "__elephc_phar_set_metadata",
        "_elephc_phar_set_metadata_fn",
    )
}

/// Lowers `__elephc_phar_set_stub()` into the stub-write bridge call.
pub(super) fn lower_elephc_phar_set_stub(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_set_stub_function_pointer(ctx);
    emit_phar_set_string_bridge(ctx, inst, "__elephc_phar_set_stub", "_elephc_phar_set_stub_fn")
}

/// Emits a `(path, data)` string -> bool PHAR bridge call (set metadata/stub).
///
/// Loads the path and data strings into the bridge's `(path_ptr, path_len, data_ptr,
/// data_len)` argument registers, calls the optional bridge pointer in `slot`, and
/// normalizes the result to a PHP bool (false when the bridge is unavailable).
fn emit_phar_set_string_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    let fail = ctx.next_label("phar_set_string_fail");
    let done = ctx.next_label("phar_set_string_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, data, "phar set-string data")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, path, "phar set-string path")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_symbol_address(ctx.emitter, "x9", slot);
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR write bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));              // missing bridge makes the write fail
            ctx.emitter.instruction("blr x9");                                  // rewrite the archive with the new metadata/stub
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge success flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize bridge result to PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, data, "phar set-string data")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, path, "phar set-string path")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", slot, 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR write bridge was published
            ctx.emitter.instruction(&format!("jz {}", fail));                   // missing bridge makes the write fail
            ctx.emitter.instruction("call r10");                                // rewrite the archive with the new metadata/stub
            ctx.emitter.instruction("test rax, rax");                           // test the bridge success flag
            ctx.emitter.instruction("setne al");                                // normalize bridge result to PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Emits a `(string) -> bool` PHAR bridge call (e.g. set the ZipCrypto password).
///
/// Loads the single string argument as (pointer, length), calls the optional bridge
/// pointer in `slot`, and normalizes its return to a PHP bool. A null bridge yields
/// false.
fn emit_phar_string_to_bool_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    let fail = ctx.next_label("phar_string_bool_fail");
    let done = ctx.next_label("phar_string_bool_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, value, "phar string->bool arg")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = string pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = string length
            abi::emit_symbol_address(ctx.emitter, "x9", slot);
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));              // missing bridge yields false
            ctx.emitter.instruction("blr x9");                                  // call the bridge setter
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge return flag
            ctx.emitter.instruction("cset x0, ne");                             // normalize to a PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("mov x0, #0");                              // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, value, "phar string->bool arg")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = string pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = string length
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", slot, 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the bridge was published
            ctx.emitter.instruction(&format!("jz {}", fail));                   // missing bridge yields false
            ctx.emitter.instruction("call r10");                                // call the bridge setter
            ctx.emitter.instruction("test rax, rax");                           // test the bridge return flag
            ctx.emitter.instruction("setne al");                                // normalize to a PHP bool
            ctx.emitter.instruction("movzx eax, al");                           // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                            // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Emits a `(path) -> string` PHAR bridge call (read metadata/stub).
///
/// Calls the optional bridge pointer in `slot` with the path and an out-length slot,
/// then persists the returned bytes into an owned PHP string. A null bridge or a null
/// result yields an owned empty string (the OOP layer treats that as "not set").
fn emit_phar_get_string_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    let empty = ctx.next_label("phar_get_string_empty");
    let persist = ctx.next_label("phar_get_string_persist");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, path, "phar get-string path")?;
            ctx.emitter.instruction("mov x0, x1");                              // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov x1, x2");                              // bridge arg 1 = archive path length
            abi::emit_symbol_address(ctx.emitter, "x2", "_phar_list_len");      // bridge arg 2 = out-length slot
            abi::emit_symbol_address(ctx.emitter, "x9", slot);
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the optional PHAR read bridge pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", empty));             // missing bridge yields an empty string
            ctx.emitter.instruction("blr x9");                                  // read the metadata/stub bytes into the global buffer
            ctx.emitter.instruction(&format!("cbz x0, {}", empty));             // a null result means the field is unset
            ctx.emitter.instruction("mov x1, x0");                              // str_persist source pointer = bridge buffer
            abi::emit_symbol_address(ctx.emitter, "x9", "_phar_list_len");
            ctx.emitter.instruction("ldr x2, [x9]");                            // str_persist length = bridge out-length
            ctx.emitter.instruction(&format!("b {}", persist));                 // persist the returned bytes
            ctx.emitter.label(&empty);
            ctx.emitter.instruction("mov x1, #0");                              // empty source pointer (length 0 is not dereferenced)
            ctx.emitter.instruction("mov x2, #0");                              // empty string length
            ctx.emitter.label(&persist);
            ctx.emitter.instruction("bl __rt_str_persist");                     // copy into an owned heap string -> x1=ptr, x2=len
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, path, "phar get-string path")?;
            ctx.emitter.instruction("mov rdi, rax");                            // bridge arg 0 = archive path pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // bridge arg 1 = archive path length
            abi::emit_symbol_address(ctx.emitter, "rdx", "_phar_list_len");     // bridge arg 2 = out-length slot
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", slot, 0);
            ctx.emitter.instruction("test r10, r10");                           // test whether the PHAR read bridge was published
            ctx.emitter.instruction(&format!("jz {}", empty));                  // missing bridge yields an empty string
            ctx.emitter.instruction("call r10");                                // read the metadata/stub bytes into the global buffer
            ctx.emitter.instruction("test rax, rax");                           // a null result means the field is unset
            ctx.emitter.instruction(&format!("jz {}", empty));                  // fall back to an empty string
            ctx.emitter.instruction("mov rdi, rax");                            // str_persist source pointer = bridge buffer
            abi::emit_load_symbol_to_reg(ctx.emitter, "rdx", "_phar_list_len", 0); // str_persist length = bridge out-length
            ctx.emitter.instruction(&format!("jmp {}", persist));              // persist the returned bytes
            ctx.emitter.label(&empty);
            ctx.emitter.instruction("mov rdi, 0");                              // empty source pointer (length 0 is not dereferenced)
            ctx.emitter.instruction("mov rdx, 0");                              // empty string length
            ctx.emitter.label(&persist);
            ctx.emitter.instruction("call __rt_str_persist");                  // copy into an owned heap string -> rax=ptr, rdx=len
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_phar_get_file_metadata()` into the per-file metadata-read bridge.
pub(super) fn lower_elephc_phar_get_file_metadata(
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

/// Lowers `__elephc_phar_set_file_metadata()` into the per-file metadata-write bridge.
/// The single `phar://archive/entry` URL argument is split by the bridge, so this
/// reuses the same `(url, data) -> bool` shape as the archive-level metadata writer.
pub(super) fn lower_elephc_phar_set_file_metadata(
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
pub(super) fn lower_elephc_phar_gzip_archive(
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
pub(super) fn lower_elephc_phar_bzip2_archive(
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
pub(super) fn lower_elephc_phar_decompress_archive(
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
pub(super) fn lower_elephc_phar_sign_openssl(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_sign_openssl_function_pointer(ctx);
    emit_phar_set_string_bridge(
        ctx,
        inst,
        "__elephc_phar_sign_openssl",
        "_elephc_phar_sign_openssl_fn",
    )
}

/// Lowers `__elephc_phar_sign_hash(path, algo)` into the hash-based signing bridge.
pub(super) fn lower_elephc_phar_sign_hash(
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
pub(super) fn lower_elephc_phar_set_zip_password(
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
pub(super) fn lower_elephc_phar_get_signature_hash(
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
pub(super) fn lower_elephc_phar_get_signature_type(
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
fn emit_phar_path_int_to_bool_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
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
            ctx.emitter.instruction(&format!("cbz x9, {}", fail));             // missing bridge makes the op fail
            ctx.emitter.instruction("blr x9");                                  // invoke the bridge
            ctx.emitter.instruction("cmp x0, #0");                              // test the bridge success flag
            ctx.emitter.instruction("cset x0, ne");                            // normalize to PHP bool
            ctx.emitter.instruction(&format!("b {}", done));                   // skip the failure result
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
            ctx.emitter.instruction(&format!("jz {}", fail));                  // missing bridge makes the op fail
            ctx.emitter.instruction("call r10");                               // invoke the bridge
            ctx.emitter.instruction("test rax, rax");                          // test the bridge success flag
            ctx.emitter.instruction("setne al");                               // normalize to PHP bool
            ctx.emitter.instruction("movzx eax, al");                          // widen the normalized bool
            ctx.emitter.instruction(&format!("jmp {}", done));                 // skip the failure result
            ctx.emitter.label(&fail);
            ctx.emitter.instruction("xor eax, eax");                           // report false when the bridge is unavailable
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers the compiler-internal PHAR entry-list helper into a PHP string array.
pub(super) fn lower_elephc_phar_list_entries(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_phar_list_entries", 1)?;
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
fn emit_phar_list_entries_buffer_to_array_aarch64(ctx: &mut FunctionContext<'_>) {
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
fn emit_phar_list_entries_buffer_to_array_x86_64(ctx: &mut FunctionContext<'_>) {
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

/// Lowers `file_exists(path)` through the target-aware runtime stat helper.
pub(super) fn lower_file_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_file_exists_with_wrapper(ctx, inst)
}

/// Lowers `unlink(path)` through the target-aware runtime helper.
pub(super) fn lower_unlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "unlink", 1)?;
    let path = expect_operand(inst, 0)?;
    let path_literal = optional_const_string_operand(ctx, path)?;
    let can_be_phar = path_literal
        .as_deref()
        .map(|path| path.starts_with("phar://"))
        .unwrap_or(true);
    if can_be_phar {
        publish_phar_delete_function_pointer(ctx);
    }
    load_string_to_result(ctx, path, "unlink")?;
    if can_be_phar {
        emit_unlink_maybe_phar_dispatch(ctx);
    } else {
        emit_single_path_wrapper_dispatch(ctx, "__rt_unlink", STREAM_WRAPPER_UNLINK_SLOT);
    }
    store_if_result(ctx, inst)
}

/// Lowers `mkdir(path)` through the target-aware runtime helper.
pub(super) fn lower_mkdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_single_path_wrapper_op(ctx, inst, "mkdir", "__rt_mkdir", STREAM_WRAPPER_MKDIR_SLOT)
}

/// Lowers `rmdir(path)` through the target-aware runtime helper.
pub(super) fn lower_rmdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_single_path_wrapper_op(ctx, inst, "rmdir", "__rt_rmdir", STREAM_WRAPPER_RMDIR_SLOT)
}

/// Lowers `chdir(path)` through the target-aware runtime helper.
pub(super) fn lower_chdir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "chdir", "__rt_chdir")
}

/// Lowers `copy(source, dest)` through the target-aware runtime helper.
pub(super) fn lower_copy(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "copy", "__rt_copy")
}

/// Lowers `rename(from, to)` through the target-aware runtime helper.
pub(super) fn lower_rename(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_rename_with_wrapper(ctx, inst)
}

/// Lowers `tempnam(directory, prefix)` through the target-aware runtime helper.
pub(super) fn lower_tempnam(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "tempnam", "__rt_tempnam")
}

/// Lowers `scandir(path)` through the target-aware runtime directory listing helper.
pub(super) fn lower_scandir(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_array(ctx, inst, "scandir", "__rt_scandir")
}

/// Lowers `glob(pattern)` through the target-aware runtime glob expansion helper.
pub(super) fn lower_glob(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_array(ctx, inst, "glob", "__rt_glob")
}

/// Lowers `chmod(path, mode)` through the target-aware runtime helper.
pub(super) fn lower_chmod(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chmod_with_wrapper(ctx, inst)
}

/// Lowers `chown(path, owner)` for integer UIDs and string user names.
pub(super) fn lower_chown(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chown_or_chgrp(ctx, inst, "chown", PrincipalKind::Owner)
}

/// Lowers `chgrp(path, group)` for integer GIDs and string group names.
pub(super) fn lower_chgrp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_chown_or_chgrp(ctx, inst, "chgrp", PrincipalKind::Group)
}

/// Lowers `lchown(path, owner)` for integer UIDs and string user names without following symlinks.
pub(super) fn lower_lchown(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_lchown_or_lchgrp(ctx, inst, "lchown", PrincipalKind::Owner)
}

/// Lowers `lchgrp(path, group)` for integer GIDs and string group names without following symlinks.
pub(super) fn lower_lchgrp(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_lchown_or_lchgrp(ctx, inst, "lchgrp", PrincipalKind::Group)
}

/// Lowers `umask(mask?)` through the target-aware runtime helper.
pub(super) fn lower_umask(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "umask", 0, 1)?;
    if inst.operands.is_empty() {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x0, #0");                          // probe the current umask with a temporary zero mask
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("stp x0, xzr, [sp, #-16]!");            // save the probed previous mask while restoring it
                ctx.emitter.instruction("ldr x0, [sp]");                        // pass the previous mask back to restore process state
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("ldp x0, xzr, [sp], #16");              // return the originally probed mask to PHP
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor eax, eax");                        // probe the current umask with a temporary zero mask
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("push rax");                            // save the probed previous mask while restoring it
                ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");            // pass the previous mask back to restore process state
                abi::emit_call_label(ctx.emitter, "__rt_umask");
                ctx.emitter.instruction("pop rax");                             // return the originally probed mask to PHP
            }
        }
        return store_if_result(ctx, inst);
    }
    let mask = expect_operand(inst, 0)?;
    require_int(ctx.load_value_to_result(mask)?.codegen_repr(), "umask mask")?;
    abi::emit_call_label(ctx.emitter, "__rt_umask");
    store_if_result(ctx, inst)
}

/// Lowers `touch(path, mtime?, atime?)` through the target-aware runtime helper.
pub(super) fn lower_touch(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "touch", 1, 3)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "touch path")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_touch_args_aarch64(ctx, inst)?,
        Arch::X86_64 => lower_touch_args_x86_64(ctx, inst)?,
    }
    emit_touch_wrapper_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `basename(path, suffix?)` through the target-aware runtime helper.
pub(super) fn lower_basename(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "basename", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "basename path")?;
    if inst.operands.len() == 2 {
        let suffix = expect_operand(inst, 1)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
                load_string_to_result(ctx, suffix, "basename suffix")?;
                ctx.emitter.instruction("mov x3, x1");                          // pass the suffix pointer in the runtime helper's secondary string slot
                ctx.emitter.instruction("mov x4, x2");                          // pass the suffix length in the runtime helper's secondary string slot
                abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
                load_string_to_result(ctx, suffix, "basename suffix")?;
                ctx.emitter.instruction("mov rdi, rax");                        // pass the suffix pointer while the path remains on the stack
                ctx.emitter.instruction("mov rsi, rdx");                        // pass the suffix length while the path remains on the stack
                abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            }
        }
    } else {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x3, #0");                          // signal that no suffix pointer was supplied
                ctx.emitter.instruction("mov x4, #0");                          // signal that no suffix length was supplied
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xor edi, edi");                        // signal that no suffix pointer was supplied
                ctx.emitter.instruction("xor esi, esi");                        // signal that no suffix length was supplied
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_basename");
    store_if_result(ctx, inst)
}

/// Lowers `dirname(path, levels?)` through the target-aware runtime helper.
pub(super) fn lower_dirname(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "dirname", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "dirname path")?;
    if inst.operands.len() == 1 {
        abi::emit_call_label(ctx.emitter, "__rt_dirname");
        return store_if_result(ctx, inst);
    }
    let levels = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(levels)?.codegen_repr(), "dirname levels")?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the requested parent depth to the levels-aware runtime helper
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(levels)?.codegen_repr(), "dirname levels")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the requested parent depth to the levels-aware runtime helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_dirname_levels");
    store_if_result(ctx, inst)
}

/// Lowers `fnmatch(pattern, filename, flags?)` through the target-aware runtime helper.
pub(super) fn lower_fnmatch(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fnmatch", 2, 3)?;
    let pattern = expect_operand(inst, 0)?;
    let filename = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, pattern, "fnmatch pattern")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, filename, "fnmatch filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            if inst.operands.len() == 3 {
                let flags = expect_operand(inst, 2)?;
                require_int(ctx.load_value_to_result(flags)?.codegen_repr(), "fnmatch flags")?;
                ctx.emitter.instruction("mov x5, x0");                          // pass the caller-supplied fnmatch flags to the runtime helper
            } else {
                ctx.emitter.instruction("mov x5, #0");                          // use the PHP default flags value
            }
            abi::emit_pop_reg_pair(ctx.emitter, "x3", "x4");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, pattern, "fnmatch pattern")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, filename, "fnmatch filename")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            if inst.operands.len() == 3 {
                let flags = expect_operand(inst, 2)?;
                require_int(ctx.load_value_to_result(flags)?.codegen_repr(), "fnmatch flags")?;
                ctx.emitter.instruction("mov rcx, rax");                        // pass the caller-supplied fnmatch flags to the runtime helper
            } else {
                ctx.emitter.instruction("xor ecx, ecx");                        // use the PHP default flags value
            }
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fnmatch");
    store_if_result(ctx, inst)
}

/// Lowers `pathinfo(path, flags?)` through string, array, or boxed dynamic helpers.
pub(super) fn lower_pathinfo(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pathinfo", 1, 2)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "pathinfo path")?;
    let result_ty = inst.result_php_type.codegen_repr();
    if inst.operands.len() == 1 {
        abi::emit_call_label(ctx.emitter, "__rt_pathinfo_array");
        if result_ty == PhpType::Mixed {
            box_owned_pathinfo_array_as_mixed(ctx);
        }
        return store_if_result(ctx, inst);
    }
    let flag = expect_operand(inst, 1)?;
    match result_ty {
        PhpType::AssocArray { .. } => {
            abi::emit_call_label(ctx.emitter, "__rt_pathinfo_array");
        }
        PhpType::Str => {
            lower_pathinfo_string(ctx, flag)?;
        }
        PhpType::Mixed => {
            lower_pathinfo_mixed(ctx, flag)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "pathinfo result PHP type {:?}",
                other
            )));
        }
    }
    store_if_result(ctx, inst)
}

/// Selects which ownership field a filesystem principal builtin changes.
#[derive(Clone, Copy)]
enum PrincipalKind {
    Owner,
    Group,
}

/// Selects how `touch()` should materialize optional timestamp operands.
enum TouchTimeShape {
    BothNow,
    MtimeAlsoAtime,
    ExplicitBoth,
}

/// Lowers the shared path/principal calling convention for `chown()` and `chgrp()`.
fn lower_chown_or_chgrp(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let principal = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_chown_or_chgrp_aarch64(ctx, path, principal, name, kind)?,
        Arch::X86_64 => lower_chown_or_chgrp_x86_64(ctx, path, principal, name, kind)?,
    }
    store_if_result(ctx, inst)
}

/// Materializes `chown()`/`chgrp()` operands for the ARM64 runtime ABI.
fn lower_chown_or_chgrp_aarch64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            emit_owner_group_name_wrapper_dispatch(
                ctx,
                principal_name_option(kind),
                principal_string_runtime(kind),
            );
        }
        PhpType::Int => {
            emit_owner_group_wrapper_dispatch(ctx, principal_int_option(kind));
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Materializes `chown()`/`chgrp()` operands for the Linux x86_64 runtime ABI.
fn lower_chown_or_chgrp_x86_64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            emit_owner_group_name_wrapper_dispatch(
                ctx,
                principal_name_option(kind),
                principal_string_runtime(kind),
            );
        }
        PhpType::Int => {
            emit_owner_group_wrapper_dispatch(ctx, principal_int_option(kind));
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Lowers the native symlink-aware path/principal convention for `lchown()` and `lchgrp()`.
fn lower_lchown_or_lchgrp(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let path = expect_operand(inst, 0)?;
    let principal = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_lchown_or_lchgrp_aarch64(ctx, path, principal, name, kind)?,
        Arch::X86_64 => lower_lchown_or_lchgrp_x86_64(ctx, path, principal, name, kind)?,
    }
    store_if_result(ctx, inst)
}

/// Materializes `lchown()`/`lchgrp()` operands for the ARM64 runtime ABI.
fn lower_lchown_or_lchgrp_aarch64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            ctx.emitter.instruction("mov x3, x1");                              // pass principal name pointer to symlink ownership helper
            ctx.emitter.instruction("mov x4, x2");                              // pass principal name length to symlink ownership helper
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_call_label(ctx.emitter, lprincipal_string_runtime(kind));
        }
        PhpType::Int => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve uid/gid while restoring the path
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            if matches!(kind, PrincipalKind::Owner) {
                ctx.emitter.instruction("mov x3, x9");                          // pass uid and leave symlink group unchanged
                ctx.emitter.instruction("mov x4, #-1");                         // keep the symlink group unchanged
            } else {
                ctx.emitter.instruction("mov x3, #-1");                         // keep the symlink owner unchanged
                ctx.emitter.instruction("mov x4, x9");                          // pass gid and leave symlink owner unchanged
            }
            abi::emit_call_label(ctx.emitter, "__rt_lchown");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Materializes `lchown()`/`lchgrp()` operands for the Linux x86_64 runtime ABI.
fn lower_lchown_or_lchgrp_x86_64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    principal: ValueId,
    name: &str,
    kind: PrincipalKind,
) -> Result<()> {
    load_string_to_result(ctx, path, name)?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    match ctx.load_value_to_result(principal)?.codegen_repr() {
        PhpType::Str => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass principal name pointer to symlink ownership helper
            ctx.emitter.instruction("mov rsi, rdx");                            // pass principal name length to symlink ownership helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_call_label(ctx.emitter, lprincipal_string_runtime(kind));
        }
        PhpType::Int => {
            ctx.emitter.instruction("mov r9, rax");                             // preserve uid/gid while restoring the path
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            if matches!(kind, PrincipalKind::Owner) {
                ctx.emitter.instruction("mov rdi, r9");                         // pass uid and leave symlink group unchanged
                ctx.emitter.instruction("mov rsi, -1");                         // keep the symlink group unchanged
            } else {
                ctx.emitter.instruction("mov rdi, -1");                         // keep the symlink owner unchanged
                ctx.emitter.instruction("mov rsi, r9");                         // pass gid and leave symlink owner unchanged
            }
            abi::emit_call_label(ctx.emitter, "__rt_lchown");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} principal PHP type {:?}",
                name, other
            )));
        }
    }
    Ok(())
}

/// Returns the wrapper metadata option for string ownership changes.
fn principal_name_option(kind: PrincipalKind) -> usize {
    match kind {
        PrincipalKind::Owner => STREAM_META_OWNER_NAME,
        PrincipalKind::Group => STREAM_META_GROUP_NAME,
    }
}

/// Returns the wrapper metadata option for integer ownership changes.
fn principal_int_option(kind: PrincipalKind) -> usize {
    match kind {
        PrincipalKind::Owner => STREAM_META_OWNER,
        PrincipalKind::Group => STREAM_META_GROUP,
    }
}

/// Returns the string-principal runtime helper for the ownership field.
fn principal_string_runtime(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "__rt_chown_user",
        PrincipalKind::Group => "__rt_chgrp_group",
    }
}

/// Returns the string-principal runtime helper for symlink ownership changes.
fn lprincipal_string_runtime(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::Owner => "__rt_lchown_user",
        PrincipalKind::Group => "__rt_lchgrp_group",
    }
}

/// Lowers `chmod()` through wrapper `stream_metadata()` before libc chmod.
fn lower_chmod_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "chmod", 2)?;
    let path = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    let wrapper = ctx.next_label("chmod_wrapper");
    let after = ctx.next_label("chmod_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, path, "chmod path")?;
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve path and mode scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length
            require_int(ctx.load_value_to_result(mode)?.codegen_repr(), "chmod mode")?;
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the requested mode
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // pass path pointer to native chmod
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // pass path length to native chmod
            ctx.emitter.instruction("ldr x3, [sp, #16]");                       // pass requested mode to native chmod
            ctx.emitter.instruction("add sp, sp, #32");                         // release scratch before native chmod
            abi::emit_call_label(ctx.emitter, "__rt_chmod");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper stream_metadata after native chmod
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the requested mode for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the boxed mode value
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass wrapper path pointer
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass wrapper path length
            ctx.emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov x3, #{}", STREAM_META_ACCESS)); // select STREAM_META_ACCESS
            ctx.emitter.instruction("ldr x4, [sp, #16]");                       // pass boxed mode as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve stream_metadata result across value release
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the boxed mode value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore the stream_metadata boolean result
            ctx.emitter.instruction("add sp, sp, #32");                         // release scratch after wrapper chmod
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, path, "chmod path")?;
            ctx.emitter.instruction("sub rsp, 32");                             // reserve path and mode scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length
            require_int(ctx.load_value_to_result(mode)?.codegen_repr(), "chmod mode")?;
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the requested mode
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // pass path pointer to native chmod
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // pass path length to native chmod
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass requested mode to native chmod
            ctx.emitter.instruction("add rsp, 32");                             // release scratch before native chmod
            abi::emit_call_label(ctx.emitter, "__rt_chmod");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper stream_metadata after native chmod
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the requested mode for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the boxed mode value
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass wrapper path pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass wrapper path length
            ctx.emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov rcx, {}", STREAM_META_ACCESS)); // select STREAM_META_ACCESS
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // pass boxed mode as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve stream_metadata result across value release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the boxed mode value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the stream_metadata boolean result
            ctx.emitter.instruction("add rsp, 32");                             // release scratch after wrapper chmod
            ctx.emitter.label(&after);
        }
    }
    store_if_result(ctx, inst)
}

/// Emits wrapper dispatch for `chown()`/`chgrp()` with a string principal.
fn emit_owner_group_name_wrapper_dispatch(
    ctx: &mut FunctionContext<'_>,
    option: usize,
    libc_helper: &str,
) {
    let wrapper = ctx.next_label("meta_name_wrapper");
    let after = ctx.next_label("meta_name_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve name scratch above the preserved path
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve principal name pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve principal name length
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // pass path pointer to libc owner/group resolver
            ctx.emitter.instruction("ldr x2, [sp, #24]");                       // pass path length to libc owner/group resolver
            ctx.emitter.instruction("ldr x3, [sp, #0]");                        // pass principal name pointer to libc resolver
            ctx.emitter.instruction("ldr x4, [sp, #8]");                        // pass principal name length to libc resolver
            ctx.emitter.instruction("add sp, sp, #32");                         // release name scratch and preserved path before libc helper
            abi::emit_call_label(ctx.emitter, libc_helper);
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // reload principal name pointer for boxing
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // reload principal name length for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the boxed principal value
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // pass wrapper path pointer
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // pass wrapper path length
            ctx.emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov x3, #{}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("ldr x4, [sp, #0]");                        // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("str x0, [sp, #8]");                        // preserve stream_metadata result across value release
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // restore the stream_metadata boolean result
            ctx.emitter.instruction("add sp, sp, #32");                         // release name scratch and preserved path
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve name scratch above the preserved path
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve principal name pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve principal name length
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // pass path pointer to libc owner/group resolver
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");           // pass path length to libc owner/group resolver
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass principal name pointer to libc resolver
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass principal name length to libc resolver
            ctx.emitter.instruction("add rsp, 32");                             // release name scratch and preserved path before libc helper
            abi::emit_call_label(ctx.emitter, libc_helper);
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload principal name pointer for boxing
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // reload principal name length for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the boxed principal value
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass wrapper path pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass wrapper path length
            ctx.emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov rcx, {}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 0]");             // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // preserve stream_metadata result across value release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // restore the stream_metadata boolean result
            ctx.emitter.instruction("add rsp, 32");                             // release name scratch and preserved path
            ctx.emitter.label(&after);
        }
    }
}

/// Emits wrapper dispatch for `chown()`/`chgrp()` with an integer principal.
fn emit_owner_group_wrapper_dispatch(ctx: &mut FunctionContext<'_>, option: usize) {
    let wrapper = ctx.next_label("meta_owngrp_wrapper");
    let after = ctx.next_label("meta_owngrp_after");
    let is_owner = option == STREAM_META_OWNER;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, x0");                              // preserve the uid/gid value across path restoration
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the preserved path pointer and length
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve path and principal scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length
            ctx.emitter.instruction("str x9, [sp, #16]");                       // preserve the uid/gid value
            ctx.emitter.instruction("mov x0, x1");                              // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov x1, x2");                              // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // pass path pointer to native chown
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // pass path length to native chown
            if is_owner {
                ctx.emitter.instruction("ldr x3, [sp, #16]");                   // pass uid and leave gid unchanged
                ctx.emitter.instruction("mov x4, #-1");                         // keep the file group unchanged
            } else {
                ctx.emitter.instruction("mov x3, #-1");                         // keep the file owner unchanged
                ctx.emitter.instruction("ldr x4, [sp, #16]");                   // pass gid and leave uid unchanged
            }
            ctx.emitter.instruction("add sp, sp, #32");                         // release scratch before native chown
            abi::emit_call_label(ctx.emitter, "__rt_chown");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload uid/gid for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the boxed principal value
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass wrapper path pointer
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass wrapper path length
            ctx.emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov x3, #{}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("ldr x4, [sp, #16]");                       // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve stream_metadata result across value release
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore the stream_metadata boolean result
            ctx.emitter.instruction("add sp, sp, #32");                         // release wrapper metadata scratch storage
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, rax");                             // preserve the uid/gid value across path restoration
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("sub rsp, 32");                             // reserve path and principal scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r9");            // preserve the uid/gid value
            ctx.emitter.instruction("mov rdi, rax");                            // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, rdx");                            // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // pass path pointer to native chown
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // pass path length to native chown
            if is_owner {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");       // pass uid and leave gid unchanged
                ctx.emitter.instruction("mov rsi, -1");                         // keep the file group unchanged
            } else {
                ctx.emitter.instruction("mov rdi, -1");                         // keep the file owner unchanged
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");       // pass gid and leave uid unchanged
            }
            ctx.emitter.instruction("add rsp, 32");                             // release scratch before native chown
            abi::emit_call_label(ctx.emitter, "__rt_chown");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper stream_metadata after native helper
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload uid/gid for boxing
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the boxed principal value
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass wrapper path pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass wrapper path length
            ctx.emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov rcx, {}", option));           // pass owner/group metadata option
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // pass boxed principal as mixed value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve stream_metadata result across value release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the boxed principal value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the stream_metadata boolean result
            ctx.emitter.instruction("add rsp, 32");                             // release wrapper metadata scratch storage
            ctx.emitter.label(&after);
        }
    }
}

/// Emits wrapper `stream_metadata()` dispatch for a loaded `touch()` call.
fn emit_touch_wrapper_dispatch(ctx: &mut FunctionContext<'_>) {
    let wrapper = ctx.next_label("touch_wrapper");
    let after = ctx.next_label("touch_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #48");                         // reserve path, time, flags, and result scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length
            ctx.emitter.instruction("str x3, [sp, #16]");                       // preserve mtime seconds
            ctx.emitter.instruction("str x4, [sp, #24]");                       // preserve atime seconds
            ctx.emitter.instruction("str x5, [sp, #32]");                       // preserve current-time flags
            ctx.emitter.instruction("mov x0, x1");                              // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov x1, x2");                              // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction(&format!("cbnz x0, {}", wrapper));          // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("ldr x1, [sp, #0]");                        // pass path pointer to native touch
            ctx.emitter.instruction("ldr x2, [sp, #8]");                        // pass path length to native touch
            ctx.emitter.instruction("ldr x3, [sp, #16]");                       // pass mtime seconds to native touch
            ctx.emitter.instruction("ldr x4, [sp, #24]");                       // pass atime seconds to native touch
            ctx.emitter.instruction("ldr x5, [sp, #32]");                       // pass current-time flags to native touch
            ctx.emitter.instruction("add sp, sp, #48");                         // release scratch before native touch
            abi::emit_call_label(ctx.emitter, "__rt_touch");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper stream_metadata after native touch
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // pass mtime to touch metadata array builder
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // pass atime to touch metadata array builder
            ctx.emitter.instruction("ldr x2, [sp, #32]");                       // pass current-time flags to metadata array builder
            abi::emit_call_label(ctx.emitter, "__rt_touch_meta_array");
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the boxed touch metadata value
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // pass wrapper path pointer
            ctx.emitter.instruction("ldr x1, [sp, #8]");                        // pass wrapper path length
            ctx.emitter.instruction(&format!("mov x2, #{}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov x3, #{}", STREAM_META_TOUCH)); // select STREAM_META_TOUCH
            ctx.emitter.instruction("ldr x4, [sp, #16]");                       // pass boxed touch metadata value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve stream_metadata result across value release
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the boxed touch metadata value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore the stream_metadata boolean result
            ctx.emitter.instruction("add sp, sp, #48");                         // release wrapper touch scratch storage
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 48");                             // reserve path, time, flags, and result scratch storage
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the path pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // preserve the path length
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rdi");           // preserve mtime seconds
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rsi");           // preserve atime seconds
            ctx.emitter.instruction("mov QWORD PTR [rsp + 32], rcx");           // preserve current-time flags
            ctx.emitter.instruction("mov rdi, rax");                            // pass path pointer to wrapper-scheme probe
            ctx.emitter.instruction("mov rsi, rdx");                            // pass path length to wrapper-scheme probe
            abi::emit_call_label(ctx.emitter, "__rt_path_is_wrapper");
            ctx.emitter.instruction("test rax, rax");                           // test whether the path scheme matched a wrapper
            ctx.emitter.instruction(&format!("jnz {}", wrapper));               // registered wrapper schemes use stream_metadata
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // pass path pointer to native touch
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // pass path length to native touch
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass mtime seconds to native touch
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass atime seconds to native touch
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");           // pass current-time flags to native touch
            ctx.emitter.instruction("add rsp, 48");                             // release scratch before native touch
            abi::emit_call_label(ctx.emitter, "__rt_touch");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper stream_metadata after native touch
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // pass mtime to touch metadata array builder
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass atime to touch metadata array builder
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");           // pass current-time flags to metadata array builder
            abi::emit_call_label(ctx.emitter, "__rt_touch_meta_array");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the boxed touch metadata value
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // pass wrapper path pointer
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");            // pass wrapper path length
            ctx.emitter.instruction(&format!("mov rdx, {}", STREAM_METADATA_SLOT)); // select stream_metadata vtable slot
            ctx.emitter.instruction(&format!("mov rcx, {}", STREAM_META_TOUCH)); // select STREAM_META_TOUCH
            ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // pass boxed touch metadata value
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_path_op");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve stream_metadata result across value release
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the boxed touch metadata value
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the stream_metadata boolean result
            ctx.emitter.instruction("add rsp, 48");                             // release wrapper touch scratch storage
            ctx.emitter.label(&after);
        }
    }
}

/// Materializes timestamp arguments for the `touch()` call on ARM64.
fn lower_touch_args_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    match touch_time_shape(ctx, inst)? {
        TouchTimeShape::BothNow => {
            ctx.emitter.instruction("mov x3, #0");                              // ignored mtime seconds when runtime uses current time
            ctx.emitter.instruction("mov x4, #0");                              // ignored atime seconds when runtime uses current time
            ctx.emitter.instruction(&format!("mov x5, #{}", TOUCH_BOTH_NOW));   // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            let mtime = expect_operand(inst, 1)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("mov x3, x0");                              // pass explicit mtime seconds
            ctx.emitter.instruction("mov x4, x0");                              // default atime to the explicit mtime seconds
            ctx.emitter.instruction("mov x5, #0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        TouchTimeShape::ExplicitBoth => {
            let mtime = expect_operand(inst, 1)?;
            let atime = expect_operand(inst, 2)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // save explicit mtime while atime is evaluated
            require_int(ctx.load_value_to_result(atime)?.codegen_repr(), "touch atime")?;
            ctx.emitter.instruction("mov x4, x0");                              // pass explicit atime seconds
            ctx.emitter.instruction("ldr x3, [sp], #16");                       // restore explicit mtime seconds
            ctx.emitter.instruction("mov x5, #0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
    }
    Ok(())
}

/// Materializes timestamp arguments for the `touch()` call on x86_64.
fn lower_touch_args_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    match touch_time_shape(ctx, inst)? {
        TouchTimeShape::BothNow => {
            ctx.emitter.instruction("mov rdi, 0");                              // ignored mtime seconds when runtime uses current time
            ctx.emitter.instruction("mov rsi, 0");                              // ignored atime seconds when runtime uses current time
            ctx.emitter.instruction(&format!("mov rcx, {}", TOUCH_BOTH_NOW));   // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            let mtime = expect_operand(inst, 1)?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass explicit mtime seconds
            ctx.emitter.instruction("mov rsi, rax");                            // default atime to the explicit mtime seconds
            ctx.emitter.instruction("mov rcx, 0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
        TouchTimeShape::ExplicitBoth => {
            let mtime = expect_operand(inst, 1)?;
            let atime = expect_operand(inst, 2)?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(mtime)?.codegen_repr(), "touch mtime")?;
            ctx.emitter.instruction("sub rsp, 16");                             // reserve aligned temporary storage for mtime
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // save explicit mtime while atime is evaluated
            require_int(ctx.load_value_to_result(atime)?.codegen_repr(), "touch atime")?;
            ctx.emitter.instruction("mov rsi, rax");                            // pass explicit atime seconds
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // restore explicit mtime seconds
            ctx.emitter.instruction("add rsp, 16");                             // release the aligned mtime temporary
            ctx.emitter.instruction("mov rcx, 0");                              // mark both timestamp fields as explicit
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    Ok(())
}

/// Classifies optional `touch()` timestamp operands after EIR type checking.
fn touch_time_shape(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<TouchTimeShape> {
    match inst.operands.len() {
        1 => Ok(TouchTimeShape::BothNow),
        2 if is_nullish_value(ctx, expect_operand(inst, 1)?)? => Ok(TouchTimeShape::BothNow),
        2 => Ok(TouchTimeShape::MtimeAlsoAtime),
        _ if is_nullish_value(ctx, expect_operand(inst, 1)?)?
            && is_nullish_value(ctx, expect_operand(inst, 2)?)? =>
        {
            Ok(TouchTimeShape::BothNow)
        }
        _ if is_nullish_value(ctx, expect_operand(inst, 2)?)? => {
            Ok(TouchTimeShape::MtimeAlsoAtime)
        }
        _ => Ok(TouchTimeShape::ExplicitBoth),
    }
}

/// Returns true when an EIR value represents PHP `null`.
fn is_nullish_value(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(value)?.codegen_repr(),
        PhpType::Void
    ))
}

/// Calls the single-component `pathinfo()` helper after materializing an integer flag.
fn lower_pathinfo_string(ctx: &mut FunctionContext<'_>, flag: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(flag)?.codegen_repr(), "pathinfo flags")?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the selected PATHINFO_* flag to the string helper
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(flag)?.codegen_repr(), "pathinfo flags")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the selected PATHINFO_* flag to the string helper
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_pathinfo_str");
    Ok(())
}

/// Lowers dynamic `pathinfo(path, flag)` and boxes string or array results as Mixed.
fn lower_pathinfo_mixed(ctx: &mut FunctionContext<'_>, flag: ValueId) -> Result<()> {
    let array_label = ctx.next_label("pathinfo_dynamic_array");
    let done_label = ctx.next_label("pathinfo_dynamic_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            require_int(ctx.load_value_to_result(flag)?.codegen_repr(), "pathinfo flags")?;
            ctx.emitter.instruction("mov x3, x0");                              // keep the evaluated flag in the string-helper flag register
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("cmp x3, #15");                             // does the runtime flag request PATHINFO_ALL exactly?
            ctx.emitter.instruction(&format!("b.eq {}", array_label));          // runtime PATHINFO_ALL must produce the array shape
            abi::emit_call_label(ctx.emitter, "__rt_pathinfo_str");
            ctx.emitter.instruction("mov x0, #1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip array boxing after building the string result
            ctx.emitter.label(&array_label);
            abi::emit_call_label(ctx.emitter, "__rt_pathinfo_array");
            box_owned_pathinfo_array_as_mixed(ctx);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            require_int(ctx.load_value_to_result(flag)?.codegen_repr(), "pathinfo flags")?;
            ctx.emitter.instruction("mov rdi, rax");                            // keep the evaluated flag in the string-helper flag register
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("cmp rdi, 15");                             // does the runtime flag request PATHINFO_ALL exactly?
            ctx.emitter.instruction(&format!("je {}", array_label));            // runtime PATHINFO_ALL must produce the array shape
            abi::emit_call_label(ctx.emitter, "__rt_pathinfo_str");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the component string pointer as the Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the component string length as the Mixed high payload word
            ctx.emitter.instruction("mov eax, 1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip array boxing after building the string result
            ctx.emitter.label(&array_label);
            abi::emit_call_label(ctx.emitter, "__rt_pathinfo_array");
            box_owned_pathinfo_array_as_mixed(ctx);
            ctx.emitter.label(&done_label);
        }
    }
    Ok(())
}

/// Lowers `getcwd()` through the target-aware runtime helper.
pub(super) fn lower_getcwd(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "getcwd", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_getcwd");
    store_if_result(ctx, inst)
}

/// Lowers `sys_get_temp_dir()` as the project's hardcoded `/tmp` string.
pub(super) fn lower_sys_get_temp_dir(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "sys_get_temp_dir", 0)?;
    let (label, len) = ctx.data.add_string(b"/tmp");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers `tmpfile()` and boxes the anonymous stream descriptor or PHP false.
pub(super) fn lower_tmpfile(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "tmpfile", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_tmpfile");
    box_stream_fd_or_false_result(ctx, "tmpfile");
    store_if_result(ctx, inst)
}

/// Lowers `filesize(path)` through the target-aware runtime stat helper.
pub(super) fn lower_filesize(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_filesize_with_wrapper(ctx, inst)
}

/// Lowers `filemtime(path)` through the target-aware runtime stat helper.
pub(super) fn lower_filemtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_int(ctx, inst, "filemtime", "__rt_filemtime")
}

/// Lowers `linkinfo(path)` through the target-aware runtime lstat helper.
pub(super) fn lower_linkinfo(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_int(ctx, inst, "linkinfo", "__rt_linkinfo")
}

/// Lowers `symlink(target, link)` through the target-aware libc wrapper.
pub(super) fn lower_symlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "symlink", "__rt_symlink")
}

/// Lowers `link(oldpath, newpath)` through the target-aware libc wrapper.
pub(super) fn lower_link(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_binary_path_call(ctx, inst, "link", "__rt_link")
}

/// Lowers `readlink(path)` and boxes the owned runtime string-or-false result.
pub(super) fn lower_readlink(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "readlink", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "readlink")?;
    abi::emit_call_label(ctx.emitter, "__rt_readlink");
    box_owned_string_or_false_result(ctx, "readlink");
    store_if_result(ctx, inst)
}

/// Lowers `fileatime(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileatime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileatime", "__rt_fileatime")
}

/// Lowers `filectime(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_filectime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "filectime", "__rt_filectime")
}

/// Lowers `fileperms(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileperms(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileperms", "__rt_fileperms")
}

/// Lowers `fileowner(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileowner(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileowner", "__rt_fileowner")
}

/// Lowers `filegroup(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_filegroup(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "filegroup", "__rt_filegroup")
}

/// Lowers `fileinode(path)` and boxes the runtime integer-or-false result.
pub(super) fn lower_fileinode(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_stat_int_or_false(ctx, inst, "fileinode", "__rt_fileinode")
}

/// Lowers `filetype(path)` and boxes the runtime string-or-false result.
pub(super) fn lower_filetype(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "filetype", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "filetype")?;
    abi::emit_call_label(ctx.emitter, "__rt_filetype");
    box_stat_string_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `stat(path)` and boxes the runtime stat array or PHP false result.
pub(super) fn lower_stat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_stat_array_or_false(ctx, inst, "stat", "__rt_stat_array")
}

/// Lowers `lstat(path)` and boxes the runtime lstat array or PHP false result.
pub(super) fn lower_lstat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_unary_path_stat_array_or_false(ctx, inst, "lstat", "__rt_lstat_array")
}

/// Lowers `fstat(stream)` and boxes the runtime stat array or PHP false result.
pub(super) fn lower_fstat(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "fstat", 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "fstat")?;
    let wrapper_label = ctx.next_label("fstat_user_wrapper");
    let done_label = ctx.next_label("fstat_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));        // dispatch synthetic handles to stream_stat
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether this stream is a userspace-wrapper handle
            ctx.emitter.instruction(&format!("jge {}", wrapper_label));         // dispatch synthetic handles to stream_stat
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fstat_array");
    box_stat_array_or_false_result(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip wrapper stat after the native helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip wrapper stat after the native helper
        }
    }
    ctx.emitter.label(&wrapper_label);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the synthetic wrapper descriptor to the stat helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fstat");
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `clearstatcache(...)` as an ordered no-op after EIR operand evaluation.
pub(super) fn lower_clearstatcache(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "clearstatcache expected at most 2 args, got {}",
            inst.operands.len()
        )));
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_file(path)` through the target-aware runtime stat helper.
pub(super) fn lower_is_file(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_is_file_with_wrapper(ctx, inst)
}

/// Lowers `is_dir(path)` through the target-aware runtime stat helper.
pub(super) fn lower_is_dir(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_dir", "__rt_is_dir")
}

/// Lowers `is_readable(path)` through the target-aware runtime access helper.
pub(super) fn lower_is_readable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_readable", "__rt_is_readable")
}

/// Lowers `is_writable(path)` through the target-aware runtime access helper.
pub(super) fn lower_is_writable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_writable", "__rt_is_writable")
}

/// Lowers `is_writeable(path)`, PHP's alias of `is_writable(path)`.
pub(super) fn lower_is_writeable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_writeable", "__rt_is_writable")
}

/// Lowers `is_executable(path)` through the target-aware runtime access helper.
pub(super) fn lower_is_executable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_executable", "__rt_is_executable")
}

/// Lowers `is_link(path)` through the target-aware runtime lstat helper.
pub(super) fn lower_is_link(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_path_predicate(ctx, inst, "is_link", "__rt_is_link")
}

/// Emits the wrapper-vs-filesystem dispatch for `readfile()`.
fn emit_readfile_wrapper_dispatch(ctx: &mut FunctionContext<'_>) {
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
fn lower_file_exists_with_wrapper(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "file_exists", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "file_exists")?;
    emit_file_exists_wrapper_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Emits `file_exists()` wrapper `url_stat()` dispatch for the loaded path.
fn emit_file_exists_wrapper_dispatch(ctx: &mut FunctionContext<'_>) {
    let fallback = ctx.next_label("file_exists_fs");
    let done = ctx.next_label("file_exists_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // reserve path/result scratch storage
            ctx.emitter.instruction("str x1, [sp, #0]");                        // preserve the path pointer for filesystem stat
            ctx.emitter.instruction("str x2, [sp, #8]");                        // preserve the path length for filesystem stat
            ctx.emitter.instruction("mov x0, x1");                              // pass the path pointer to url_stat
            ctx.emitter.instruction("mov x1, x2");                              // pass the path length to url_stat
            ctx.emitter.instruction("mov x2, #0");                              // pass url_stat flags = 0
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
            ctx.emitter.instruction("xor edx, edx");                            // pass url_stat flags = 0
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

/// Lowers `filesize()` through userspace `url_stat()['size']` before filesystem stat.
fn lower_filesize_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "filesize", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "filesize")?;
    emit_url_stat_field_or_fallback(ctx, "__rt_filesize", 0);
    store_if_result(ctx, inst)
}

/// Lowers `is_file()` through userspace `url_stat()['mode']` before filesystem stat.
fn lower_is_file_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "is_file", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "is_file")?;
    emit_is_file_wrapper_dispatch(ctx);
    store_if_result(ctx, inst)
}

/// Emits a wrapper url_stat field lookup with a native filesystem fallback.
fn emit_url_stat_field_or_fallback(
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
fn emit_is_file_wrapper_dispatch(ctx: &mut FunctionContext<'_>) {
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
fn lower_single_path_wrapper_op(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
    vtable_slot: usize,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    emit_single_path_wrapper_dispatch(ctx, runtime_label, vtable_slot);
    store_if_result(ctx, inst)
}

/// Emits wrapper dispatch for a single-path mutation with native fallback.
fn emit_single_path_wrapper_dispatch(
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
fn emit_unlink_maybe_phar_dispatch(ctx: &mut FunctionContext<'_>) {
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
fn lower_rename_with_wrapper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "rename", 2)?;
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

/// Loads a path string into runtime argument/result registers and stores the boolean result.
fn lower_unary_path_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Loads a path string into runtime argument/result registers and stores the integer result.
fn lower_unary_path_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Loads a path string, calls an array-returning runtime helper, and stores the array.
fn lower_unary_path_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Loads a stream resource, calls a boolean fd runtime helper, and stores its result.
fn lower_unary_stream_bool_runtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Stores `__rt_flock`'s would-block output into a local slot while preserving the return value.
fn store_flock_would_block(ctx: &mut FunctionContext<'_>, slot: LocalSlotId) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, x1");                              // move would_block into the canonical integer register for local storage
            abi::store_at_offset(ctx.emitter, "x0", offset);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, rdx");                            // move would_block into the canonical integer register for local storage
            abi::store_at_offset(ctx.emitter, "rax", offset);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Returns the local slot loaded by a stream builtin operand when it came from `load_local`.
fn source_load_local_slot(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LocalSlotId>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    if inst_ref.op == Op::LoadLocal {
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// Loads two path strings into the runtime ABI, calls a helper, and stores its result.
fn lower_binary_path_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 2)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, first, name)?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, second, name)?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the second path pointer in the runtime helper's secondary string slot
            ctx.emitter.instruction("mov x4, x2");                              // pass the second path length in the runtime helper's secondary string slot
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, first, name)?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, second, name)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the second path pointer while the first path remains on the stack
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the second path length while the first path remains on the stack
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, runtime_label);
    store_if_result(ctx, inst)
}

/// Dispatches a directory handle to libc/glob runtime helpers or userspace wrappers.
fn lower_directory_handle_dispatch(
    ctx: &mut FunctionContext<'_>,
    runtime_label: &str,
    wrapper_label: &str,
    label_prefix: &str,
) {
    let wrapper = ctx.next_label(&format!("{}_wrapper", label_prefix));
    let after = ctx.next_label(&format!("{}_after", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrapper));              // dispatch synthetic handles to the wrapper directory runtime
            abi::emit_call_label(ctx.emitter, runtime_label);
            ctx.emitter.instruction(&format!("b {}", after));                   // skip the wrapper path after the native directory call
            ctx.emitter.label(&wrapper);
            abi::emit_call_label(ctx.emitter, wrapper_label);
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rax, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrapper));               // dispatch synthetic handles to the wrapper directory runtime
            ctx.emitter.instruction("mov rdi, rax");                            // pass the native directory descriptor to the runtime helper
            abi::emit_call_label(ctx.emitter, runtime_label);
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip the wrapper path after the native directory call
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rdi, rax");                            // pass the synthetic wrapper descriptor to the runtime helper
            abi::emit_call_label(ctx.emitter, wrapper_label);
            ctx.emitter.label(&after);
        }
    }
}

/// Dispatches `stream_set_timeout` to native fd handling or wrapper `stream_set_option`.
fn lower_stream_timeout_dispatch(ctx: &mut FunctionContext<'_>) {
    let wrapper = ctx.next_label("set_timeout_wrapper");
    let after = ctx.next_label("set_timeout_after");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrapper));              // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_timeout");
            ctx.emitter.instruction(&format!("b {}", after));                   // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov x3, x2");                              // pass microseconds as wrapper option arg2
            ctx.emitter.instruction("mov x2, x1");                              // pass seconds as wrapper option arg1
            ctx.emitter.instruction(&format!("mov x1, #{}", STREAM_OPTION_READ_TIMEOUT)); // select STREAM_OPTION_READ_TIMEOUT
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrapper));               // dispatch synthetic handles to stream_set_option
            abi::emit_call_label(ctx.emitter, "__rt_stream_set_timeout");
            ctx.emitter.instruction(&format!("jmp {}", after));                 // skip wrapper dispatch after the native fd update
            ctx.emitter.label(&wrapper);
            ctx.emitter.instruction("mov rcx, rdx");                            // pass microseconds as wrapper option arg2
            ctx.emitter.instruction("mov rdx, rsi");                            // pass seconds as wrapper option arg1
            ctx.emitter.instruction(&format!("mov rsi, {}", STREAM_OPTION_READ_TIMEOUT)); // select STREAM_OPTION_READ_TIMEOUT
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_set_option");
            ctx.emitter.label(&after);
        }
    }
}

/// Calls the read-all `stream_get_contents` runtime helper for the loaded fd.
fn lower_stream_get_contents_read_all(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the stream descriptor to the read-all helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents");
}

/// Materializes `stream_socket_accept` timeout as microseconds or `-1`.
fn lower_stream_socket_accept_timeout(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let Some(timeout) = inst.operands.get(1).copied() else {
        emit_fd_result(ctx, -1);
        return Ok(());
    };
    if matches!(
        ctx.raw_value_php_type(timeout)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        emit_fd_result(ctx, -1);
        return Ok(());
    }
    require_int(
        ctx.load_value_to_result(timeout)?.codegen_repr(),
        "stream_socket_accept timeout",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, #0x4240");                         // load low bits of 1_000_000 microseconds per second
            ctx.emitter.instruction("movk x9, #0xF, lsl #16");                  // complete the 1_000_000 multiplier
            ctx.emitter.instruction("mul x0, x0, x9");                          // convert timeout seconds to microseconds
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("imul rax, rax, 1000000");                  // convert timeout seconds to microseconds
        }
    }
    Ok(())
}

/// Stores `_accept_peer_*` into a local string slot while preserving the result.
fn store_accept_peer_name(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let Some(slot) = source_load_local_slot(ctx, value)? else {
        return Err(CodegenIrError::unsupported(
            "stream_socket_accept peer_name output for non-local arguments",
        ));
    };
    let offset = ctx.local_offset(slot)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_accept_peer_ptr");
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the accepted peer address pointer
            abi::emit_symbol_address(ctx.emitter, "x9", "_accept_peer_len");
            ctx.emitter.instruction("ldr x11, [x9]");                           // load the accepted peer address byte length
            abi::store_at_offset(ctx.emitter, "x10", offset);
            abi::store_at_offset(ctx.emitter, "x11", offset - 8);
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_accept_peer_ptr");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // load the accepted peer address pointer
            abi::emit_symbol_address(ctx.emitter, "r9", "_accept_peer_len");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // load the accepted peer address byte length
            abi::store_at_offset(ctx.emitter, "r10", offset);
            abi::store_at_offset(ctx.emitter, "r11", offset - 8);
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Stores `stream_socket_recvfrom`'s sender address into a local output slot.
fn store_recvfrom_address(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let Some(slot) = source_load_local_slot(ctx, value)? else {
        return Err(CodegenIrError::unsupported(
            "stream_socket_recvfrom address output for non-local arguments",
        ));
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_recvfrom_addr_ptr");
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the stashed sender-address pointer
            abi::emit_symbol_address(ctx.emitter, "x9", "_recvfrom_addr_len");
            ctx.emitter.instruction("ldr x11, [x9]");                           // load the stashed sender-address byte length
            store_string_output_to_local(ctx, slot, "x10", "x11")?;
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_recvfrom_addr_ptr");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // load the stashed sender-address pointer
            abi::emit_symbol_address(ctx.emitter, "r9", "_recvfrom_addr_len");
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // load the stashed sender-address byte length
            store_string_output_to_local(ctx, slot, "r10", "r11")?;
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Stores local `$errno` and `$errstr` outputs for `fsockopen`.
fn store_fsockopen_error_outputs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let errno_slot = if inst.operands.len() >= 3 {
        source_load_local_slot(ctx, expect_operand(inst, 2)?)?
    } else {
        None
    };
    let errstr_slot = if inst.operands.len() >= 4 {
        source_load_local_slot(ctx, expect_operand(inst, 3)?)?
    } else {
        None
    };
    if errno_slot.is_none() && errstr_slot.is_none() {
        return Ok(());
    }
    let (empty_sym, _) = ctx.data.add_string(b"");
    let (msg_sym, msg_len) = ctx.data.add_string(b"Connection refused");
    let econnrefused = ctx.emitter.platform.econnrefused();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the fsockopen connection succeeded
            ctx.emitter.instruction("mov x9, #0");                              // success error code is zero
            ctx.emitter.instruction(&format!("mov x10, #{}", econnrefused));    // failure error code is ECONNREFUSED
            ctx.emitter.instruction("csel x9, x9, x10, ge");                    // choose the error code for the connection outcome
            abi::emit_symbol_address(ctx.emitter, "x10", &msg_sym);
            abi::emit_symbol_address(ctx.emitter, "x11", &empty_sym);
            ctx.emitter.instruction("csel x10, x11, x10, ge");                  // choose the error-message pointer for the outcome
            ctx.emitter.instruction("mov x11, #0");                             // success error-message length is zero
            ctx.emitter.instruction(&format!("mov x12, #{}", msg_len));         // failure error-message byte length
            ctx.emitter.instruction("csel x11, x11, x12, ge");                  // choose the error-message length for the outcome
            if let Some(slot) = errstr_slot {
                let preserve_errno = errno_slot.is_some()
                    && ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed;
                if preserve_errno {
                    abi::emit_push_reg(ctx.emitter, "x9");
                }
                store_string_output_to_local(ctx, slot, "x10", "x11")?;
                if preserve_errno {
                    abi::emit_pop_reg(ctx.emitter, "x9");
                }
            }
            if let Some(slot) = errno_slot {
                store_int_output_to_local(ctx, slot, "x9")?;
            }
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("cmp rax, 0");                              // test whether the fsockopen connection succeeded
            ctx.emitter.instruction(&format!("mov r9, {}", econnrefused));      // failure error code is ECONNREFUSED
            ctx.emitter.instruction("mov r10, 0");                              // success error code is zero without clobbering compare flags
            ctx.emitter.instruction("cmovge r9, r10");                          // choose the error code for the connection outcome
            abi::emit_symbol_address(ctx.emitter, "r10", &msg_sym);
            abi::emit_symbol_address(ctx.emitter, "r11", &empty_sym);
            ctx.emitter.instruction("cmovge r10, r11");                         // choose the error-message pointer for the outcome
            ctx.emitter.instruction(&format!("mov r11, {}", msg_len));          // failure error-message byte length
            ctx.emitter.instruction("mov rcx, 0");                              // success error-message length is zero without clobbering compare flags
            ctx.emitter.instruction("cmovge r11, rcx");                         // choose the error-message length for the outcome
            if let Some(slot) = errstr_slot {
                let preserve_errno = errno_slot.is_some()
                    && ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed;
                if preserve_errno {
                    abi::emit_push_reg(ctx.emitter, "r9");
                }
                store_string_output_to_local(ctx, slot, "r10", "r11")?;
                if preserve_errno {
                    abi::emit_pop_reg(ctx.emitter, "r9");
                }
            }
            if let Some(slot) = errno_slot {
                store_int_output_to_local(ctx, slot, "r9")?;
            }
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    Ok(())
}

/// Stores an integer output into a local slot, boxing it when the slot is `Mixed`.
fn store_int_output_to_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    value_reg: &str,
) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x0, {}", value_reg));     // move the error code into the canonical integer result register
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov rax, {}", value_reg));    // move the error code into the canonical integer result register
            }
        }
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
        return Ok(());
    }
    abi::store_at_offset_scratch(ctx.emitter, value_reg, offset, "x13");
    Ok(())
}

/// Stores a string output into a local slot, boxing it when the slot is `Mixed`.
fn store_string_output_to_local(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let offset = ctx.local_offset(slot)?;
    if ctx.local_php_type(slot)?.codegen_repr() == PhpType::Mixed {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x1, {}", ptr_reg));       // move the error-message pointer into the canonical string result register
                ctx.emitter.instruction(&format!("mov x2, {}", len_reg));       // move the error-message length into the canonical string result register
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("mov rax, {}", ptr_reg));      // move the error-message pointer into the canonical string result register
                ctx.emitter.instruction(&format!("mov rdx, {}", len_reg));      // move the error-message length into the canonical string result register
            }
        }
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
        abi::store_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), offset);
        return Ok(());
    }
    abi::store_at_offset_scratch(ctx.emitter, ptr_reg, offset, "x13");
    abi::store_at_offset_scratch(ctx.emitter, len_reg, offset - 8, "x13");
    Ok(())
}

/// Maps runtime-supported built-in stream filter names to byte-table ids.
fn stream_filter_id(name: &str) -> Option<u8> {
    match name {
        "string.toupper" => Some(1),
        "string.tolower" => Some(2),
        "string.rot13" => Some(3),
        "string.strip_tags" => Some(4),
        "dechunk" => Some(5),
        "convert.base64-encode" => Some(6),
        "convert.base64-decode" => Some(7),
        "convert.quoted-printable-encode" => Some(8),
        "convert.quoted-printable-decode" => Some(9),
        _ => None,
    }
}

/// Reads a compile-time integer filter parameter from the fourth builtin operand.
fn const_int_filter_param(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    _key: &str,
    primary: bool,
    min: i64,
    max: i64,
) -> Result<Option<i64>> {
    if !primary {
        return Ok(None);
    }
    let Some(value) = inst.operands.get(3).copied() else {
        return Ok(None);
    };
    Ok(optional_const_i64_operand(ctx, value)?.map(|n| n.clamp(min, max)))
}

/// Returns a literal integer operand when the value was produced by `ConstI64`.
fn optional_const_i64_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<i64>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstI64 {
        return Ok(None);
    }
    match inst_ref.immediate {
        Some(Immediate::I64(value)) => Ok(Some(value)),
        _ => Err(CodegenIrError::invalid_module(
            "integer literal operand has no i64 immediate",
        )),
    }
}

/// Attaches a built-in stream filter by writing its id into per-fd direction tables.
fn lower_builtin_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    id: u8,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    materialize_stream_filter_mode(ctx, inst)?;
    let skip_read = ctx.next_label("sf_skip_read");
    let skip_write = ctx.next_label("sf_skip_write");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x1");
            ctx.emitter.instruction("tst x0, #1");                              // test whether STREAM_FILTER_READ is enabled
            ctx.emitter.instruction(&format!("b.eq {}", skip_read));            // skip the read-filter table when the read bit is clear
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction(&format!("mov w10, #{}", id));              // materialize the built-in stream-filter id
            ctx.emitter.instruction("strb w10, [x9, x1]");                      // record the read filter for this descriptor
            ctx.emitter.label(&skip_read);
            ctx.emitter.instruction("tst x0, #2");                              // test whether STREAM_FILTER_WRITE is enabled
            ctx.emitter.instruction(&format!("b.eq {}", skip_write));           // skip the write-filter table when the write bit is clear
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction(&format!("mov w10, #{}", id));              // materialize the built-in stream-filter id
            ctx.emitter.instruction("strb w10, [x9, x1]");                      // record the write filter for this descriptor
            ctx.emitter.label(&skip_write);
            ctx.emitter.instruction("mov x0, x1");                              // move the descriptor into the resource payload register
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "rcx");
            ctx.emitter.instruction("test rax, 1");                             // test whether STREAM_FILTER_READ is enabled
            ctx.emitter.instruction(&format!("jz {}", skip_read));              // skip the read-filter table when the read bit is clear
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters"); // read-filter table base
            ctx.emitter.instruction(&format!("mov BYTE PTR [r9 + rcx], {}", id)); // record the read filter for this descriptor
            ctx.emitter.label(&skip_read);
            ctx.emitter.instruction("test rax, 2");                             // test whether STREAM_FILTER_WRITE is enabled
            ctx.emitter.instruction(&format!("jz {}", skip_write));             // skip the write-filter table when the write bit is clear
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters"); // write-filter table base
            ctx.emitter.instruction(&format!("mov BYTE PTR [r9 + rcx], {}", id)); // record the write filter for this descriptor
            ctx.emitter.label(&skip_write);
            ctx.emitter.instruction("mov rax, rcx");                            // move the descriptor into the resource payload register
        }
    }
    emit_boxed_stream_resource(ctx);
    store_if_result(ctx, inst)
}

/// Materializes the stream-filter mode operand, defaulting to STREAM_FILTER_ALL.
fn materialize_stream_filter_mode(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() >= 3 {
        let mode = expect_operand(inst, 2)?;
        require_int_or_bool(
            ctx.load_value_to_result(mode)?.codegen_repr(),
            "stream_filter_append mode",
        )?;
        return Ok(());
    }
    emit_fd_result(ctx, 3);
    Ok(())
}

/// Materializes the optional stream-filter params operand as an owned boxed
/// Mixed cell, defaulting to PHP null when the caller omitted it.
fn materialize_stream_filter_params(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() < 4 {
        emit_null_mixed(ctx);
        return Ok(());
    }
    let params = expect_operand(inst, 3)?;
    let params_ty = ctx.value_php_type(params)?.codegen_repr();
    ctx.load_value_to_result(params)?;
    if matches!(params_ty, PhpType::Mixed | PhpType::Union(_)) {
        if !ctx.value_can_own_mixed_box_source(params)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &params_ty);
        }
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &params_ty);
    }
    Ok(())
}

/// Attaches a user-defined stream filter through the runtime registry.
fn lower_user_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    let filter = expect_operand(inst, 1)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            materialize_stream_filter_mode(ctx, inst)?;
            abi::emit_push_reg(ctx.emitter, "x0");
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov x4, x0");                              // pass the boxed stream-filter params to the attach helper
            abi::emit_pop_reg(ctx.emitter, "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("ldr x0, [sp]");                            // pass the saved stream descriptor without popping it yet
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            materialize_stream_filter_mode(ctx, inst)?;
            abi::emit_push_reg(ctx.emitter, "rax");
            materialize_stream_filter_params(ctx, inst)?;
            ctx.emitter.instruction("mov r8, rax");                             // pass the boxed stream-filter params to the attach helper
            abi::emit_pop_reg(ctx.emitter, "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // pass the saved stream descriptor without popping it yet
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_filter_attach_user");
    let fail = ctx.next_label("sfau_false");
    let done = ctx.next_label("sfau_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", fail));              // unknown filter or failed onCreate returns PHP false
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the descriptor for the returned filter resource
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_stream_resource(ctx);
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the PHP false boxing path
            ctx.emitter.label(&fail);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_bool(ctx, false);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the attach helper report success?
            ctx.emitter.instruction(&format!("jz {}", fail));                   // unknown filter or failed onCreate returns PHP false
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // reload the descriptor for the returned filter resource
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_stream_resource(ctx);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the PHP false boxing path
            ctx.emitter.label(&fail);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_boxed_bool(ctx, false);
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Boxes the current integer result as a PHP stream resource Mixed cell.
fn emit_boxed_stream_resource(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // use the descriptor as the resource payload
            ctx.emitter.instruction("mov x2, #0");                              // resource Mixed payloads do not use the high word
            ctx.emitter.instruction("mov x0, #9");                              // runtime tag 9 = resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // use the descriptor as the resource payload
            ctx.emitter.instruction("xor esi, esi");                            // resource Mixed payloads do not use the high word
            ctx.emitter.instruction("mov eax, 9");                              // runtime tag 9 = resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Boxes a PHP boolean Mixed cell in the current result register.
fn emit_boxed_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    emit_bool_result(ctx, value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Boxes a PHP null Mixed cell in the current result register.
fn emit_null_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #0");                              // null has no payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor eax, eax");                            // null has no payload
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Emits the AArch64 body for `stream_bucket_new`.
fn lower_stream_bucket_new_aarch64(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldr x1, [sp, #16]");                               // reload the bucket data string pointer
    ctx.emitter.instruction("ldr x2, [sp, #24]");                               // reload the bucket data string length
    ctx.emitter.instruction("mov x0, #1");                                      // runtime tag 1 = string
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov x3, x0");                                      // pass boxed data as the stdClass property value
    abi::emit_pop_reg(ctx.emitter, "x0");
    abi::emit_push_reg(ctx.emitter, "x0");
    let (data_sym, data_len) = ctx.data.add_string(b"data");
    abi::emit_symbol_address(ctx.emitter, "x1", &data_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", data_len));                 // pass the `data` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    ctx.emitter.instruction("ldr x1, [sp, #24]");                               // use the original string length as datalen
    ctx.emitter.instruction("mov x2, #0");                                      // integer Mixed payloads do not use the high word
    ctx.emitter.instruction("mov x0, #0");                                      // runtime tag 0 = int
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov x3, x0");                                      // pass boxed datalen as the property value
    abi::emit_pop_reg(ctx.emitter, "x0");
    let (datalen_sym, datalen_len) = ctx.data.add_string(b"datalen");
    abi::emit_symbol_address(ctx.emitter, "x1", &datalen_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", datalen_len));              // pass the `datalen` property-name length
    abi::emit_push_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_pop_reg(ctx.emitter, "x0");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("mov x1, x0");                                      // pass the bucket object pointer as the Mixed payload
    ctx.emitter.instruction("mov x2, #0");                                      // object Mixed payloads do not use the high word
    ctx.emitter.instruction("mov x0, #6");                                      // runtime tag 6 = object
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Emits the x86_64 body for `stream_bucket_new`.
fn lower_stream_bucket_new_x86_64(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                   // reload the bucket data string pointer
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                   // reload the bucket data string length
    ctx.emitter.instruction("mov rax, 1");                                      // runtime tag 1 = string
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov rcx, rax");                                    // pass boxed data as the stdClass property value
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the bucket object as the first stdClass argument
    let (data_sym, data_len) = ctx.data.add_string(b"data");
    abi::emit_symbol_address(ctx.emitter, "rsi", &data_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", data_len));                 // pass the `data` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                   // use the original string length as datalen
    ctx.emitter.instruction("xor esi, esi");                                    // integer Mixed payloads do not use the high word
    ctx.emitter.instruction("mov rax, 0");                                      // runtime tag 0 = int
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov rcx, rax");                                    // pass boxed datalen as the property value
    abi::emit_pop_reg(ctx.emitter, "rdi");
    abi::emit_push_reg(ctx.emitter, "rdi");
    let (datalen_sym, datalen_len) = ctx.data.add_string(b"datalen");
    abi::emit_symbol_address(ctx.emitter, "rsi", &datalen_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", datalen_len));              // pass the `datalen` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_pop_reg(ctx.emitter, "rdi");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("xor esi, esi");                                    // object Mixed payloads do not use the high word
    ctx.emitter.instruction("mov rax, 6");                                      // runtime tag 6 = object
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Emits the AArch64 body for stream bucket append/prepend.
fn lower_stream_bucket_append_aarch64(
    ctx: &mut FunctionContext<'_>,
    bucket: ValueId,
    brigade_is_mixed: bool,
    buckets_sym: &str,
    buckets_len: usize,
    done: &str,
    init: &str,
    existing: &str,
) -> Result<()> {
    if brigade_is_mixed {
        ctx.emitter.instruction(&format!("cbz x0, {}", done));                  // null Mixed means there is no brigade to mutate
        ctx.emitter.instruction("ldr x9, [x0]");                                // load the Mixed runtime tag
        ctx.emitter.instruction("cmp x9, #6");                                  // tag 6 identifies object values
        ctx.emitter.instruction(&format!("b.ne {}", done));                     // non-object brigades are ignored
        ctx.emitter.instruction("ldr x0, [x0, #8]");                            // unbox the stdClass object pointer
    }
    ctx.emitter.instruction(&format!("cbz x0, {}", done));                      // null brigade objects are ignored
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_result(bucket)?;
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the brigade object for `_buckets` lookup
    abi::emit_symbol_address(ctx.emitter, "x1", buckets_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    ctx.emitter.instruction(&format!("cbz x0, {}", init));                      // missing `_buckets` property allocates a fresh array
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the property Mixed tag
    ctx.emitter.instruction("cmp x9, #4");                                      // tag 4 identifies indexed arrays
    ctx.emitter.instruction(&format!("b.ne {}", init));                         // non-array `_buckets` allocates a fresh array
    ctx.emitter.instruction("ldr x9, [x0, #8]");                                // unbox the indexed-array pointer
    ctx.emitter.instruction(&format!("cbz x9, {}", init));                      // null array payload allocates a fresh array
    ctx.emitter.instruction("mov x0, x9");                                      // use the existing `_buckets` array
    ctx.emitter.instruction(&format!("b {}", existing));                        // skip fresh-array allocation

    ctx.emitter.label(init);
    ctx.emitter.instruction("mov x0, #4");                                      // initial bucket-array capacity
    ctx.emitter.instruction("mov x1, #8");                                      // bucket-array elements are Mixed-cell pointers
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    ctx.emitter.instruction("ldr x10, [x0, #-8]");                              // load the array metadata word
    ctx.emitter.instruction("mov x12, #0x80ff");                                // preserve kind and COW bits while changing value type
    ctx.emitter.instruction("and x10, x10, x12");                               // keep only the persistent array metadata bits
    ctx.emitter.instruction("mov x11, #7");                                     // value_type 7 = boxed Mixed pointer
    ctx.emitter.instruction("lsl x11, x11, #8");                                // move the value type into the metadata byte lane
    ctx.emitter.instruction("orr x10, x10, x11");                               // merge the boxed-Mixed value type
    ctx.emitter.instruction("str x10, [x0, #-8]");                              // store the updated array metadata word

    ctx.emitter.label(existing);
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the bucket Mixed cell for retention
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_pop_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldr x1, [sp, #0]");                                // pass the bucket Mixed cell to array_push
    abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
    ctx.emitter.instruction("mov x1, x0");                                      // pass the bucket array as the Mixed payload
    ctx.emitter.instruction("mov x2, #0");                                      // indexed-array Mixed payloads do not use the high word
    ctx.emitter.instruction("mov x0, #4");                                      // runtime tag 4 = indexed array
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov x3, x0");                                      // pass the boxed array as the stdClass property value
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the brigade object
    abi::emit_symbol_address(ctx.emitter, "x1", buckets_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(done);
    Ok(())
}

/// Emits the x86_64 body for stream bucket append/prepend.
fn lower_stream_bucket_append_x86_64(
    ctx: &mut FunctionContext<'_>,
    bucket: ValueId,
    brigade_is_mixed: bool,
    buckets_sym: &str,
    buckets_len: usize,
    done: &str,
    init: &str,
    existing: &str,
) -> Result<()> {
    if brigade_is_mixed {
        ctx.emitter.instruction("test rax, rax");                               // null Mixed means there is no brigade to mutate
        ctx.emitter.instruction(&format!("jz {}", done));                       // skip mutation when the brigade is null
        ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                    // load the Mixed runtime tag
        ctx.emitter.instruction("cmp r10, 6");                                  // tag 6 identifies object values
        ctx.emitter.instruction(&format!("jne {}", done));                      // non-object brigades are ignored
        ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // unbox the stdClass object pointer
    }
    ctx.emitter.instruction("test rax, rax");                                   // null brigade objects are ignored
    ctx.emitter.instruction(&format!("jz {}", done));                           // skip mutation when the brigade object is null
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_result(bucket)?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                   // reload the brigade object for `_buckets` lookup
    abi::emit_symbol_address(ctx.emitter, "rsi", buckets_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    ctx.emitter.instruction("test rax, rax");                                   // missing `_buckets` property allocates a fresh array
    ctx.emitter.instruction(&format!("jz {}", init));                           // branch to fresh-array allocation
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the property Mixed tag
    ctx.emitter.instruction("cmp r10, 4");                                      // tag 4 identifies indexed arrays
    ctx.emitter.instruction(&format!("jne {}", init));                          // non-array `_buckets` allocates a fresh array
    ctx.emitter.instruction("mov r10, QWORD PTR [rax + 8]");                    // unbox the indexed-array pointer
    ctx.emitter.instruction("test r10, r10");                                   // null array payload allocates a fresh array
    ctx.emitter.instruction(&format!("jz {}", init));                           // branch to fresh-array allocation
    ctx.emitter.instruction("mov rax, r10");                                    // use the existing `_buckets` array
    ctx.emitter.instruction(&format!("jmp {}", existing));                      // skip fresh-array allocation

    ctx.emitter.label(init);
    ctx.emitter.instruction("mov rdi, 4");                                      // initial bucket-array capacity
    ctx.emitter.instruction("mov rsi, 8");                                      // bucket-array elements are Mixed-cell pointers
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    ctx.emitter.instruction("mov r10, QWORD PTR [rax - 8]");                    // load the array metadata word
    ctx.emitter.instruction("mov r11, 0xffffffff000080ff");                     // preserve magic, kind, and COW bits while changing value type
    ctx.emitter.instruction("and r10, r11");                                    // keep only the persistent array metadata bits
    ctx.emitter.instruction("mov r11, 7");                                      // value_type 7 = boxed Mixed pointer
    ctx.emitter.instruction("shl r11, 8");                                      // move the value type into the metadata byte lane
    ctx.emitter.instruction("or r10, r11");                                     // merge the boxed-Mixed value type
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // store the updated array metadata word

    ctx.emitter.label(existing);
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the bucket Mixed cell for retention
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_pop_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the `_buckets` array to array_push
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // pass the bucket Mixed cell to array_push
    abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the bucket array as the Mixed payload
    ctx.emitter.instruction("xor esi, esi");                                    // indexed-array Mixed payloads do not use the high word
    ctx.emitter.instruction("mov rax, 4");                                      // runtime tag 4 = indexed array
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov rcx, rax");                                    // pass the boxed array as the stdClass property value
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                   // reload the brigade object
    abi::emit_symbol_address(ctx.emitter, "rsi", buckets_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(done);
    Ok(())
}

/// Tears down the TLS session attached to the current fd result, if one exists.
fn emit_tls_session_teardown_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("tls_teardown_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_tls_sessions");
            ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");               // load the TLS session handle for this descriptor
            ctx.emitter.instruction(&format!("cbz x10, {}", skip));             // skip close_notify when no TLS session is attached
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, x10");                             // pass the TLS handle to the close helper
            abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the published TLS close function pointer
            ctx.emitter.instruction("blr x9");                                  // close the TLS session and send close_notify
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_tls_sessions");
            ctx.emitter.instruction("str xzr, [x9, x0, lsl #3]");               // clear the per-fd TLS session slot
            ctx.emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_tls_sessions");       // TLS session table base
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");         // load the TLS session handle for this descriptor
            ctx.emitter.instruction("test r10, r10");                           // test whether a TLS session is attached
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip close_notify when no TLS session is attached
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, r10");                            // pass the TLS handle to the close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_close_fn", 0); // load the published TLS close function pointer
            ctx.emitter.instruction("call r9");                                 // close the TLS session and send close_notify
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "r9", "_tls_sessions");       // TLS session table base
            ctx.emitter.instruction("mov QWORD PTR [r9 + rax*8], 0");           // clear the per-fd TLS session slot
            ctx.emitter.label(&skip);
        }
    }
}

/// Flushes an attached zlib.deflate write filter before the fd is closed.
fn emit_zlib_flush_on_close_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("fclose_zlib_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_zstream_handles");
            ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");               // load this descriptor's zlib stream handle
            ctx.emitter.instruction(&format!("cbz x10, {}", skip));             // skip flush when no zlib filter is attached
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_zlib_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the zlib close helper pointer
            ctx.emitter.instruction("blr x9");                                  // flush the deflate tail and end the zlib stream
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_zstream_handles");    // zlib stream handle table base
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");         // load this descriptor's zlib stream handle
            ctx.emitter.instruction("test r10, r10");                           // test whether a zlib filter is attached
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip flush when no zlib filter is attached
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fd to the zlib close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_zlib_close_fn", 0); // load the zlib close helper pointer
            ctx.emitter.instruction("call r9");                                 // flush the deflate tail and end the zlib stream
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.label(&skip);
        }
    }
}

/// Flushes a `bzip2.compress` write filter before closing the current descriptor.
fn emit_bz2_flush_on_close_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("fclose_bz2_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_bzstream_handles");
            ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");               // load this descriptor's bzip2 stream handle
            ctx.emitter.instruction(&format!("cbz x10, {}", skip));             // skip flush when no bzip2 filter is attached
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_bz2_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the bzip2 close helper pointer
            ctx.emitter.instruction("blr x9");                                  // flush the compressed tail and end the bzip2 stream
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_bzstream_handles");   // bzip2 stream handle table base
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");         // load this descriptor's bzip2 stream handle
            ctx.emitter.instruction("test r10, r10");                           // test whether a bzip2 filter is attached
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip flush when no bzip2 filter is attached
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fd to the bzip2 close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_bz2_close_fn", 0); // load the bzip2 close helper pointer
            ctx.emitter.instruction("call r9");                                 // flush the compressed tail and end the bzip2 stream
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.label(&skip);
        }
    }
}

/// Closes a `convert.iconv` write filter before closing the current descriptor.
fn emit_iconv_flush_on_close_for_current_fd(ctx: &mut FunctionContext<'_>) {
    let skip = ctx.next_label("fclose_iconv_skip");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_iconv_handles");
            ctx.emitter.instruction("ldr x10, [x9, x0, lsl #3]");               // load this descriptor's iconv transcoder handle
            ctx.emitter.instruction(&format!("cbz x10, {}", skip));             // skip close when no iconv write filter is attached
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x9", "_iconv_close_fn");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load the iconv close helper pointer
            ctx.emitter.instruction("blr x9");                                  // close the transcoder and clear the handle
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.label(&skip);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_iconv_handles");      // iconv transcoder handle table base
            ctx.emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");         // load this descriptor's iconv transcoder handle
            ctx.emitter.instruction("test r10, r10");                           // test whether an iconv write filter is attached
            ctx.emitter.instruction(&format!("je {}", skip));                   // skip close when no iconv write filter is attached
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fd to the iconv close helper
            abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_iconv_close_fn", 0); // load the iconv close helper pointer
            ctx.emitter.instruction("call r9");                                 // close the transcoder and clear the handle
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.label(&skip);
        }
    }
}

/// Emits the AArch64 TLS attach path for `stream_socket_enable_crypto(true)`.
fn lower_stream_socket_enable_crypto_attach_aarch64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    crate::codegen::builtins::publish_tls_function_pointers(ctx.emitter);
    let fail_label = ctx.next_label("ssec_attach_fail");
    let peer_ok = ctx.next_label("ssec_peer_ok");
    let host_default = ctx.next_label("ssec_host_default");
    let plain_attach = ctx.next_label("ssec_plain_attach");
    let do_attach = ctx.next_label("ssec_do_attach");
    ctx.emitter.instruction("sub sp, sp, #64");                                 // reserve peer-name and client-cert/key spill storage
    ctx.emitter.instruction("add x0, sp, #0");                                  // pass peer-name out_ptr address
    ctx.emitter.instruction("add x1, sp, #8");                                  // pass peer-name out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_name");
    ctx.emitter.instruction(&format!("cbnz x0, {}", peer_ok));                  // use ssl.peer_name when the context provides it
    ctx.emitter.instruction("ldr x10, [sp, #64]");                              // reload fd for the connect-host table lookup
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_connect_host");
    ctx.emitter.instruction("add x9, x9, x10, lsl #4");                         // address this fd's saved host pointer/length pair
    ctx.emitter.instruction("ldr x11, [x9, #8]");                               // load the saved connection-host byte length
    ctx.emitter.instruction(&format!("cbz x11, {}", host_default));             // fall back to localhost when no connection host is known
    ctx.emitter.instruction("ldr x12, [x9, #0]");                               // load the saved connection-host pointer
    ctx.emitter.instruction("str x12, [sp, #0]");                               // use the connection host as peer_name pointer
    ctx.emitter.instruction("str x11, [sp, #8]");                               // use the connection host as peer_name length
    ctx.emitter.instruction(&format!("b {}", peer_ok));                         // skip the localhost fallback
    ctx.emitter.label(&host_default);
    abi::emit_symbol_address(ctx.emitter, "x9", "_tls_peer_name_default");
    ctx.emitter.instruction("str x9, [sp, #0]");                                // use localhost as the fallback peer_name pointer
    ctx.emitter.instruction("mov x9, #9");                                      // strlen("localhost")
    ctx.emitter.instruction("str x9, [sp, #8]");                                // use localhost as the fallback peer_name length
    ctx.emitter.label(&peer_ok);

    ctx.emitter.instruction("str xzr, [sp, #24]");                              // default local_cert length to zero
    ctx.emitter.instruction("str xzr, [sp, #40]");                              // default local_pk length to zero
    abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
    ctx.emitter.instruction("mov x1, #3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_local_cert_key_str");
    ctx.emitter.instruction("mov x3, #10");                                     // strlen("local_cert")
    ctx.emitter.instruction("add x4, sp, #16");                                 // pass local_cert out_ptr address
    ctx.emitter.instruction("add x5, sp, #24");                                 // pass local_cert out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");
    abi::emit_symbol_address(ctx.emitter, "x0", "_ssl_key_str");
    ctx.emitter.instruction("mov x1, #3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "x2", "_ssl_local_pk_key_str");
    ctx.emitter.instruction("mov x3, #8");                                      // strlen("local_pk")
    ctx.emitter.instruction("add x4, sp, #32");                                 // pass local_pk out_ptr address
    ctx.emitter.instruction("add x5, sp, #40");                                 // pass local_pk out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");

    ctx.emitter.instruction("ldr x0, [sp, #64]");                               // reload fd as the first TLS attach argument
    ctx.emitter.instruction("ldr x1, [sp, #0]");                                // pass peer_name pointer
    ctx.emitter.instruction("ldr x2, [sp, #8]");                                // pass peer_name byte length
    ctx.emitter.instruction("ldr x9, [sp, #24]");                               // load local_cert byte length
    ctx.emitter.instruction(&format!("cbz x9, {}", plain_attach));              // no client certificate selects plain TLS attach
    ctx.emitter.instruction("ldr x9, [sp, #40]");                               // load local_pk byte length
    ctx.emitter.instruction(&format!("cbz x9, {}", plain_attach));              // missing key selects plain TLS attach
    ctx.emitter.instruction("ldr x3, [sp, #16]");                               // pass local_cert path pointer
    ctx.emitter.instruction("ldr x4, [sp, #24]");                               // pass local_cert path length
    ctx.emitter.instruction("ldr x5, [sp, #32]");                               // pass local_pk path pointer
    ctx.emitter.instruction("ldr x6, [sp, #40]");                               // pass local_pk path length
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_attach_fd_client_cert_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the mutual-TLS attach function pointer
    ctx.emitter.instruction(&format!("b {}", do_attach));                       // call the selected attach function
    ctx.emitter.label(&plain_attach);
    abi::emit_symbol_address(ctx.emitter, "x9", "_elephc_tls_attach_fd_fn");
    ctx.emitter.instruction("ldr x9, [x9]");                                    // load the default TLS attach function pointer
    ctx.emitter.label(&do_attach);
    ctx.emitter.instruction("blr x9");                                          // attach TLS to the fd and return a session handle
    ctx.emitter.instruction("ldr x10, [sp, #64]");                              // reload fd before releasing the spill storage
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("cmp x0, #0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("b.lt {}", fail_label));                   // return false when attach failed
    abi::emit_symbol_address(ctx.emitter, "x11", "_tls_sessions");
    ctx.emitter.instruction("str x0, [x11, x10, lsl #3]");                      // store the TLS session handle for this fd
    ctx.emitter.instruction("mov x0, #1");                                      // return true after successful TLS attach
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the failure result
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false after TLS attach failure
}

/// Emits the x86_64 TLS attach path for `stream_socket_enable_crypto(true)`.
fn lower_stream_socket_enable_crypto_attach_x86_64(
    ctx: &mut FunctionContext<'_>,
    done_label: &str,
) {
    crate::codegen::builtins::publish_tls_function_pointers(ctx.emitter);
    let fail_label = ctx.next_label("ssec_attach_fail");
    let peer_ok = ctx.next_label("ssec_peer_ok");
    let host_default = ctx.next_label("ssec_host_default");
    let plain_attach = ctx.next_label("ssec_plain_attach_x");
    let after_attach = ctx.next_label("ssec_after_attach_x");
    ctx.emitter.instruction("sub rsp, 64");                                     // reserve peer-name and client-cert/key spill storage
    ctx.emitter.instruction("lea rdi, [rsp + 0]");                              // pass peer-name out_ptr address
    ctx.emitter.instruction("lea rsi, [rsp + 8]");                              // pass peer-name out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_ssl_peer_name");
    ctx.emitter.instruction("test rax, rax");                                   // did the context provide ssl.peer_name?
    ctx.emitter.instruction(&format!("jnz {}", peer_ok));                       // use ssl.peer_name when present
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                   // reload fd for the connect-host table lookup
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_connect_host");
    ctx.emitter.instruction("shl r10, 4");                                      // fd * 16, the host table stride
    ctx.emitter.instruction("add r9, r10");                                     // address this fd's saved host pointer/length pair
    ctx.emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                     // load the saved connection-host byte length
    ctx.emitter.instruction("test r11, r11");                                   // is a connection host known for this fd?
    ctx.emitter.instruction(&format!("jz {}", host_default));                   // fall back to localhost when no host is known
    ctx.emitter.instruction("mov r10, QWORD PTR [r9 + 0]");                     // load the saved connection-host pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r10");                    // use the connection host as peer_name pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r11");                    // use the connection host as peer_name length
    ctx.emitter.instruction(&format!("jmp {}", peer_ok));                       // skip the localhost fallback
    ctx.emitter.label(&host_default);
    abi::emit_symbol_address(ctx.emitter, "r9", "_tls_peer_name_default");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");                     // use localhost as the fallback peer_name pointer
    ctx.emitter.instruction("mov r9, 9");                                       // strlen("localhost")
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r9");                     // use localhost as the fallback peer_name length
    ctx.emitter.label(&peer_ok);

    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], 0");                     // default local_cert length to zero
    ctx.emitter.instruction("mov QWORD PTR [rsp + 40], 0");                     // default local_pk length to zero
    abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
    ctx.emitter.instruction("mov rsi, 3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_local_cert_key_str");
    ctx.emitter.instruction("mov rcx, 10");                                     // strlen("local_cert")
    ctx.emitter.instruction("lea r8, [rsp + 16]");                              // pass local_cert out_ptr address
    ctx.emitter.instruction("lea r9, [rsp + 24]");                              // pass local_cert out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");
    abi::emit_symbol_address(ctx.emitter, "rdi", "_ssl_key_str");
    ctx.emitter.instruction("mov rsi, 3");                                      // strlen("ssl")
    abi::emit_symbol_address(ctx.emitter, "rdx", "_ssl_local_pk_key_str");
    ctx.emitter.instruction("mov rcx, 8");                                      // strlen("local_pk")
    ctx.emitter.instruction("lea r8, [rsp + 32]");                              // pass local_pk out_ptr address
    ctx.emitter.instruction("lea r9, [rsp + 40]");                              // pass local_pk out_len address
    abi::emit_call_label(ctx.emitter, "__rt_get_string_context_option");

    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");                   // reload fd as the first TLS attach argument
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                    // pass peer_name pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // pass peer_name byte length
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // load local_cert byte length
    ctx.emitter.instruction("test rax, rax");                                   // is a client certificate path present?
    ctx.emitter.instruction(&format!("jz {}", plain_attach));                   // no client certificate selects plain TLS attach
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // load local_pk byte length
    ctx.emitter.instruction("test rax, rax");                                   // is a client private key path present?
    ctx.emitter.instruction(&format!("jz {}", plain_attach));                   // missing key selects plain TLS attach
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");                   // pass local_cert path pointer
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 24]");                    // pass local_cert path length
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // stage local_pk path length for the stack argument
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // pass local_pk path pointer
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve the seventh stack argument plus padding
    ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");                    // pass local_pk path length as the seventh argument
    abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_elephc_tls_attach_fd_client_cert_fn", 0); // load the mutual-TLS attach function pointer
    ctx.emitter.instruction("call r10");                                        // attach TLS with a client certificate
    ctx.emitter.instruction("add rsp, 16");                                     // release the seventh stack argument
    ctx.emitter.instruction(&format!("jmp {}", after_attach));                  // skip the default attach variant
    ctx.emitter.label(&plain_attach);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 64]");                   // reload fd as the first TLS attach argument
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                    // pass peer_name pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                    // pass peer_name byte length
    abi::emit_load_symbol_to_reg(ctx.emitter, "r9", "_elephc_tls_attach_fd_fn", 0); // load the default TLS attach function pointer
    ctx.emitter.instruction("call r9");                                         // attach TLS and return a session handle
    ctx.emitter.label(&after_attach);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                   // reload fd before releasing the spill storage
    abi::emit_release_temporary_stack(ctx.emitter, 64);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("cmp rax, 0");                                      // negative handles indicate TLS attach failure
    ctx.emitter.instruction(&format!("jl {}", fail_label));                     // return false when attach failed
    abi::emit_symbol_address(ctx.emitter, "r11", "_tls_sessions");
    ctx.emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rax");              // store the TLS session handle for this fd
    ctx.emitter.instruction("mov eax, 1");                                      // return true after successful TLS attach
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the failure result
    ctx.emitter.label(&fail_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false after TLS attach failure
}

/// Reserves temporary storage for `stream_get_contents` fd and length operands.
fn emit_stream_get_contents_frame_enter(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #32");                         // reserve aligned temporary storage for fd and length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 32");                             // reserve aligned temporary storage for fd and length
        }
    }
}

/// Releases temporary storage used by `stream_get_contents`.
fn emit_stream_get_contents_frame_leave(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add sp, sp, #32");                         // release stream_get_contents temporary storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 32");                             // release stream_get_contents temporary storage
        }
    }
}

/// Saves the currently loaded stream descriptor in the temporary frame.
fn emit_stream_get_contents_save_fd(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #0]");                        // save the stream descriptor across length and offset evaluation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // save the stream descriptor across length and offset evaluation
        }
    }
}

/// Saves the currently loaded length value in the temporary frame.
fn emit_stream_get_contents_save_length(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #8]");                        // save the requested byte count or unlimited sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // save the requested byte count or unlimited sentinel
        }
    }
}

/// Reloads the saved fd and releases the `stream_get_contents` temporary frame.
fn lower_stream_get_contents_reload_fd_and_leave_frame(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the stream descriptor for the read-all path
            emit_stream_get_contents_frame_leave(ctx);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the stream descriptor for the read-all path
            emit_stream_get_contents_frame_leave(ctx);
        }
    }
}

/// Applies the optional `stream_get_contents` seek before reading.
fn lower_stream_get_contents_seek(
    ctx: &mut FunctionContext<'_>,
    skip_seek: &str,
    wrap_seek: &str,
    seek_failed: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // a negative offset means no seek is requested
            ctx.emitter.instruction(&format!("b.lt {}", skip_seek));            // keep the current position for negative offsets
            ctx.emitter.instruction("mov x1, x0");                              // pass offset as the second seek argument
            ctx.emitter.instruction("mov x2, #0");                              // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the stream descriptor for seeking
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrap_seek));            // dispatch synthetic handles to wrapper stream_seek
            ctx.emitter.syscall(199);
            if ctx.emitter.platform.needs_cmp_before_error_branch() {
                ctx.emitter.instruction("cmp x0, #0");                          // Linux reports lseek failure as a negative result
            }
            ctx.emitter.instruction(&ctx.emitter.platform.branch_on_syscall_success(skip_seek)); // continue only when lseek succeeded
            ctx.emitter.instruction(&format!("b {}", seek_failed));             // failed seek makes stream_get_contents return false
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp x0, #0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("b.ne {}", seek_failed));          // failed wrapper seek makes stream_get_contents return false
            ctx.emitter.label(skip_seek);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // a negative offset means no seek is requested
            ctx.emitter.instruction(&format!("jl {}", skip_seek));              // keep the current position for negative offsets
            ctx.emitter.instruction("mov rsi, rax");                            // pass offset as the second seek argument
            ctx.emitter.instruction("xor edx, edx");                            // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the stream descriptor for seeking
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the handle is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrap_seek));             // dispatch synthetic handles to wrapper stream_seek
            ctx.emitter.instruction("call lseek");                              // seek the native stream descriptor
            ctx.emitter.instruction("cmp rax, 0");                              // test whether lseek returned a non-negative offset
            ctx.emitter.instruction(&format!("jl {}", seek_failed));            // failed seek makes stream_get_contents return false
            ctx.emitter.instruction(&format!("jmp {}", skip_seek));             // continue after a successful native seek
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp rax, 0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("jne {}", seek_failed));           // failed wrapper seek makes stream_get_contents return false
            ctx.emitter.label(skip_seek);
        }
    }
}

/// Performs a finite bounded read or jumps to read-all for null/negative length.
fn lower_stream_get_contents_bounded_or_all(
    ctx: &mut FunctionContext<'_>,
    read_all: &str,
    done: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // reload the requested byte count
            emit_branch_if_unlimited_length(ctx, "x9", "x10", read_all);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the stream descriptor for bounded reading
            ctx.emitter.instruction("mov x1, x9");                              // pass the finite byte count to the bounded helper
            emit_stream_get_contents_frame_leave(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents_bounded");
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("b {}", done));                    // bounded read completed successfully
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // reload the requested byte count
            emit_branch_if_unlimited_length(ctx, "r9", "r10", read_all);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the stream descriptor for bounded reading
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream descriptor to the bounded helper
            ctx.emitter.instruction("mov rsi, r9");                             // pass the finite byte count to the bounded helper
            emit_stream_get_contents_frame_leave(ctx);
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents_bounded");
            crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // bounded read completed successfully
        }
    }
}

/// Branches when a length value means "read until EOF".
fn emit_branch_if_unlimited_length(
    ctx: &mut FunctionContext<'_>,
    length_reg: &str,
    scratch_reg: &str,
    target_label: &str,
) {
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, NULL_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", length_reg, scratch_reg)); // test whether length is PHP null
            ctx.emitter.instruction(&format!("b.eq {}", target_label));         // null length means read until EOF
            ctx.emitter.instruction(&format!("cmp {}, #0", length_reg));        // test whether length is negative
            ctx.emitter.instruction(&format!("b.lt {}", target_label));         // negative length means read until EOF
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", length_reg, scratch_reg)); // test whether length is PHP null
            ctx.emitter.instruction(&format!("je {}", target_label));           // null length means read until EOF
            ctx.emitter.instruction(&format!("cmp {}, 0", length_reg));         // test whether length is negative
            ctx.emitter.instruction(&format!("jl {}", target_label));           // negative length means read until EOF
        }
    }
}

/// Creates the `stream_copy_to_stream` scratch frame after source/destination fd loading.
fn emit_stream_copy_frame_enter(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // preserve the destination descriptor while restoring the source
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("sub sp, sp, #48");                         // reserve source, destination, total, chunk, and max-length slots
            ctx.emitter.instruction("str x0, [sp, #0]");                        // save the source descriptor
            ctx.emitter.instruction("str x1, [sp, #8]");                        // save the destination descriptor
            ctx.emitter.instruction("str xzr, [sp, #16]");                      // initialize copied-byte total to zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // preserve the destination descriptor while restoring the source
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.instruction("sub rsp, 48");                             // reserve source, destination, total, chunk, and max-length slots
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rdi");            // save the source descriptor
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rsi");            // save the destination descriptor
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], 0");             // initialize copied-byte total to zero
        }
    }
}

/// Releases the `stream_copy_to_stream` scratch frame.
fn emit_stream_copy_frame_leave(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add sp, sp, #48");                         // release stream_copy_to_stream temporary storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rsp, 48");                             // release stream_copy_to_stream temporary storage
        }
    }
}

/// Saves the optional `stream_copy_to_stream` length or the unlimited sentinel.
fn materialize_stream_copy_length(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() >= 3 {
        let length = expect_operand(inst, 2)?;
        require_optional_int(
            ctx.load_value_to_result(length)?.codegen_repr(),
            "stream_copy_to_stream length",
        )?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), -1);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #32]");                       // save requested max bytes or the unlimited sentinel
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 32], rax");           // save requested max bytes or the unlimited sentinel
        }
    }
    Ok(())
}

/// Applies the optional `stream_copy_to_stream` source seek before copying.
fn lower_stream_copy_seek(
    ctx: &mut FunctionContext<'_>,
    skip_seek: &str,
    wrap_seek: &str,
    seek_failed: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // a negative offset means no seek is requested
            ctx.emitter.instruction(&format!("b.lt {}", skip_seek));            // keep the current source position for negative offsets
            ctx.emitter.instruction("mov x1, x0");                              // pass offset as the second seek argument
            ctx.emitter.instruction("mov x2, #0");                              // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the source descriptor for seeking
            ctx.emitter.instruction("mov w9, #0x4000");                         // materialize the high half of USER_WRAPPER_FD_BASE
            ctx.emitter.instruction("lsl w9, w9, #16");                         // form the synthetic wrapper fd base 0x40000000
            ctx.emitter.instruction("cmp x0, x9");                              // test whether the source is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("b.ge {}", wrap_seek));            // dispatch synthetic handles to wrapper stream_seek
            ctx.emitter.syscall(199);
            if ctx.emitter.platform.needs_cmp_before_error_branch() {
                ctx.emitter.instruction("cmp x0, #0");                          // Linux reports lseek failure as a negative result
            }
            ctx.emitter.instruction(&ctx.emitter.platform.branch_on_syscall_success(skip_seek)); // continue only when lseek succeeded
            ctx.emitter.instruction(&format!("b {}", seek_failed));             // failed native seek returns PHP false
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp x0, #0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("b.ne {}", seek_failed));          // failed wrapper seek returns PHP false
            ctx.emitter.label(skip_seek);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // a negative offset means no seek is requested
            ctx.emitter.instruction(&format!("jl {}", skip_seek));              // keep the current source position for negative offsets
            ctx.emitter.instruction("mov rsi, rax");                            // pass offset as the second seek argument
            ctx.emitter.instruction("xor edx, edx");                            // pass SEEK_SET as the third seek argument
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the source descriptor for seeking
            ctx.emitter.instruction("mov r9d, 0x40000000");                     // materialize USER_WRAPPER_FD_BASE for synthetic handles
            ctx.emitter.instruction("cmp rdi, r9");                             // test whether the source is a synthetic wrapper fd
            ctx.emitter.instruction(&format!("jge {}", wrap_seek));             // dispatch synthetic handles to wrapper stream_seek
            ctx.emitter.instruction("call lseek");                              // seek the native stream descriptor
            ctx.emitter.instruction("cmp rax, 0");                              // test whether lseek returned a non-negative offset
            ctx.emitter.instruction(&format!("jl {}", seek_failed));            // failed native seek returns PHP false
            ctx.emitter.instruction(&format!("jmp {}", skip_seek));             // continue after a successful native seek
            ctx.emitter.label(wrap_seek);
            abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
            ctx.emitter.instruction("cmp rax, 0");                              // wrapper fseek returns zero on success
            ctx.emitter.instruction(&format!("jne {}", seek_failed));           // failed wrapper seek returns PHP false
            ctx.emitter.label(skip_seek);
        }
    }
}

/// Copies source chunks into the destination and boxes the int-or-false result.
fn lower_stream_copy_loop_and_box(
    ctx: &mut FunctionContext<'_>,
    seek_failed: &str,
    boxed_done: &str,
) {
    let loop_label = ctx.next_label("scs_loop");
    let done_label = ctx.next_label("scs_done");
    let length_unlimited = ctx.next_label("scs_length_unlimited");
    let after_length_check = ctx.next_label("scs_after_length_check");
    let request_unlimited = ctx.next_label("scs_request_unlimited");
    let after_request = ctx.next_label("scs_after_request");
    let chunk_unlimited = ctx.next_label("scs_chunk_unlimited");
    let after_chunk = ctx.next_label("scs_after_chunk");
    ctx.emitter.label(&loop_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_stream_copy_loop_aarch64(
            ctx,
            &loop_label,
            &done_label,
            &length_unlimited,
            &after_length_check,
            &request_unlimited,
            &after_request,
            &chunk_unlimited,
            &after_chunk,
        ),
        Arch::X86_64 => lower_stream_copy_loop_x86_64(
            ctx,
            &loop_label,
            &done_label,
            &length_unlimited,
            &after_length_check,
            &request_unlimited,
            &after_request,
            &chunk_unlimited,
            &after_chunk,
        ),
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b {}", boxed_done));              // successful copy skips the seek-failure false result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jmp {}", boxed_done));            // successful copy skips the seek-failure false result
        }
    }
    ctx.emitter.label(seek_failed);
    emit_stream_copy_frame_leave(ctx);
    emit_bool_result(ctx, false);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(boxed_done);
}

/// Emits the AArch64 stream-copy read/write loop.
fn lower_stream_copy_loop_aarch64(
    ctx: &mut FunctionContext<'_>,
    loop_label: &str,
    done_label: &str,
    length_unlimited: &str,
    after_length_check: &str,
    request_unlimited: &str,
    after_request: &str,
    chunk_unlimited: &str,
    after_chunk: &str,
) {
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load copied-byte total so far
    ctx.emitter.instruction("ldr x10, [sp, #32]");                              // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "x10", "x11", length_unlimited);
    ctx.emitter.instruction("cmp x9, x10");                                     // test whether requested byte count is complete
    ctx.emitter.instruction(&format!("b.ge {}", done_label));                   // finish once length bytes have been copied
    ctx.emitter.instruction(&format!("b {}", after_length_check));              // continue with a finite request
    ctx.emitter.label(length_unlimited);
    ctx.emitter.label(after_length_check);
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // reload the source descriptor
    ctx.emitter.instruction("mov x1, #4096");                                   // request up to 4096 bytes by default
    ctx.emitter.instruction("ldr x10, [sp, #32]");                              // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "x10", "x11", request_unlimited);
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // reload copied-byte total so far
    ctx.emitter.instruction("sub x10, x10, x9");                                // compute remaining finite bytes
    ctx.emitter.instruction("cmp x10, x1");                                     // check whether remaining bytes are below the chunk cap
    ctx.emitter.instruction("csel x1, x10, x1, lt");                            // clamp the read request to remaining bytes
    ctx.emitter.instruction(&format!("b {}", after_request));                   // finite request size is ready
    ctx.emitter.label(request_unlimited);
    ctx.emitter.label(after_request);
    abi::emit_call_label(ctx.emitter, "__rt_fread");
    ctx.emitter.instruction(&format!("cbz x2, {}", done_label));                // stop when the source returns an empty chunk
    ctx.emitter.instruction("ldr x10, [sp, #32]");                              // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "x10", "x11", chunk_unlimited);
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // reload copied-byte total so far
    ctx.emitter.instruction("sub x10, x10, x9");                                // compute remaining finite bytes
    ctx.emitter.instruction("cmp x2, x10");                                     // check whether the read returned too many bytes
    ctx.emitter.instruction("csel x2, x2, x10, ls");                            // clamp wrapper chunks to remaining bytes
    ctx.emitter.instruction(&format!("b {}", after_chunk));                     // finite chunk length is ready
    ctx.emitter.label(chunk_unlimited);
    ctx.emitter.label(after_chunk);
    ctx.emitter.instruction("str x1, [sp, #24]");                               // save the owned chunk pointer for release
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load copied-byte total so far
    ctx.emitter.instruction("add x9, x9, x2");                                  // add this chunk length to the total
    ctx.emitter.instruction("str x9, [sp, #16]");                               // store updated copied-byte total
    ctx.emitter.instruction("ldr x0, [sp, #8]");                                // reload the destination descriptor
    abi::emit_call_label(ctx.emitter, "__rt_fwrite");
    ctx.emitter.instruction("ldr x0, [sp, #24]");                               // reload the owned chunk pointer
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // copy the next chunk
    ctx.emitter.label(done_label);
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // return the copied-byte total
    emit_stream_copy_frame_leave(ctx);
}

/// Emits the x86_64 stream-copy read/write loop.
fn lower_stream_copy_loop_x86_64(
    ctx: &mut FunctionContext<'_>,
    loop_label: &str,
    done_label: &str,
    length_unlimited: &str,
    after_length_check: &str,
    request_unlimited: &str,
    after_request: &str,
    chunk_unlimited: &str,
    after_chunk: &str,
) {
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // load copied-byte total so far
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "r9", "r10", length_unlimited);
    ctx.emitter.instruction("cmp r8, r9");                                      // test whether requested byte count is complete
    ctx.emitter.instruction(&format!("jge {}", done_label));                    // finish once length bytes have been copied
    ctx.emitter.instruction(&format!("jmp {}", after_length_check));            // continue with a finite request
    ctx.emitter.label(length_unlimited);
    ctx.emitter.label(after_length_check);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                    // reload the source descriptor
    ctx.emitter.instruction("mov rsi, 4096");                                   // request up to 4096 bytes by default
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "r9", "r10", request_unlimited);
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // reload copied-byte total so far
    ctx.emitter.instruction("sub r9, r8");                                      // compute remaining finite bytes
    ctx.emitter.instruction("cmp r9, rsi");                                     // check whether remaining bytes are below the chunk cap
    ctx.emitter.instruction("cmovl rsi, r9");                                   // clamp the read request to remaining bytes
    ctx.emitter.instruction(&format!("jmp {}", after_request));                 // finite request size is ready
    ctx.emitter.label(request_unlimited);
    ctx.emitter.label(after_request);
    abi::emit_call_label(ctx.emitter, "__rt_fread");
    ctx.emitter.instruction("test rdx, rdx");                                   // check whether the source returned an empty chunk
    ctx.emitter.instruction(&format!("jz {}", done_label));                     // stop when the source returns an empty chunk
    ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 32]");                    // load requested max byte count
    emit_branch_if_unlimited_length(ctx, "r9", "r10", chunk_unlimited);
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // reload copied-byte total so far
    ctx.emitter.instruction("sub r9, r8");                                      // compute remaining finite bytes
    ctx.emitter.instruction("cmp rdx, r9");                                     // check whether the read returned too many bytes
    ctx.emitter.instruction("cmova rdx, r9");                                   // clamp wrapper chunks to remaining bytes
    ctx.emitter.instruction(&format!("jmp {}", after_chunk));                   // finite chunk length is ready
    ctx.emitter.label(chunk_unlimited);
    ctx.emitter.label(after_chunk);
    ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");                   // save the owned chunk pointer for release
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                    // load copied-byte total so far
    ctx.emitter.instruction("add r8, rdx");                                     // add this chunk length to the total
    ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r8");                    // store updated copied-byte total
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the chunk pointer to fwrite
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                    // reload the destination descriptor
    abi::emit_call_label(ctx.emitter, "__rt_fwrite");
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                   // reload the owned chunk pointer
    abi::emit_call_label(ctx.emitter, "__rt_decref_any");
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // copy the next chunk
    ctx.emitter.label(done_label);
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // return the copied-byte total
    emit_stream_copy_frame_leave(ctx);
}

/// Verifies that a builtin call has a lowered operand count within an inclusive range.
fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    let actual = inst.operands.len();
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {}..={} args, got {}",
        name, min, max, actual
    )))
}

/// Loads the four-argument `stream_context_set_option` form into the runtime helper ABI.
fn lower_stream_context_set_option_4(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let wrapper = expect_operand(inst, 1)?;
    let option = expect_operand(inst, 2)?;
    let value = expect_operand(inst, 3)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_to_result(ctx, wrapper, "stream_context_set_option wrapper")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, option, "stream_context_set_option option")?;
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, value, "stream_context_set_option value")?;
            ctx.emitter.instruction("mov x4, x1");                              // pass the option value pointer as the fifth runtime argument
            ctx.emitter.instruction("mov x5, x2");                              // pass the option value length as the sixth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "x2", "x3");
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            load_string_to_result(ctx, wrapper, "stream_context_set_option wrapper")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, option, "stream_context_set_option option")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, value, "stream_context_set_option value")?;
            ctx.emitter.instruction("mov r8, rax");                             // pass the option value pointer as the fifth runtime argument
            ctx.emitter.instruction("mov r9, rdx");                             // pass the option value length as the sixth runtime argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdx", "rcx");
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_context_set_option_4");
    Ok(())
}

/// Stores an options heap pointer in the runtime's single stream-context slot.
fn store_stream_context_options(
    ctx: &mut FunctionContext<'_>,
    options: ValueId,
    clear_on_null: bool,
) -> Result<()> {
    if matches!(
        ctx.raw_value_php_type(options)?.codegen_repr(),
        PhpType::Void | PhpType::Never
    ) {
        if clear_on_null {
            clear_stream_context_options(ctx);
        }
        return Ok(());
    }
    ctx.load_value_to_result(options)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => store_stream_context_options_aarch64(ctx, clear_on_null),
        Arch::X86_64 => store_stream_context_options_x86_64(ctx, clear_on_null),
    }
    Ok(())
}

/// Stores the loaded AArch64 options pointer into `_stream_context_options`.
fn store_stream_context_options_aarch64(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    let skip_label = ctx.next_label("sctx_store_done");
    if clear_on_null {
        let zero_label = ctx.next_label("sctx_store_zero");
        ctx.emitter.instruction(&format!("cbz x0, {}", zero_label));            // clear the context slot when a null options value is passed
        abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
        ctx.emitter.instruction("str x0, [x9]");                                // persist the options heap pointer globally
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        ctx.emitter.instruction(&format!("b {}", skip_label));                  // skip the null-clearing fallback after retaining options
        ctx.emitter.label(&zero_label);
        clear_stream_context_options(ctx);
        ctx.emitter.label(&skip_label);
        return;
    }
    ctx.emitter.instruction(&format!("cbz x0, {}", skip_label));                // leave the context slot unchanged for null options
    abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
    ctx.emitter.instruction("str x0, [x9]");                                    // persist the options heap pointer globally
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip_label);
}

/// Stores the loaded x86_64 options pointer into `_stream_context_options`.
fn store_stream_context_options_x86_64(ctx: &mut FunctionContext<'_>, clear_on_null: bool) {
    let skip_label = ctx.next_label("sctx_store_done_x86");
    if clear_on_null {
        let zero_label = ctx.next_label("sctx_store_zero_x86");
        ctx.emitter.instruction("test rax, rax");                               // check whether the options pointer is null
        ctx.emitter.instruction(&format!("jz {}", zero_label));                 // clear the context slot when a null options value is passed
        abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
        ctx.emitter.instruction("mov QWORD PTR [r9], rax");                     // persist the options heap pointer globally
        ctx.emitter.instruction("mov rdi, rax");                                // pass the options pointer to incref
        abi::emit_call_label(ctx.emitter, "__rt_incref");
        ctx.emitter.instruction(&format!("jmp {}", skip_label));                // skip the null-clearing fallback after retaining options
        ctx.emitter.label(&zero_label);
        clear_stream_context_options(ctx);
        ctx.emitter.label(&skip_label);
        return;
    }
    ctx.emitter.instruction("test rax, rax");                                   // check whether the options pointer is null
    ctx.emitter.instruction(&format!("jz {}", skip_label));                     // leave the context slot unchanged for null options
    abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
    ctx.emitter.instruction("mov QWORD PTR [r9], rax");                         // persist the options heap pointer globally
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the options pointer to incref
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    ctx.emitter.label(&skip_label);
}

/// Clears the runtime's single stream-context options slot.
fn clear_stream_context_options(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str xzr, [x9]");                           // clear the persisted stream-context options pointer
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], 0");                   // clear the persisted stream-context options pointer
        }
    }
}

/// Emits an empty associative hash with Mixed values as the current result.
fn emit_empty_mixed_hash(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #1");                              // pass the empty hash's initial capacity
            ctx.emitter.instruction("mov x1, #7");                              // select Mixed values for the empty hash
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 1");                              // pass the empty hash's initial capacity
            ctx.emitter.instruction("mov esi, 7");                              // select Mixed values for the empty hash
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Emits an indexed string array from static names as the current result.
fn emit_static_string_array(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    let capacity = names.len().max(1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 16);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_static_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_static_string_array_fill_x86_64(ctx, names),
    }
}

/// Appends static strings to the current result array on AArch64.
fn emit_static_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the string array while appending entries
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown string array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final string array as the result
}

/// Appends static strings to the current result array on x86_64.
fn emit_static_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[&str]) {
    ctx.emitter.instruction("push rax");                                        // park the string array while appending entries
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the string array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown string array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final string array as the result
}

/// Emits a stream descriptor as the current integer/resource result.
fn emit_fd_result(ctx: &mut FunctionContext<'_>, fd: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), fd);
}

/// Emits a boolean scalar as the current integer result.
fn emit_bool_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Returns a literal string operand when the value was produced by `ConstStr`.
fn optional_const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "string literal operand has no data id",
        ));
    };
    Ok(Some(
        ctx.module
            .data
            .strings
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?,
    ))
}

/// Maps statically-known `php://` standard-stream URLs to native descriptors.
fn php_standard_stream_fd(path: &str) -> Option<i64> {
    match path {
        "php://stdin" | "php://input" => Some(0),
        "php://stdout" | "php://output" => Some(1),
        "php://stderr" => Some(2),
        _ => None,
    }
}

/// Recognizes `php://fd/N` URLs and returns the descriptor embedded in the URL.
fn php_fd_stream(path: &str) -> Option<i64> {
    let suffix = path.strip_prefix("php://fd/")?;
    suffix.parse::<i64>().ok()
}

/// Recognizes in-memory `php://` stream URLs backed by the temp-file helper.
fn is_php_memory_stream(path: &str) -> bool {
    path == "php://memory" || path == "php://temp" || path.starts_with("php://temp/")
}

/// Loads a path string, calls a stat helper, boxes int success or PHP false, and stores it.
fn lower_unary_path_stat_int_or_false(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    box_stat_int_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Loads a path, calls a stat-array helper, boxes array success or PHP false, and stores it.
fn lower_unary_path_stat_array_or_false(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_label: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, name)?;
    abi::emit_call_label(ctx.emitter, runtime_label);
    box_stat_array_or_false_result(ctx);
    store_if_result(ctx, inst)
}

/// Loads a resource or boxed resource handle into the target integer result register.
pub(super) fn load_stream_fd_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    function_name: &str,
) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    ctx.load_value_to_result(value)?;
    match raw_ty {
        PhpType::Resource(_) => Ok(()),
        PhpType::Mixed | PhpType::Union(_) => {
            emit_unbox_stream_or_type_error(ctx, function_name);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} stream argument PHP type {:?}",
            function_name, other
        ))),
    }
}

/// Stashes the Mixed box pointer of a resource operand on the stack so an
/// explicit closer (`fclose`/`pclose`/`closedir`) can stamp a release sentinel
/// into it after the handle is unboxed.
///
/// Returns `true` when a box was captured (Mixed/Union-typed operands, which are
/// the only ones that participate in scope cleanup) and `false` for unboxed
/// `Resource`-typed handles, which have no Mixed cell. The push keeps the stack
/// 16-byte aligned across the `__rt_mixed_unbox` call performed during unboxing;
/// the matching pop lives in `apply_resource_release_sentinel`.
fn capture_resource_box_for_release(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<bool> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    if !matches!(raw_ty, PhpType::Mixed | PhpType::Union(_)) {
        return Ok(false);
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(value, "x9")?;
            ctx.emitter.instruction("str x9, [sp, #-16]!");                     // stash the resource Mixed box pointer across the unbox call
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(value, "r11")?;
            ctx.emitter.instruction("sub rsp, 16");                             // reserve a 16-byte aligned slot for the stashed box pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp], r11");                // stash the resource Mixed box pointer across the unbox call
        }
    }
    Ok(true)
}

/// Pops the stashed Mixed box pointer and writes the `-1` release sentinel into
/// its low payload word so scope cleanup (`__rt_mixed_free_deep`) skips the
/// already-closed handle — preventing a second `close`/`pclose`/`closedir` on a
/// descriptor whose number may have been reused. A no-op when nothing was
/// captured. Preserves the close result already in the int result register.
fn apply_resource_release_sentinel(ctx: &mut FunctionContext<'_>, captured: bool) {
    if !captured {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp], #16");                       // restore the stashed resource Mixed box pointer
            ctx.emitter.instruction("mov x10, #-1");                            // -1 marks the resource handle as already released
            ctx.emitter.instruction("str x10, [x9, #8]");                       // overwrite the low payload word so scope cleanup skips it
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                // restore the stashed resource Mixed box pointer
            ctx.emitter.instruction("add rsp, 16");                             // release the stash slot
            ctx.emitter.instruction("mov QWORD PTR [r11 + 8], -1");             // overwrite the low payload word so scope cleanup skips it
        }
    }
}

/// Unboxes a Mixed stream resource or emits a fatal TypeError for non-resource values.
fn emit_unbox_stream_or_type_error(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let ok_label = ctx.next_label("stream_resource_ok");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #9");                              // check whether the boxed stream value uses the resource tag
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // continue only when the boxed value is a resource
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 9");                              // check whether the boxed stream value uses the resource tag
            ctx.emitter.instruction(&format!("je {}", ok_label));               // continue only when the boxed value is a resource
        }
    }
    emit_stream_type_error(ctx, function_name);
    ctx.emitter.label(&ok_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // expose the unboxed native stream fd as the integer result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // expose the unboxed native stream fd as the integer result
        }
    }
}

/// Dispatches a stream TypeError to the concrete PHP type name from the Mixed tag.
fn emit_stream_type_error(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let int_label = ctx.next_label("stream_type_error_int");
    let string_label = ctx.next_label("stream_type_error_string");
    let float_label = ctx.next_label("stream_type_error_float");
    let bool_label = ctx.next_label("stream_type_error_bool");
    let false_label = ctx.next_label("stream_type_error_false");
    let true_label = ctx.next_label("stream_type_error_true");
    let array_label = ctx.next_label("stream_type_error_array");
    let object_label = ctx.next_label("stream_type_error_object");
    let null_label = ctx.next_label("stream_type_error_null");
    let unknown_label = ctx.next_label("stream_type_error_unknown");

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the bad stream value unwrap to an integer?
            ctx.emitter.instruction(&format!("b.eq {}", int_label));            // report PHP's int-given stream TypeError
            ctx.emitter.instruction("cmp x0, #1");                              // did the bad stream value unwrap to a string?
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // report PHP's string-given stream TypeError
            ctx.emitter.instruction("cmp x0, #2");                              // did the bad stream value unwrap to a float?
            ctx.emitter.instruction(&format!("b.eq {}", float_label));          // report PHP's float-given stream TypeError
            ctx.emitter.instruction("cmp x0, #3");                              // did the bad stream value unwrap to a boolean?
            ctx.emitter.instruction(&format!("b.eq {}", bool_label));           // split boolean payloads into true/false diagnostics
            ctx.emitter.instruction("cmp x0, #4");                              // did the bad stream value unwrap to an indexed array?
            ctx.emitter.instruction(&format!("b.eq {}", array_label));          // report PHP's array-given stream TypeError
            ctx.emitter.instruction("cmp x0, #5");                              // did the bad stream value unwrap to an associative array?
            ctx.emitter.instruction(&format!("b.eq {}", array_label));          // associative arrays share PHP's array-given wording
            ctx.emitter.instruction("cmp x0, #6");                              // did the bad stream value unwrap to an object?
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // report PHP's object-given stream TypeError
            ctx.emitter.instruction("cmp x0, #8");                              // did the bad stream value unwrap to null?
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // report PHP's null-given stream TypeError
            ctx.emitter.instruction(&format!("b {}", unknown_label));           // fall back for unsupported boxed payload tags
            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("cmp x1, #0");                              // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // report PHP's false-given stream TypeError
            ctx.emitter.instruction(&format!("b {}", true_label));              // report PHP's true-given stream TypeError
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 0");                              // did the bad stream value unwrap to an integer?
            ctx.emitter.instruction(&format!("je {}", int_label));              // report PHP's int-given stream TypeError
            ctx.emitter.instruction("cmp rax, 1");                              // did the bad stream value unwrap to a string?
            ctx.emitter.instruction(&format!("je {}", string_label));           // report PHP's string-given stream TypeError
            ctx.emitter.instruction("cmp rax, 2");                              // did the bad stream value unwrap to a float?
            ctx.emitter.instruction(&format!("je {}", float_label));            // report PHP's float-given stream TypeError
            ctx.emitter.instruction("cmp rax, 3");                              // did the bad stream value unwrap to a boolean?
            ctx.emitter.instruction(&format!("je {}", bool_label));             // split boolean payloads into true/false diagnostics
            ctx.emitter.instruction("cmp rax, 4");                              // did the bad stream value unwrap to an indexed array?
            ctx.emitter.instruction(&format!("je {}", array_label));            // report PHP's array-given stream TypeError
            ctx.emitter.instruction("cmp rax, 5");                              // did the bad stream value unwrap to an associative array?
            ctx.emitter.instruction(&format!("je {}", array_label));            // associative arrays share PHP's array-given wording
            ctx.emitter.instruction("cmp rax, 6");                              // did the bad stream value unwrap to an object?
            ctx.emitter.instruction(&format!("je {}", object_label));           // report PHP's object-given stream TypeError
            ctx.emitter.instruction("cmp rax, 8");                              // did the bad stream value unwrap to null?
            ctx.emitter.instruction(&format!("je {}", null_label));             // report PHP's null-given stream TypeError
            ctx.emitter.instruction(&format!("jmp {}", unknown_label));         // fall back for unsupported boxed payload tags
            ctx.emitter.label(&bool_label);
            ctx.emitter.instruction("test rdi, rdi");                           // is the unboxed boolean payload false?
            ctx.emitter.instruction(&format!("je {}", false_label));            // report PHP's false-given stream TypeError
            ctx.emitter.instruction(&format!("jmp {}", true_label));            // report PHP's true-given stream TypeError
        }
    }

    emit_stream_type_error_case(ctx, function_name, "int", &int_label);
    emit_stream_type_error_case(ctx, function_name, "string", &string_label);
    emit_stream_type_error_case(ctx, function_name, "float", &float_label);
    emit_stream_type_error_case(ctx, function_name, "false", &false_label);
    emit_stream_type_error_case(ctx, function_name, "true", &true_label);
    emit_stream_type_error_case(ctx, function_name, "array", &array_label);
    emit_stream_type_error_case(ctx, function_name, "object", &object_label);
    emit_stream_type_error_case(ctx, function_name, "null", &null_label);
    emit_stream_type_error_case(ctx, function_name, "unknown", &unknown_label);
}

/// Emits one concrete stream TypeError branch and terminates the process.
fn emit_stream_type_error_case(
    ctx: &mut FunctionContext<'_>,
    function_name: &str,
    given_type: &str,
    case_label: &str,
) {
    ctx.emitter.label(case_label);
    let message = format!(
        "Fatal error: Uncaught TypeError: {}(): Argument #1 ($stream) must be of type resource, {} given\n",
        function_name, given_type
    );
    let (label, len) = ctx.data.add_string(message.as_bytes());
    emit_stream_type_error_and_exit(ctx, &label, len);
}

/// Emits a fatal stream TypeError diagnostic and terminates with exit status 1.
fn emit_stream_type_error_and_exit(ctx: &mut FunctionContext<'_>, label: &str, len: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the stream TypeError diagnostic to stderr
            ctx.emitter.adrp("x1", label);                                      // load the diagnostic string page
            ctx.emitter.add_lo12("x1", "x1", label);                            // resolve the diagnostic string address within the page
            ctx.emitter.instruction(&format!("mov x2, #{}", len));              // pass the diagnostic byte length to write()
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #1");                              // exit with status 1 after reporting the TypeError
            ctx.emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", label);
            ctx.emitter.instruction(&format!("mov edx, {}", len));              // pass the diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // write the stream TypeError diagnostic to stderr
            ctx.emitter.instruction("mov eax, 1");                              // select Linux x86_64 write syscall
            ctx.emitter.instruction("syscall");                                 // emit the stream TypeError diagnostic
            ctx.emitter.instruction("mov edi, 1");                              // exit with status 1 after reporting the TypeError
            ctx.emitter.instruction("mov eax, 60");                             // select Linux x86_64 exit syscall
            ctx.emitter.instruction("syscall");                                 // terminate the process after the fatal TypeError
        }
    }
}

/// Emits the ARM64 `fseek()` syscall path after fd, offset, and whence are staged.
fn lower_fseek_aarch64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    let wrapper_label = ctx.next_label("fseek_user_wrapper");
    let after_dispatch_label = ctx.next_label("fseek_after_dispatch");
    ctx.emitter.instruction("mov x2, x0");                                      // move whence into the third lseek syscall argument
    abi::emit_pop_reg(ctx.emitter, "x1");
    abi::emit_pop_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("mov w9, #0x4000");                                 // materialize the high half of USER_WRAPPER_FD_BASE
    ctx.emitter.instruction("lsl w9, w9, #16");                                 // form the synthetic wrapper fd base 0x40000000
    ctx.emitter.instruction("cmp x0, x9");                                      // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));                // dispatch synthetic handles to stream_seek
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.syscall(199);
    if ctx.emitter.platform.needs_cmp_before_error_branch() {
        ctx.emitter.instruction("cmp x0, #0");                                  // Linux reports lseek failure as a negative result
    }
    ctx.emitter.instruction(&ctx.emitter.platform.branch_on_syscall_success(success_label)); // continue only when lseek succeeds
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction("mov x0, #-1");                                     // fseek returns -1 when lseek fails
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip EOF reset after a failed seek
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    abi::emit_symbol_address(ctx.emitter, "x10", "_eof_flags");
    ctx.emitter.instruction("strb wzr, [x10, x9]");                             // clear EOF state for the successfully repositioned stream
    ctx.emitter.instruction("mov x0, #0");                                      // fseek returns 0 after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("b {}", after_dispatch_label));            // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.label(&after_dispatch_label);
}

/// Emits the Linux x86_64 `fseek()` libc path after fd, offset, and whence are staged.
fn lower_fseek_x86_64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    let wrapper_label = ctx.next_label("fseek_user_wrapper");
    let after_dispatch_label = ctx.next_label("fseek_after_dispatch");
    ctx.emitter.instruction("mov rdx, rax");                                    // move whence into the third lseek argument
    abi::emit_pop_reg(ctx.emitter, "rsi");
    abi::emit_pop_reg(ctx.emitter, "rdi");
    ctx.emitter.instruction("mov r9d, 0x40000000");                             // materialize USER_WRAPPER_FD_BASE for synthetic handles
    ctx.emitter.instruction("cmp rdi, r9");                                     // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("jge {}", wrapper_label));                 // dispatch synthetic handles to stream_seek
    abi::emit_push_reg(ctx.emitter, "rdi");
    ctx.emitter.instruction("call lseek");                                      // reposition the stream through libc lseek()
    ctx.emitter.instruction("cmp rax, 0");                                      // test whether lseek returned a non-negative offset
    ctx.emitter.instruction(&format!("jge {}", success_label));                 // continue only when lseek succeeds
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("mov rax, -1");                                     // fseek returns -1 when lseek fails
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip EOF reset after a failed seek
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("lea r11, [rip + _eof_flags]");                     // materialize the EOF-flag table base
    ctx.emitter.instruction("mov BYTE PTR [r11 + r10], 0");                     // clear EOF state for the successfully repositioned stream
    ctx.emitter.instruction("xor eax, eax");                                    // fseek returns 0 after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));          // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.label(&after_dispatch_label);
}

/// Emits the ARM64 `rewind()` syscall path and boolean result.
fn lower_rewind_aarch64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    let wrapper_label = ctx.next_label("rewind_user_wrapper");
    let after_dispatch_label = ctx.next_label("rewind_after_dispatch");
    ctx.emitter.instruction("mov w9, #0x4000");                                 // materialize the high half of USER_WRAPPER_FD_BASE
    ctx.emitter.instruction("lsl w9, w9, #16");                                 // form the synthetic wrapper fd base 0x40000000
    ctx.emitter.instruction("cmp x0, x9");                                      // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("b.ge {}", wrapper_label));                // dispatch synthetic handles to stream_seek
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("mov x1, #0");                                      // use offset 0 for rewind
    ctx.emitter.instruction("mov x2, #0");                                      // use SEEK_SET for rewind
    ctx.emitter.syscall(199);
    if ctx.emitter.platform.needs_cmp_before_error_branch() {
        ctx.emitter.instruction("cmp x0, #0");                                  // Linux reports lseek failure as a negative result
    }
    ctx.emitter.instruction(&ctx.emitter.platform.branch_on_syscall_success(success_label)); // continue only when rewind succeeds
    abi::emit_pop_reg(ctx.emitter, "x9");
    ctx.emitter.instruction("mov x0, #0");                                      // rewind returns false when lseek fails
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip EOF reset after a failed rewind
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "x9");
    abi::emit_symbol_address(ctx.emitter, "x10", "_eof_flags");
    ctx.emitter.instruction("strb wzr, [x10, x9]");                             // clear EOF state after rewinding the stream
    ctx.emitter.instruction("mov x0, #1");                                      // rewind returns true after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("b {}", after_dispatch_label));            // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    ctx.emitter.instruction("mov x1, #0");                                      // pass offset 0 to wrapper stream_seek
    ctx.emitter.instruction("mov x2, #0");                                      // pass SEEK_SET to wrapper stream_seek
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.instruction("cmp x0, #0");                                      // wrapper fseek returns zero on success
    ctx.emitter.instruction("cset x0, eq");                                     // rewind returns true only when wrapper seek succeeded
    ctx.emitter.label(&after_dispatch_label);
}

/// Emits the Linux x86_64 `rewind()` libc path and boolean result.
fn lower_rewind_x86_64(
    ctx: &mut FunctionContext<'_>,
    success_label: &str,
    done_label: &str,
) {
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the stream fd to libc lseek()
    let wrapper_label = ctx.next_label("rewind_user_wrapper");
    let after_dispatch_label = ctx.next_label("rewind_after_dispatch");
    ctx.emitter.instruction("mov r9d, 0x40000000");                             // materialize USER_WRAPPER_FD_BASE for synthetic handles
    ctx.emitter.instruction("cmp rdi, r9");                                     // test whether this stream is a userspace-wrapper handle
    ctx.emitter.instruction(&format!("jge {}", wrapper_label));                 // dispatch synthetic handles to stream_seek
    abi::emit_push_reg(ctx.emitter, "rdi");
    ctx.emitter.instruction("xor esi, esi");                                    // use offset 0 for rewind
    ctx.emitter.instruction("xor edx, edx");                                    // use SEEK_SET for rewind
    ctx.emitter.instruction("call lseek");                                      // rewind the stream through libc lseek()
    ctx.emitter.instruction("cmp rax, 0");                                      // test whether lseek returned a non-negative offset
    ctx.emitter.instruction(&format!("jge {}", success_label));                 // continue only when rewind succeeds
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("xor eax, eax");                                    // rewind returns false when lseek fails
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip EOF reset after a failed rewind
    ctx.emitter.label(success_label);
    abi::emit_pop_reg(ctx.emitter, "r10");
    ctx.emitter.instruction("lea r11, [rip + _eof_flags]");                     // materialize the EOF-flag table base
    ctx.emitter.instruction("mov BYTE PTR [r11 + r10], 0");                     // clear EOF state after rewinding the stream
    ctx.emitter.instruction("mov rax, 1");                                      // rewind returns true after a successful seek
    ctx.emitter.label(done_label);
    ctx.emitter.instruction(&format!("jmp {}", after_dispatch_label));          // skip wrapper stream_seek after the native path
    ctx.emitter.label(&wrapper_label);
    ctx.emitter.instruction("xor esi, esi");                                    // pass offset 0 to wrapper stream_seek
    ctx.emitter.instruction("xor edx, edx");                                    // pass SEEK_SET to wrapper stream_seek
    abi::emit_call_label(ctx.emitter, "__rt_user_wrapper_fseek");
    ctx.emitter.instruction("cmp rax, 0");                                      // wrapper fseek returns zero on success
    ctx.emitter.instruction("sete al");                                         // mark wrapper seek success as true
    ctx.emitter.instruction("movzx eax, al");                                   // widen rewind bool result
    ctx.emitter.label(&after_dispatch_label);
}

/// Materializes `file_put_contents` arguments for the ARM64 runtime ABI.
fn lower_file_put_contents_arm64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
    helper: &str,
) -> Result<()> {
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    load_string_to_result(ctx, data, "file_put_contents data")?;
    ctx.emitter.instruction("mov x3, x1");                                      // pass the data pointer in the runtime helper's second string slot
    ctx.emitter.instruction("mov x4, x2");                                      // pass the data length in the runtime helper's second string slot
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_call_label(ctx.emitter, helper);
    Ok(())
}

/// Materializes `file_put_contents` arguments for the Linux x86_64 runtime ABI.
fn lower_file_put_contents_x86_64(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
    helper: &str,
) -> Result<()> {
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    load_string_to_result(ctx, data, "file_put_contents data")?;
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the data pointer while the filename remains on the temporary stack
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass the data length while the filename remains on the temporary stack
    abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_call_label(ctx.emitter, helper);
    Ok(())
}

/// Materializes and hashes `hash_file()` arguments on AArch64.
fn lower_hash_file_aarch64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fail: &str,
    done: &str,
) -> Result<()> {
    super::strings::load_string_arg_to_regs(ctx, inst, 0, "hash_file", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the algorithm string while materializing the filename
    super::strings::load_string_arg_to_regs(ctx, inst, 1, "hash_file", "x1", "x2")?;
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the filename string while materializing the binary flag
    super::strings::materialize_truthy_flag(ctx, inst, 2, "hash_file")?;
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // preserve the raw-output flag after all PHP arguments are materialized
    ctx.emitter.instruction("ldp x1, x2, [sp, #16]");                           // reload the filename string for the file reader helper
    abi::emit_call_label(ctx.emitter, "__rt_file_get_contents_maybe_url");
    ctx.emitter.instruction(&format!("cbz x1, {}", fail));                      // null file bytes mean the file could not be read
    ctx.emitter.instruction("mov x3, x1");                                      // pass file bytes as the hash data pointer
    ctx.emitter.instruction("mov x4, x2");                                      // pass file byte count as the hash data length
    ctx.emitter.instruction("ldr x5, [sp]");                                    // restore the raw-output flag into the hash ABI register
    ctx.emitter.instruction("ldp x1, x2, [sp, #32]");                           // restore the hash algorithm string
    ctx.emitter.instruction("add sp, sp, #48");                                 // discard saved algorithm, filename, and flag slots
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    ctx.emitter.instruction(&format!("b {}", done));                            // proceed to box the digest string
    ctx.emitter.label(fail);
    ctx.emitter.instruction("add sp, sp, #48");                                 // discard saved hash_file arguments on the failure path
    ctx.emitter.instruction("mov x1, #0");                                      // null pointer asks the common boxer to return PHP false
    ctx.emitter.instruction("mov x2, #0");                                      // clear the unused string length for the failure sentinel
    ctx.emitter.label(done);
    Ok(())
}

/// Materializes and hashes `hash_file()` arguments on Linux x86_64.
fn lower_hash_file_x86_64(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    fail: &str,
    done: &str,
) -> Result<()> {
    super::strings::load_string_arg_to_regs(ctx, inst, 0, "hash_file", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    super::strings::load_string_arg_to_regs(ctx, inst, 1, "hash_file", "rax", "rdx")?;
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    super::strings::materialize_truthy_flag(ctx, inst, 2, "hash_file")?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the filename pointer for the file reader helper
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");                   // reload the filename length for the file reader helper
    abi::emit_call_label(ctx.emitter, "__rt_file_get_contents_maybe_url");
    ctx.emitter.instruction("test rax, rax");                                   // null file bytes mean the file could not be read
    ctx.emitter.instruction(&format!("jz {}", fail));                           // return PHP false for unreadable files
    ctx.emitter.instruction("mov rdi, rax");                                    // pass file bytes as the hash data pointer
    ctx.emitter.instruction("mov rsi, rdx");                                    // pass file byte count as the hash data length
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // restore the raw-output flag into the hash ABI register
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                   // restore the algorithm string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                   // restore the algorithm string length
    ctx.emitter.instruction("add rsp, 48");                                     // discard saved algorithm, filename, and flag slots
    crate::codegen::builtins::hash_crypto::publish_elephc_crypto_function_pointers(
        ctx.emitter,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // proceed to box the digest string
    ctx.emitter.label(fail);
    ctx.emitter.instruction("add rsp, 48");                                     // discard saved hash_file arguments on the failure path
    ctx.emitter.instruction("xor eax, eax");                                    // null pointer asks the common boxer to return PHP false
    ctx.emitter.instruction("xor edx, edx");                                    // clear the unused string length for the failure sentinel
    ctx.emitter.label(done);
    Ok(())
}

/// Boxes a raw stream string slice or EOF result into Mixed string-or-false form.
fn box_stream_string_or_false_on_empty_result(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x2, #0");                              // test whether the stream read produced bytes
            ctx.emitter.instruction(&format!("b.le {}", false_label));          // box false when the stream hit EOF or read failure
            ctx.emitter.instruction("mov x0, #1");                              // select runtime tag 1 for the stream string
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for stream EOF
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdx, 0");                              // test whether the stream read produced bytes
            ctx.emitter.instruction(&format!("jle {}", false_label));           // box false when the stream hit EOF or read failure
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream string pointer as the Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the stream string length as the Mixed high payload word
            ctx.emitter.instruction("mov eax, 1");                              // select runtime tag 1 for the stream string
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for stream EOF
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a non-negative stream descriptor as a PHP resource or false on failure.
///
/// The resource is tagged with scope-cleanup kind 1 (native stream fd, closed via
/// `close()` at scope exit). Callers whose handle needs a different destructor use
/// `box_stream_fd_or_false_result_kind` instead.
fn box_stream_fd_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    box_stream_fd_or_false_result_kind(ctx, label_prefix, 1);
}

/// Boxes a non-negative descriptor as a PHP resource (or false on failure) and
/// records the scope-cleanup `kind` in the Mixed high payload word so
/// `__rt_mixed_free_deep` dispatches the right destructor: 1 = native stream fd
/// (`close`), 3 = `popen` pipe (`__rt_pclose`), 4 = `opendir` stream
/// (`__rt_closedir`).
fn box_stream_fd_or_false_result_kind(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
    kind: u64,
) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the stream helper returned a negative descriptor
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false when stream creation failed
            ctx.emitter.instruction("mov x1, x0");                              // pass the native stream fd as the Mixed low payload word
            ctx.emitter.instruction(&format!("mov x2, #{}", kind));             // resource-kind subtype in the Mixed high word (1=fd,3=popen,4=dir)
            ctx.emitter.instruction("mov x0, #9");                              // select runtime tag 9 for a stream resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the resource result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for fopen failure
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the stream helper returned a negative descriptor
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false when stream creation failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the native stream fd as the Mixed low payload word
            ctx.emitter.instruction(&format!("mov esi, {}", kind));             // resource-kind subtype in the Mixed high word (1=fd,3=popen,4=dir)
            ctx.emitter.instruction("mov eax, 9");                              // select runtime tag 9 for a stream resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the resource result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for fopen failure
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a socket-pair array result or PHP false as `Mixed`.
fn box_stream_socket_pair_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stream_socket_pair_false");
    let done_label = ctx.next_label("stream_socket_pair_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // null pointer means socketpair failed
            ctx.emitter.instruction("mov x1, #9");                              // resource tag: each fd becomes Mixed(resource)
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
            );
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the false boxing path after success
            ctx.emitter.label(&false_label);
            emit_bool_result(ctx, false);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // null pointer means socketpair failed
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when socketpair failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the descriptor array to array_to_mixed
            ctx.emitter.instruction("mov esi, 9");                              // resource tag: each fd becomes Mixed(resource)
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
            );
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the false boxing path after success
            ctx.emitter.label(&false_label);
            emit_bool_result(ctx, false);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes an owned runtime string result into PHP `string|false` Mixed form.
pub(super) fn box_owned_string_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // branch when the runtime returned a null string pointer for failure
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov x9, #1");                              // select runtime tag 1 for a string Mixed payload
            ctx.emitter.instruction("str x9, [x0]");                            // store the string tag in the Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "x10", "x11");
            ctx.emitter.instruction("stp x10, x11, [x0, #8]");                  // store the owned string pointer and length in the Mixed cell
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned a null string pointer for failure
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box false when the runtime string helper failed
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("mov rax, 24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov r10, 1");                              // select runtime tag 1 for a string Mixed payload
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the string tag in the Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "r10", "r11");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned string pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r11");           // store the owned string length in the Mixed cell
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a raw `readfile()` byte count into PHP `int|false` Mixed form.
fn box_readfile_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("readfile_false");
    let done_label = ctx.next_label("readfile_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, #-2");                             // runtime sentinel -2 means the file could not be opened
            ctx.emitter.instruction("cmp x0, x9");                              // test whether readfile failed before streaming began
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // box PHP false for open failure
            ctx.emitter.instruction("mov x1, x0");                              // pass the streamed byte count as the Mixed integer payload
            ctx.emitter.instruction("mov x2, #0");                              // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for readfile failure
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -2");                             // runtime sentinel -2 means the file could not be opened
            ctx.emitter.instruction(&format!("je {}", false_label));            // box PHP false for open failure
            ctx.emitter.instruction("mov rdi, rax");                            // pass the streamed byte count as the Mixed integer payload
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for readfile failure
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a non-negative integer result or PHP `false` for the `-1` sentinel.
fn box_negative_int_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the runtime returned the not-found sentinel
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false when the lookup did not find an entry
            ctx.emitter.instruction("mov x1, x0");                              // pass the lookup integer as the Mixed low payload word
            ctx.emitter.instruction("mov x2, #0");                              // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned the not-found sentinel
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false when the lookup did not find an entry
            ctx.emitter.instruction("mov rdi, rax");                            // pass the lookup integer as the Mixed low payload word
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a freshly owned pathinfo hash as a PHP associative-array Mixed cell.
fn box_owned_pathinfo_array_as_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov x9, #5");                              // select runtime tag 5 for an associative-array Mixed payload
            ctx.emitter.instruction("str x9, [x0]");                            // store the associative-array tag in the Mixed cell
            abi::emit_pop_reg(ctx.emitter, "x10");
            ctx.emitter.instruction("str x10, [x0, #8]");                       // store the owned pathinfo hash pointer in the Mixed cell
            ctx.emitter.instruction("str xzr, [x0, #16]");                      // associative-array Mixed payloads do not use a high word
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, 24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax], 5");                  // select runtime tag 5 for an associative-array Mixed payload
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned pathinfo hash pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], 0");             // associative-array Mixed payloads do not use a high word
        }
    }
}

/// Boxes the raw stat integer payload into PHP `int|false` Mixed form.
fn box_stat_int_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_int_false");
    let done_label = ctx.next_label("stat_int_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box PHP false when the runtime success flag is unset
            ctx.emitter.instruction("mov x2, xzr");                             // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x1, x0");                              // pass the stat integer as the Mixed low payload word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // test whether the runtime success flag is set
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when the stat helper failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stat integer as the Mixed low payload word
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes the raw stat hash payload into PHP `array|false` Mixed form.
fn box_stat_array_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_array_false");
    let done_label = ctx.next_label("stat_array_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // branch when the stat runtime returned a null hash pointer
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov x9, #5");                              // select runtime tag 5 for an associative-array Mixed payload
            ctx.emitter.instruction("str x9, [x0]");                            // store the associative-array tag in the Mixed cell
            abi::emit_pop_reg(ctx.emitter, "x10");
            ctx.emitter.instruction("str x10, [x0, #8]");                       // store the owned stat hash pointer in the Mixed cell
            ctx.emitter.instruction("str xzr, [x0, #16]");                      // associative-array Mixed payloads do not use a high word
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the array Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the stat runtime returned a null hash pointer
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box false when the runtime stat-array helper failed
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, 24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax], 5");                  // select runtime tag 5 for an associative-array Mixed payload
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned stat hash pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], 0");             // associative-array Mixed payloads do not use a high word
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the array Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes the raw stat string slice into PHP `string|false` Mixed form.
fn box_stat_string_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_string_false");
    let done_label = ctx.next_label("stat_string_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box PHP false when the runtime returned a null string pointer
            ctx.emitter.instruction("mov x0, #1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned a null string pointer
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when filetype failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the filetype string pointer as the Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the filetype string length as the Mixed high payload word
            ctx.emitter.instruction("mov eax, 1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Loads a string SSA value into the target string result registers, coercing
/// any scalar to its PHP string form. Shared with `system::lower_header`.
pub(super) fn load_string_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            Ok(())
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa");
            Ok(())
        }
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            Ok(())
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            lower_loaded_bool_to_string(ctx);
            Ok(())
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            lower_loaded_tagged_scalar_to_string(ctx);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            emit_empty_string_result(ctx);
            Ok(())
        }
        PhpType::Resource(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_resource_to_string");
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            context,
            other
        ))),
    }
}

/// Converts the currently loaded boolean result to PHP string result registers.
fn lower_loaded_bool_to_string(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("io_bool_to_str_false");
    let done_label = ctx.next_label("io_bool_to_str_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-string fallback after true conversion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the boolean payload is false
            ctx.emitter.instruction(&format!("je {}", false_label));            // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-string fallback after true conversion
        }
    }
    ctx.emitter.label(&false_label);
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
}

/// Converts the currently loaded tagged scalar result to PHP string result registers.
fn lower_loaded_tagged_scalar_to_string(ctx: &mut FunctionContext<'_>) {
    let null_label = ctx.next_label("io_tagged_to_str_null");
    let done_label = ctx.next_label("io_tagged_to_str_done");
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_label);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
}

/// Publishes PHP's empty-string result in the target string ABI registers.
fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Verifies that a path builtin scalar argument has the supported integer representation.
fn require_int(ty: PhpType, name: &str) -> Result<()> {
    if ty == PhpType::Int {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}

/// Verifies that an optional integer argument is either `int` or literal `null`.
fn require_optional_int(ty: PhpType, name: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}

/// Verifies that a scalar flag argument has an integer-like representation.
fn require_int_or_bool(ty: PhpType, name: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        name,
        ty
    )))
}

/// Verifies that a CSV fields argument has the supported indexed string-array layout.
fn require_string_array(ty: PhpType, name: &str) -> Result<()> {
    match ty {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Str => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name,
            other
        ))),
    }
}
