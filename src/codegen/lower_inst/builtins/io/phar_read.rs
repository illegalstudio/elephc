//! Purpose:
//! File reads, PHAR bridge publication, hashing, and readline lowering.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `file_get_contents(path)` and boxes the runtime string-or-false result.
/// php-src's `ValueError` for a negative `file_get_contents()` `$length`.
const FILE_GET_CONTENTS_NEGATIVE_LENGTH_MESSAGE: &str =
    "file_get_contents(): Argument #5 ($length) must be greater than or equal to 0";

/// Lowers `file_get_contents(path, use_include_path?, context?, offset?, length?)` and boxes the
/// runtime string-or-false result.
///
/// The full read runs first and `$offset`/`$length` then trim the owned buffer in place through
/// `__rt_file_get_contents_range`, which reproduces what PHP's seek-then-read produces for a
/// seekable stream while keeping the allocation and the copy bounded by the same byte count.
/// The negative-`$length` `ValueError` is raised BEFORE the read, exactly like php-src, so a
/// missing file plus a negative length still throws instead of warning.
pub(crate) fn lower_file_get_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "file_get_contents", 1, 5)?;
    require_absent_stream_context(ctx, inst, 2, "file_get_contents")?;
    let range = FileReadRange::from_operands(ctx, inst, 3, 4)?;
    range.emit_negative_length_guard(ctx, FILE_GET_CONTENTS_NEGATIVE_LENGTH_MESSAGE)?;
    emit_file_get_contents_bytes(ctx, inst, range.is_active())?;
    range.emit(ctx, "file_get_contents")?;
    box_owned_string_or_false_result(ctx, "fgc");
    store_if_result(ctx, inst)
}

/// Emits the unsliced `file_get_contents()` read, leaving the bytes in the string result registers.
///
/// `persist_literal_bytes` is set when a `$offset`/`$length` window follows: the literal `phar://`
/// shortcut answers with a pointer into read-only `.data`, which the in-place range trim must never
/// move or free, so those bytes are copied into an owned string first.
fn emit_file_get_contents_bytes(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    persist_literal_bytes: bool,
) -> Result<()> {
    let path = expect_operand(inst, 0)?;
    let path_literal = optional_const_string_operand(ctx, path)?;
    if let Some(path_literal) = path_literal.as_deref() {
        if path_literal.starts_with("phar://") {
            emit_literal_phar_file_get_contents_bytes(ctx, path_literal, persist_literal_bytes);
            return Ok(());
        }
        if path_literal == "php://input" {
            // file_get_contents('php://input'): under --web `__rt_php_input` copies
            // the captured request body into an owned string; in a non-web build it
            // returns a null pointer so the result boxes to PHP false.
            abi::emit_call_label(ctx.emitter, "__rt_php_input");
            return Ok(());
        }
    }
    // A literal with no `://` cannot name a stream wrapper: PHP's wrapper grammar requires the
    // separator, including for wrappers a program registers itself. Entering at the phar level
    // therefore skips only tests that provably cannot succeed — and takes the URL reader out of
    // the call graph, so a program whose reads are all constant local paths stops carrying
    // `socket`, `connect`, `bind` and the resolver. Measured on `file_get_contents("/etc/hosts")`:
    // 11 distinct syscalls before, against 3 for `<?php echo 1;`.
    //
    // The test is "contains no `://`", never a list of known schemes. A list would silently open
    // a file literally named `compress.zlib://x` on the day a scheme is missing from it; this way
    // an unrecognised scheme still reaches the multiplexer and behaves as it does today.
    let literal_cannot_be_a_wrapper = path_literal
        .as_deref()
        .is_some_and(|literal| !literal.contains("://"));
    if path_literal.is_none() {
        publish_dynamic_phar_function_pointers(ctx);
    }
    load_string_to_result(ctx, path, "file_get_contents filename")?;
    abi::emit_call_label(
        ctx.emitter,
        if literal_cannot_be_a_wrapper {
            "__rt_file_get_contents_maybe_phar"
        } else {
            "__rt_file_get_contents_maybe_url"
        },
    );
    Ok(())
}

/// Rejects a non-null `$context` argument instead of silently ignoring the stream context.
///
/// elephc has no stream-context plumbing on the read path, so honoring a real context is
/// impossible. An omitted argument, a literal `null`, and the registry's `null` default all
/// materialize a statically null operand and are accepted; anything else is a compile error
/// naming the parameter rather than a read that quietly drops the caller's options.
fn require_absent_stream_context(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
) -> Result<()> {
    let Some(context) = inst.operands.get(index).copied() else {
        return Ok(());
    };
    if operand_is_statically_null(ctx, context)? {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{}() $context argument: elephc cannot honor a stream context on this read, pass null",
        name
    )))
}

/// Reports whether an operand is the PHP `null` value at compile time.
///
/// Covers both spellings the argument planner produces: a `ConstNull` instruction for a literal
/// `null` argument or a filled-in `null` default, and a `Void`-typed value for an operand the
/// planner materialized without a concrete constant.
fn operand_is_statically_null(ctx: &FunctionContext<'_>, value: ValueId) -> Result<bool> {
    if matches!(ctx.value_php_type(value)?.codegen_repr(), PhpType::Void) {
        return Ok(true);
    }
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(false);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(inst_ref.op == Op::ConstNull)
}

/// The `$offset`/`$length` window a one-shot file read applies to the bytes it produced.
///
/// Both operands are optional: `None` means the PHP call omitted the argument, and a statically
/// absent `$length` (omitted or the `null` default) means "to the end of the data".
struct FileReadRange {
    /// The `$offset` operand, when the call passed one.
    offset: Option<ValueId>,
    /// The `$length` operand, when the call passed one.
    length: Option<ValueId>,
    /// Whether `$length` is known at compile time to be absent (omitted or literal `null`).
    length_statically_absent: bool,
}

impl FileReadRange {
    /// Reads the optional `$offset`/`$length` operands at the given positions.
    fn from_operands(
        ctx: &mut FunctionContext<'_>,
        inst: &Instruction,
        offset_index: usize,
        length_index: usize,
    ) -> Result<Self> {
        let offset = inst.operands.get(offset_index).copied();
        let length = inst.operands.get(length_index).copied();
        let length_statically_absent = match length {
            None => true,
            Some(length) => matches!(ctx.value_php_type(length)?.codegen_repr(), PhpType::Void),
        };
        Ok(Self {
            offset,
            length,
            length_statically_absent,
        })
    }

    /// Reports whether any trimming has to happen at run time.
    ///
    /// A call that passed neither argument keeps the untouched read result, so no range helper
    /// call is emitted at all and the 1-argument lowering is byte-for-byte what it was.
    fn is_active(&self) -> bool {
        self.offset.is_some() || self.length.is_some()
    }

    /// Raises php-src's negative-`$length` `ValueError` before the read is attempted.
    ///
    /// A statically absent `$length` needs no guard. A boxed `Mixed` `null` casts to `0`, which
    /// passes the guard, so a runtime `null` still reads to the end instead of throwing.
    fn emit_negative_length_guard(
        &self,
        ctx: &mut FunctionContext<'_>,
        message: &str,
    ) -> Result<()> {
        if self.length_statically_absent {
            return Ok(());
        }
        let length = self.length.expect("length operand present");
        resolve_int_operand_to_result(ctx, length, "file read length")?;
        let reg = abi::int_result_reg(ctx.emitter);
        super::super::exceptions::emit_value_error_unless(
            ctx,
            super::super::exceptions::ValueGuard::SignedAtLeast(reg, 0),
            message,
        );
        Ok(())
    }

    /// Trims the string currently in the string result registers to the requested window.
    ///
    /// The read result is spilled across the integer resolutions because unboxing a `Mixed`
    /// argument calls `__rt_mixed_cast_int`, which clobbers the caller-saved registers the
    /// pointer/length pair lives in.
    fn emit(&self, ctx: &mut FunctionContext<'_>, name: &str) -> Result<()> {
        if !self.is_active() {
            return Ok(());
        }
        let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
        abi::emit_push_reg_pair(ctx.emitter, text_ptr, text_len);
        self.resolve_offset(ctx, name)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        self.resolve_length_present(ctx)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        self.resolve_length(ctx, name)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("mov x4, x0");                          // pass the resolved byte length as the range helper's fourth argument
                abi::emit_pop_reg(ctx.emitter, "x5");                           // restore the length-present flag into the fifth range argument
                abi::emit_pop_reg(ctx.emitter, "x3");                           // restore the resolved byte offset into the third range argument
                abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rsi, rax");                        // pass the resolved byte length as the range helper's fourth argument
                abi::emit_pop_reg(ctx.emitter, "rcx");                          // restore the length-present flag into the fifth range argument
                abi::emit_pop_reg(ctx.emitter, "rdi");                          // restore the resolved byte offset into the third range argument
                abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_file_get_contents_range");
        Ok(())
    }

    /// Resolves `$offset` into the integer result register, defaulting an omitted one to `0`.
    fn resolve_offset(&self, ctx: &mut FunctionContext<'_>, name: &str) -> Result<()> {
        match self.offset {
            None => {
                let reg = abi::int_result_reg(ctx.emitter);
                abi::emit_load_int_immediate(ctx.emitter, reg, 0);
                Ok(())
            }
            Some(offset) => {
                resolve_int_operand_to_result(ctx, offset, &format!("{} offset", name))
            }
        }
    }

    /// Resolves `$length` into the integer result register, using `0` for an absent one.
    fn resolve_length(&self, ctx: &mut FunctionContext<'_>, name: &str) -> Result<()> {
        if self.length_statically_absent {
            let reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, reg, 0);
            return Ok(());
        }
        let length = self.length.expect("length operand present");
        resolve_int_operand_to_result(ctx, length, &format!("{} length", name))
    }

    /// Resolves the length-present flag PHP's `?int $length` needs.
    ///
    /// `null` means "read to the end", and every real `i64` — including `0` — is a genuine byte
    /// count, so the helper cannot recognise the absent case from the length value alone.
    fn resolve_length_present(&self, ctx: &mut FunctionContext<'_>) -> Result<()> {
        let reg = abi::int_result_reg(ctx.emitter);
        if self.length_statically_absent {
            abi::emit_load_int_immediate(ctx.emitter, reg, 0);
            return Ok(());
        }
        let length = self.length.expect("length operand present");
        if !matches!(
            ctx.value_php_type(length)?.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        ) {
            abi::emit_load_int_immediate(ctx.emitter, reg, 1);
            return Ok(());
        }
        ctx.load_value_to_result(length)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #8");                          // runtime tag 8 marks a boxed PHP null length argument
                ctx.emitter.instruction("cset x0, ne");                         // report a length only when the boxed payload is not null
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("cmp rax, 8");                          // runtime tag 8 marks a boxed PHP null length argument
                ctx.emitter.instruction("setne al");                            // report a length only when the boxed payload is not null
                ctx.emitter.instruction("movzx rax, al");                       // widen the length-present flag to a full integer argument word
            }
        }
        Ok(())
    }
}

/// Publishes bridge/decompressor entry points into runtime slots used by
/// dynamic `phar://` reads.
pub(super) fn publish_dynamic_phar_function_pointers(ctx: &mut FunctionContext<'_>) {
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
pub(super) fn publish_phar_bridge_entries(ctx: &mut FunctionContext<'_>, entries: &[(&str, &str)]) {
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
pub(super) fn publish_phar_write_function_pointer(ctx: &mut FunctionContext<'_>) {
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
pub(super) fn publish_dynamic_phar_write_function_pointer(ctx: &mut FunctionContext<'_>) {
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
pub(super) fn publish_phar_delete_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_delete_url",
        "_elephc_phar_delete_url_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the native PHAR compression-control bridge.
pub(super) fn publish_phar_set_compression_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_set_compression",
        "_elephc_phar_set_compression_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive-entry listing bridge used by PHAR OOP constructors.
pub(super) fn publish_phar_list_entries_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_list_entries",
        "_elephc_phar_list_entries_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive global-metadata read bridge.
pub(super) fn publish_phar_get_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_get_metadata", "_elephc_phar_get_metadata_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive global-metadata write bridge.
pub(super) fn publish_phar_set_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_set_metadata", "_elephc_phar_set_metadata_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive stub read bridge.
pub(super) fn publish_phar_get_stub_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[("elephc_phar_get_stub", "_elephc_phar_get_stub_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the archive stub write bridge.
pub(super) fn publish_phar_set_stub_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[("elephc_phar_set_stub", "_elephc_phar_set_stub_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the per-file metadata read bridge.
pub(super) fn publish_phar_get_file_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_get_file_metadata",
        "_elephc_phar_get_file_metadata_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the per-file metadata write bridge.
pub(super) fn publish_phar_set_file_metadata_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_set_file_metadata",
        "_elephc_phar_set_file_metadata_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the whole-archive gzip compression bridge.
pub(super) fn publish_phar_gzip_archive_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_gzip_archive", "_elephc_phar_gzip_archive_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the whole-archive bzip2 compression bridge.
pub(super) fn publish_phar_bzip2_archive_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_bzip2_archive", "_elephc_phar_bzip2_archive_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the whole-archive decompression bridge.
pub(super) fn publish_phar_decompress_archive_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_decompress_archive",
        "_elephc_phar_decompress_archive_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the OpenSSL (RSA-SHA1) signing bridge.
pub(super) fn publish_phar_sign_openssl_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] =
        &[("elephc_phar_sign_openssl", "_elephc_phar_sign_openssl_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the hash-based signing bridge.
pub(super) fn publish_phar_sign_hash_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[("elephc_phar_sign_hash", "_elephc_phar_sign_hash_fn")];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the ZipCrypto password bridge used to read encrypted ZIP entries.
pub(super) fn publish_phar_set_zip_password_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_set_zip_password",
        "_elephc_phar_set_zip_password_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the signature-hash read bridge.
pub(super) fn publish_phar_get_signature_hash_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_get_signature_hash",
        "_elephc_phar_get_signature_hash_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Publishes the signature-type read bridge.
pub(super) fn publish_phar_get_signature_type_function_pointer(ctx: &mut FunctionContext<'_>) {
    const ENTRIES: &[(&str, &str)] = &[(
        "elephc_phar_get_signature_type",
        "_elephc_phar_get_signature_type_fn",
    )];
    publish_phar_bridge_entries(ctx, ENTRIES);
}

/// Lowers `hash_file(algo, filename, binary?)` by reading bytes then hashing them.
pub(crate) fn lower_hash_file(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
pub(crate) fn lower_readfile(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "readfile", 1)?;
    let path = expect_operand(inst, 0)?;
    load_string_to_result(ctx, path, "readfile")?;
    emit_readfile_wrapper_dispatch(ctx);
    box_readfile_result(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `readline(prompt?)` by optionally writing a prompt and reading stdin.
pub(crate) fn lower_readline(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "readline", 0, 1)?;
    if inst.operands.len() == 1 {
        let prompt = expect_operand(inst, 0)?;
        load_string_to_result(ctx, prompt, "readline prompt")?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("bl __rt_vd_write");                    // write x1/x2 through the ob/web-aware stdout sink (register-preserving)
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
    // `__rt_fgets` keeps the newline and answers zero bytes at EOF; `readline`
    // strips the newline and answers `false`. Both halves are in one helper
    // because the ORDER between them is the whole correctness argument.
    super::super::io::box_readline_result(ctx, "readline");
    store_if_result(ctx, inst)
}

