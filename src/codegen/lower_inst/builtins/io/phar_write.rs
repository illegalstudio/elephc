//! Purpose:
//! File writes and PHAR compression or metadata bridge helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - THESE ROUTES CLOSE THE STREAM THEY OPENED, through `__rt_stream_close_backend` rather than
//!   `__rt_resource_mark_closed`. Marking a resource closed is what SKIPS the backend close, and
//!   the backend close is where a userspace wrapper's `stream_flush()`/`stream_close()` are
//!   dispatched from — so `file_put_contents("wrapper://x", $d)` wrote through the wrapper and
//!   then never let it finish. MEASURED on `php -n` 8.5.6 against a wrapper that traces its own
//!   calls: php closes, elephc did not.
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `file_put_contents(path, data)` through the target-aware runtime writer.
/// Loads `file_put_contents()`'s `$data` into the string result registers.
///
/// php JOINS an array payload rather than casting it — MEASURED: `["a","b","c"]` writes `"abc"`
/// and `[1, 2.5, true, null]` writes `"12.51"`, so each element takes the ordinary string
/// conversion and the pieces are concatenated with no separator. `(string)$array` would answer
/// `"Array"` with a notice, which is why this cannot live in the shared string loader.
///
/// `__rt_implode` leaves its result in the string result registers, which is where every caller
/// here reads the payload from, so the array case needs nothing after it.
/// Reports whether a value of this type could be a STREAM at run time.
///
/// `fopen()` is declared `resource|false`, so the common case is a union rather than a bare
/// resource, and `mixed` can hold one too. Both are decided by the tag, at run time.
fn may_be_a_stream(ty: &PhpType) -> bool {
    match ty {
        PhpType::Resource(_) | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(may_be_a_stream),
        _ => false,
    }
}

pub(super) fn load_file_put_contents_payload(
    ctx: &mut FunctionContext<'_>,
    data: ValueId,
) -> Result<()> {
    // php DRAINS a stream argument: `file_put_contents($p, $h)` writes what `$h` still holds,
    // from wherever it is. elephc converted the handle to a STRING and wrote the eleven bytes of
    // `Resource id #5` — MEASURED: php wrote `from-stream` (11 bytes) and elephc wrote 14.
    let declared = ctx.value_php_type(data)?;
    if may_be_a_stream(&declared) {
        return load_file_put_contents_stream_payload(ctx, data, &declared);
    }
    if !matches!(
        declared.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    ) {
        return load_string_to_result(ctx, data, "file_put_contents data");
    }
    let (empty_label, _) = ctx.data.add_string(b"");
    load_value_to_first_int_arg(ctx, data)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // the array to join
            abi::emit_symbol_address(ctx.emitter, "x1", &empty_label);          // php joins with no separator
            ctx.emitter.instruction("mov x2, #0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdx, rax");                            // the array to join
            abi::emit_symbol_address(ctx.emitter, "rdi", &empty_label);         // php joins with no separator
            ctx.emitter.instruction("xor esi, esi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_implode");                          // the pair lands in the string result registers
    Ok(())
}

/// Materializes the payload for a `$data` that may be a stream, deciding at run time.
///
/// A BARE `resource` is the handle itself; a union or `mixed` is a boxed cell whose tag says so.
/// Anything else falls through to the string conversion this call has always done.
fn load_file_put_contents_stream_payload(
    ctx: &mut FunctionContext<'_>,
    data: ValueId,
    declared: &PhpType,
) -> Result<()> {
    let boxed = declared.codegen_repr() != PhpType::Int;
    let plain = ctx.next_label("fpc_data_not_stream");
    let done = ctx.next_label("fpc_data_done");
    load_value_to_first_int_arg(ctx, data)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if boxed {
                ctx.emitter.instruction(&format!("cbz x0, {plain}"));           // a null cell holds no stream
                ctx.emitter.instruction("ldr x9, [x0]");                        // the cell's runtime tag
                ctx.emitter.instruction("cmp x9, #9");                          // tag 9 is a resource
                ctx.emitter.instruction(&format!("b.ne {plain}"));
                ctx.emitter.instruction("ldr x0, [x0, #8]");                    // payload_lo is the opaque handle
            }
            ctx.emitter.instruction("sub sp, sp, #16");
            ctx.emitter.instruction("str x0, [sp, #0]");                        // the handle outlives the chunk-size call
            abi::emit_call_label(ctx.emitter, "__rt_stream_chunk_size");
            ctx.emitter.instruction("mov x1, x0");                              // the read-loop chunk php would use
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            ctx.emitter.instruction("add sp, sp, #16");
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents");      // x1 = bytes, x2 = length
            abi::emit_jump(ctx.emitter, &done);
        }
        Arch::X86_64 => {
            if boxed {
                ctx.emitter.instruction("test rax, rax");
                ctx.emitter.instruction(&format!("jz {plain}"));                // a null cell holds no stream
                ctx.emitter.instruction("mov r10, QWORD PTR [rax]");            // the cell's runtime tag
                ctx.emitter.instruction("cmp r10, 9");                          // tag 9 is a resource
                ctx.emitter.instruction(&format!("jne {plain}"));
                ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");        // payload_lo is the opaque handle
            }
            ctx.emitter.instruction("sub rsp, 16");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // the handle outlives the chunk-size call
            ctx.emitter.instruction("mov rdi, rax");
            abi::emit_call_label(ctx.emitter, "__rt_stream_chunk_size");
            ctx.emitter.instruction("mov rsi, rax");                            // the read-loop chunk php would use
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");
            ctx.emitter.instruction("mov rdi, rax");
            ctx.emitter.instruction("add rsp, 16");
            abi::emit_call_label(ctx.emitter, "__rt_stream_get_contents");      // rax = bytes, rdx = length
            abi::emit_jump(ctx.emitter, &done);
        }
    }
    ctx.emitter.label(&plain);
    load_string_to_result(ctx, data, "file_put_contents data")?;
    ctx.emitter.label(&done);
    Ok(())
}

pub(crate) fn lower_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    // php names THIS builtin in the two lines a refused `php://` URL prints, and the
    // run-time opener sees only a path; publish them before any open can reach it.
    super::fopen_core::emit_publish_wrapper_open_callee(ctx, "file_put_contents");
    super::super::ensure_arg_count_between(inst, "file_put_contents", 2, 4)?;
    // php opens a stream internally for this call, so it consumes one PHP-visible resource
    // id even though the caller never sees a handle. elephc uses raw syscalls and minted
    // nothing, so every id AFTER such a call was one lower than php's — visible through
    // `var_dump($handle)`, `(int) $handle` and `get_resource_id()`. The cursor is never
    // reused, so advancing it is the whole of what php does here.
    abi::emit_call_label(ctx.emitter, "__rt_resource_id_burn");
    let path = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
    // php throws rather than warning for an empty filename — see `emit_empty_path_value_error`.
    super::emit_empty_path_value_error(ctx, path, super::EMPTY_PATH_MESSAGE)?;
    let path_literal = optional_const_string_operand(ctx, path)?;
    if let Some(path_literal) = path_literal.as_deref() {
        if path_literal.starts_with("phar://") {
            return lower_literal_phar_file_put_contents(ctx, inst, path_literal, data);
        }
        if let Some(underlying) = path_literal.strip_prefix("compress.zlib://") {
            return lower_literal_compress_zlib_file_put_contents(
                ctx,
                inst,
                path_literal,
                underlying,
                data,
            );
        }
        // A scheme that is NOT built in names a wrapper the program registered, and the opener is
        // the only thing that can serve it. Everything else reached the one-shot filesystem
        // writer, which took `uww://x` for a FILENAME and answered false with php's
        // "No such file or directory" — while `file_get_contents()` on the same URL read it.
        // php-src has one opener for both directions.
        if path_literal.find("://").is_some_and(|scheme_end| {
            let scheme = &path_literal[..scheme_end];
            !crate::types::stream_constants::STREAM_WRAPPERS
                .iter()
                .any(|known| *known == scheme)
                && scheme != "compress.zlib"
                && scheme != "compress.bzip2"
        }) || super::is_php_substream_uri(path_literal)
        {
            return lower_literal_wrapper_file_put_contents(ctx, inst, path_literal, data);
        }
    }
    let helper = if path_literal.is_none() {
        publish_dynamic_phar_write_function_pointer(ctx);
        "__rt_file_put_contents_maybe_phar"
    } else {
        "__rt_file_put_contents"
    };
    let flags = inst.operands.get(2).copied();
    // A `php://filter/write=.../resource=...` filename writes THROUGH the named filters, which
    // needs a stream: the one-shot writer below has nowhere to attach a chain. The route probes
    // the URL at run time — one spelling serves the literal and the assembled form alike — and
    // falls through to the ordinary writer when the URL is not a usable filter URL.
    let filter_done = if path_literal.is_none()
        || path_literal.as_deref().is_some_and(|p| p.starts_with("php://filter/"))
    {
        Some(emit_file_put_contents_filter_route(ctx, path, data, flags)?)
    } else {
        None
    };
    // A filename assembled at run time may name one of php's OWN sub-streams — `php://output`,
    // `php://memory`, `php://temp`. The writer below is `open(2)` and can only take those for
    // filenames; the opener serves them, exactly as it does for the literal spelling.
    let php_substream_done = if path_literal.is_none() {
        Some(emit_dynamic_php_substream_write_route(ctx, path, data)?)
    } else {
        None
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_file_put_contents_arm64(ctx, path, data, flags, helper)?,
        Arch::X86_64 => lower_file_put_contents_x86_64(ctx, path, data, flags, helper)?,
    }
    if let Some(done) = php_substream_done {
        ctx.emitter.label(&done);                                               // the route rejoins with a raw count or -1
    }
    if let Some(done) = filter_done {
        ctx.emitter.label(&done);                                               // the route rejoins with a raw count or -1
    }
    // php answers `int|false`, and the runtime's -1 is the failure sentinel; the box is what
    // lets `file_put_contents($p, $d) === false` — the manual's own failure test — fire.
    box_negative_int_or_false_result(ctx, "fpc");
    store_if_result(ctx, inst)
}

/// Writes through a user-registered wrapper, which only the shared opener can reach.
///
/// The same open/write/close php performs internally, and the same shape
/// `lower_literal_compress_zlib_file_put_contents` uses minus the deflate tail. php reports the
/// INPUT byte count rather than whatever `stream_write()` claims to have consumed, so that is what
/// travels back.
fn lower_literal_wrapper_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    uri: &str,
    data: ValueId,
) -> Result<()> {
    let appending = match inst.operands.get(2).copied() {
        Some(flags) => optional_const_i64_operand(ctx, flags)?.is_some_and(|f| f & 8 != 0),
        None => false,
    };
    let done = ctx.next_label("fpc_wrapper_done");
    let failed = ctx.next_label("fpc_wrapper_failed");
    begin_fopen_context_scope(ctx, inst.operands.get(3).copied())?;
    super::fopen_core::emit_literal_fopen_result(
        ctx,
        // php's own writer opens BINARY: a wrapper sees `wb` from `file_put_contents()`.
        super::fopen_core::LiteralOpenMode::Fixed(if appending { "ab" } else { "wb" }),
        uri,
        "file_put_contents",
    )?;
    finish_fopen_context_scope(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed open result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", failed));               // a failed open answers php false
            ctx.emitter.instruction("ldr x9, [x0, #8]");                        // the opaque stream handle
            ctx.emitter.instruction("sub sp, sp, #32");
            ctx.emitter.instruction("str x9, [sp, #0]");
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the handle; the payload is already in x1/x2
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");                   // reaches the wrapper's stream_write()
            ctx.emitter.instruction("str x0, [sp, #8]");                        // php reports the INPUT byte count
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // the count is this route's raw result
            ctx.emitter.instruction("add sp, sp, #32");
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov x0, #-1");                             // the shared failure sentinel
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed open result tag
            ctx.emitter.instruction("cmp r9, 9");                               // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", failed));                // a failed open answers php false
            ctx.emitter.instruction("mov r9, QWORD PTR [rax + 8]");             // the opaque stream handle
            ctx.emitter.instruction("sub rsp, 32");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // the handle
            ctx.emitter.instruction("mov rsi, rax");                            // the data pointer; the length is already in rdx
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");                   // reaches the wrapper's stream_write()
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // php reports the INPUT byte count
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // the count is this route's raw result
            ctx.emitter.instruction("add rsp, 32");
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov rax, -1");                             // the shared failure sentinel
            ctx.emitter.label(&done);
        }
    }
    box_negative_int_or_false_result(ctx, "fpc_wrapper");
    store_if_result(ctx, inst)
}

/// Writes a `compress.zlib://` filename as the gzip member php's wrapper produces.
///
/// The one-shot `__rt_file_put_contents` writer has nowhere to attach a deflate stream, so it
/// used to create a file literally NAMED `compress.zlib://out.gz` — the scheme was never
/// recognised here at all. This route is the same open/write/close php performs internally,
/// reusing the wrapper open the `fopen()` path already grew: the gzip framing, the context's
/// `zlib.level`, and the sync-flushed tail all come from that one place, so the two entry points
/// cannot drift.
///
/// php answers the INPUT byte count, not the compressed one (MEASURED on `php -n` 8.5.6: 1160
/// for a 1160-byte payload that lands as 66 bytes), which is exactly what `__rt_fwrite` returns
/// through the deflate helper. A failed open leaves -1 for the shared negative-int-or-false
/// boxing, so `file_put_contents(...) === false` still fires.
///
/// `$flags` is read for `FILE_APPEND` the same way the plain writer reads it, but only when it
/// is a compile-time constant; php appends a SECOND gzip member in that case, which is what
/// opening the underlying file in `a` produces.
fn lower_literal_compress_zlib_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    uri: &str,
    underlying: &str,
    data: ValueId,
) -> Result<()> {
    let appending = match inst.operands.get(2).copied() {
        Some(flags) => optional_const_i64_operand(ctx, flags)?.is_some_and(|f| f & 8 != 0),
        None => false,
    };
    let mode = if appending { "a" } else { "w" };
    let done = ctx.next_label("fpc_zlib_done");
    let failed = ctx.next_label("fpc_zlib_failed");
    begin_fopen_context_scope(ctx, inst.operands.get(3).copied())?;
    emit_literal_compress_wrapper_fopen_result(
        ctx,
        CompressUnderlying::Literal(underlying),
        uri,
        CompressWrapper::Zlib,
        mode,
    )?;
    finish_fopen_context_scope(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed open result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", failed));               // a failed open answers php false
            ctx.emitter.instruction("ldr x9, [x0, #8]");                        // the opaque stream handle
            ctx.emitter.instruction("sub sp, sp, #32");
            ctx.emitter.instruction("str x9, [sp, #0]");
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the handle; the payload is already in x1/x2
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");                   // deflates through _stream_write_filters
            ctx.emitter.instruction("str x0, [sp, #8]");                        // php reports the INPUT byte count
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // the deflate tail is keyed by DESCRIPTOR
            super::close_crypto_arch::emit_zlib_flush_on_close_for_current_fd(ctx);
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // the count is this route's raw result
            ctx.emitter.instruction("add sp, sp, #32");
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov x0, #-1");                             // the shared failure sentinel
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed open result tag
            ctx.emitter.instruction("cmp r9, 9");                               // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", failed));                // a failed open answers php false
            ctx.emitter.instruction("mov r9, QWORD PTR [rax + 8]");             // the opaque stream handle
            ctx.emitter.instruction("sub rsp, 32");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // the handle
            ctx.emitter.instruction("mov rsi, rax");                            // the data pointer; the length is already in rdx
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");                   // deflates through _stream_write_filters
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // php reports the INPUT byte count
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // the deflate tail is keyed by DESCRIPTOR
            super::close_crypto_arch::emit_zlib_flush_on_close_for_current_fd(ctx);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // the count is this route's raw result
            ctx.emitter.instruction("add rsp, 32");
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov rax, -1");                             // the shared failure sentinel
            ctx.emitter.label(&done);
        }
    }
    box_negative_int_or_false_result(ctx, "fpc");
    store_if_result(ctx, inst)
}

/// Writes THROUGH the filters a `php://filter/write=.../resource=...` filename names.
///
/// One spelling serves both forms: the URL bytes are probed with `__rt_php_filter_parse` at
/// run time, so a literal URL and an assembled one take the identical path. When the parse
/// declines, everything falls through untouched to the ordinary one-shot writer.
///
/// The streamed shape mirrors the read route: open the RESOURCE (php:// wrapper or filesystem,
/// `w`/`a` chosen by the FILE_APPEND bit), attach the parked chain once the stream is boxed,
/// write through `__rt_fwrite_filtered` — php answers the INPUT byte count, which is what that
/// helper returns — and close. A resource that cannot be opened warns in php's words, naming
/// `file_put_contents` and the WHOLE URL with the wrapper's generic `operation failed`, and
/// leaves -1 for the shared negative-int-or-false boxing.
///
/// Emits everything up to and including the fall-through; the caller places the returned
/// done-label after the plain writer, before the shared boxing, so both paths converge on a
/// raw count-or-minus-one.
/// Writes a run-time `php://` sub-stream through the same opener `fopen()` uses.
///
/// Entry state: nothing staged. On a `php://` URL that is not a filter URL the route opens the
/// stream, writes the payload, closes it and branches to the returned label with the byte count in
/// the integer result register (or -1, which the caller's boxing reads as php false); anything else
/// falls through so the plain writer still runs.
///
/// The mode is always `w`: of the sub-streams this reaches, none distinguishes truncating from
/// appending — `php://output` has no position at all, and `php://memory`/`php://temp` are created
/// empty by the open itself.
fn emit_dynamic_php_substream_write_route(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
) -> Result<String> {
    let done = ctx.next_label("fpc_php_done");
    let not_php = ctx.next_label("fpc_php_plain");
    let failed = ctx.next_label("fpc_php_failed");
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x2, #7");                              // `php://` plus the naming byte
            ctx.emitter.instruction(&format!("b.lt {}", not_php));
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset));
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));
                ctx.emitter.instruction(&format!("b.ne {}", not_php));
            }
            ctx.emitter.instruction("ldrb w9, [x1, #6]");                       // the first byte of the sub-stream name
            ctx.emitter.instruction("cmp w9, #0x66");                           // 'f' as in filter, which has its own route
            ctx.emitter.instruction(&format!("b.eq {}", not_php));
            ctx.emitter.instruction("mov x0, x1");                              // the opener takes ptr/len in x0/x1
            ctx.emitter.instruction("mov x1, x2");
            abi::emit_call_label(ctx.emitter, "__rt_php_wrapper_open");         // x0 = descriptor, or -1
            ctx.emitter.instruction("cmn x0, #1");
            ctx.emitter.instruction(&format!("b.eq {}", failed));
            ctx.emitter.instruction("sub sp, sp, #32");
            ctx.emitter.instruction("str x0, [sp, #0]");                        // the descriptor, across the payload load
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the payload is already in x1/x2
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
            ctx.emitter.instruction("str x0, [sp, #8]");                        // php reports the INPUT byte count
            // `__rt_php_wrapper_open` hands back a DESCRIPTOR, so the backend close is the one
            // that takes it: the same one every implicit destruction goes through.
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("ldr x0, [sp, #8]");
            ctx.emitter.instruction("add sp, sp, #32");
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov x0, #-1");                             // the shared boxing reads -1 as PHP false
            ctx.emitter.instruction(&format!("b {}", done));
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdx, 7");                              // `php://` plus the naming byte
            ctx.emitter.instruction(&format!("jl {}", not_php));
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter
                    .instruction(&format!("cmp BYTE PTR [rax + {}], {}", offset, byte));
                ctx.emitter.instruction(&format!("jne {}", not_php));
            }
            ctx.emitter.instruction("cmp BYTE PTR [rax + 6], 0x66");            // 'f' as in filter
            ctx.emitter.instruction(&format!("je {}", not_php));
            ctx.emitter.instruction("mov rdi, rax");                            // the opener takes ptr/len in rdi/rsi
            ctx.emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(ctx.emitter, "__rt_php_wrapper_open");         // rax = descriptor, or -1
            ctx.emitter.instruction("cmp rax, -1");
            ctx.emitter.instruction(&format!("je {}", failed));
            ctx.emitter.instruction("sub rsp, 32");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // the descriptor, across the payload load
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            ctx.emitter.instruction("mov rsi, rax");                            // the data pointer; the length is already in rdx
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // php reports the INPUT byte count
            // See the AArch64 counterpart.
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");
            ctx.emitter.instruction("add rsp, 32");
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&failed);
            ctx.emitter.instruction("mov rax, -1");                             // the shared boxing reads -1 as PHP false
            ctx.emitter.instruction(&format!("jmp {}", done));
        }
    }
    ctx.emitter.label(&not_php);
    Ok(done)
}

fn emit_file_put_contents_filter_route(
    ctx: &mut FunctionContext<'_>,
    path: ValueId,
    data: ValueId,
    flags: Option<ValueId>,
) -> Result<String> {
    let done = ctx.next_label("fpc_filter_done");
    let not_filter = ctx.next_label("fpc_filter_plain");
    let drop_and_fall = ctx.next_label("fpc_filter_drop");
    let open_file = ctx.next_label("fpc_filter_file");
    let mode_ready = ctx.next_label("fpc_filter_mode_ready");
    let boxed = ctx.next_label("fpc_filter_boxed");
    let failed = ctx.next_label("fpc_filter_failed");
    load_string_to_result(ctx, path, "file_put_contents filename")?;
    // `w` and `a` both name the write direction and nothing else, so a prefix-less filter list
    // is applied exactly once here — which is how many times php warns for a name it cannot
    // resolve. Published before the parse, which is what reads it back for the report.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, #2");                              // file_put_contents writes
            abi::emit_symbol_address(ctx.emitter, "x10", "_php_filter_open_dirs");
            ctx.emitter.instruction("str x9, [x10]");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r10", "_php_filter_open_dirs");
            ctx.emitter.instruction("mov QWORD PTR [r10], 2");                  // file_put_contents writes
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                   // the URL, for the failure warning
            ctx.emitter.instruction("mov x0, x1");                              // the candidate filter URL
            ctx.emitter.instruction("mov x1, x2");                              // and its length
            abi::emit_call_label(ctx.emitter, "__rt_php_filter_parse");
            ctx.emitter.instruction("cmp x0, #2");                              // a filter URL that names no resource?
            ctx.emitter.instruction(&format!("b.eq {}_no_resource", not_filter)); // php throws for it
            ctx.emitter.instruction(&format!("cbz x0, {}", drop_and_fall));     // not a usable filter URL: the plain writer decides
            // The openers name themselves and the bare RESOURCE when they fail; php names
            // `file_put_contents` and the whole URL. Same suppression the read route uses.
            abi::emit_call_label(ctx.emitter, "__rt_diag_push_suppression");
            // The resource may be a user wrapper, whose `stream_open` is PHP and can `fopen()` a
            // filter URL of its own — which republishes the hand-off this route is holding.
            super::fopen_core::emit_dynamic_php_filter_save(ctx);
            // -- the FILE_APPEND bit picks the mode, exactly as it does for the plain writer --
            match flags {
                Some(flags) => {
                    ctx.load_value_to_result(flags)?;
                }
                None => ctx.emitter.instruction("mov x0, #0"),
            }
            abi::emit_symbol_address(ctx.emitter, "x3", "_fpc_mode_w");
            ctx.emitter.instruction(&format!("tbz x0, #3, {}", mode_ready));    // FILE_APPEND clear: truncate
            abi::emit_symbol_address(ctx.emitter, "x3", "_fpc_mode_a");
            ctx.emitter.label(&mode_ready);
            ctx.emitter.instruction("mov x4, #1");                              // one mode byte
            // -- the RESOURCE the parse published decides the opener --
            abi::emit_symbol_address(ctx.emitter, "x9", "_php_filter_res_ptr");
            ctx.emitter.instruction("ldr x1, [x9]");
            abi::emit_symbol_address(ctx.emitter, "x9", "_php_filter_res_len");
            ctx.emitter.instruction("ldr x2, [x9]");
            ctx.emitter.instruction("cmp x2, #6");                              // long enough for php://?
            ctx.emitter.instruction(&format!("b.lt {}", open_file));
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset));
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));
                ctx.emitter.instruction(&format!("b.ne {}", open_file));
            }
            ctx.emitter.instruction("mov x0, x1");                              // the wrapper opener takes ptr/len in x0/x1
            ctx.emitter.instruction("mov x1, x2");
            abi::emit_call_label(ctx.emitter, "__rt_php_wrapper_open");
            ctx.emitter.instruction(&format!("b {}", boxed));
            ctx.emitter.label(&open_file);
            abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
            ctx.emitter.label(&boxed);
            box_stream_fd_or_false_result(ctx, "fpc_filter");
            abi::emit_call_label(ctx.emitter, "__rt_diag_pop_suppression");     // preserves the boxed result: x9/x10 only
            super::fopen_core::emit_dynamic_php_filter_restore(ctx);            // this route's own hand-off, not a nested open's
            abi::emit_call_label(ctx.emitter, "__rt_php_filter_attach_pending");
            super::fopen_core::emit_php_filter_unknown_report(ctx, "file_put_contents");
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed open result tag
            ctx.emitter.instruction("cmp x9, #9");
            ctx.emitter.instruction(&format!("b.ne {}", failed));
            // -- write through the chain, then close the stream php opened on our behalf --
            ctx.emitter.instruction("ldr x9, [x0, #8]");                        // the opaque stream handle
            ctx.emitter.instruction("sub sp, sp, #32");
            ctx.emitter.instruction("str x9, [sp, #0]");
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");          // x0 = the input byte count php reports
            ctx.emitter.instruction("str x0, [sp, #8]");
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // the count is the route's raw result
            ctx.emitter.instruction("add sp, sp, #32");
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // drop the saved URL
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&failed);
            // php's wording: the function, the WHOLE URL, and the wrapper's generic reason.
            abi::emit_push_reg(ctx.emitter, "x0");                              // the boxed false, released below
            abi::emit_symbol_address(ctx.emitter, "x1", "_diag_open_failed_fpc_prefix");
            ctx.emitter.instruction(&format!("mov x2, #{}", "Warning: file_put_contents(".len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // the saved full URL
            ctx.emitter.instruction("ldr x2, [sp, #24]");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "x1", "_fgc_filter_fail_tail");
            ctx.emitter.instruction(&format!(
                "mov x2, #{}",
                crate::codegen_support::runtime::data::FGC_FILTER_FAIL_TAIL.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_free");                // a fresh unaliased false cell owns nothing else
            ctx.emitter.instruction("mov x0, #-1");                             // the shared boxing reads -1 as PHP false
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // drop the saved URL
            ctx.emitter.instruction(&format!("b {}", done));
            ctx.emitter.label(&drop_and_fall);
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // drop the saved URL on the plain path
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                 // the URL, for the failure warning
            ctx.emitter.instruction("mov rdi, rax");                            // the candidate filter URL
            ctx.emitter.instruction("mov rsi, rdx");                            // and its length
            abi::emit_call_label(ctx.emitter, "__rt_php_filter_parse");
            ctx.emitter.instruction("cmp rax, 2");                              // a filter URL that names no resource?
            ctx.emitter.instruction(&format!("je {}_no_resource", not_filter)); // php throws for it
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", drop_and_fall));          // not a usable filter URL: the plain writer decides
            // See the AArch64 counterpart: the openers' own failure warnings are suppressed.
            abi::emit_call_label(ctx.emitter, "__rt_diag_push_suppression");
            // See the AArch64 counterpart: a user wrapper's `stream_open` republishes the hand-off.
            super::fopen_core::emit_dynamic_php_filter_save(ctx);
            match flags {
                Some(flags) => {
                    ctx.load_value_to_result(flags)?;
                }
                None => ctx.emitter.instruction("xor eax, eax"),
            }
            abi::emit_symbol_address(ctx.emitter, "rdi", "_fpc_mode_w");
            ctx.emitter.instruction("test rax, 8");                             // FILE_APPEND?
            ctx.emitter.instruction(&format!("jz {}", mode_ready));
            abi::emit_symbol_address(ctx.emitter, "rdi", "_fpc_mode_a");
            ctx.emitter.label(&mode_ready);
            ctx.emitter.instruction("mov rsi, 1");                              // one mode byte
            abi::emit_symbol_address(ctx.emitter, "r9", "_php_filter_res_ptr");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");
            abi::emit_symbol_address(ctx.emitter, "r9", "_php_filter_res_len");
            ctx.emitter.instruction("mov rdx, QWORD PTR [r9]");
            ctx.emitter.instruction("cmp rdx, 6");                              // long enough for php://?
            ctx.emitter.instruction(&format!("jl {}", open_file));
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter.instruction(&format!("cmp BYTE PTR [rax + {}], {}", offset, byte));
                ctx.emitter.instruction(&format!("jne {}", open_file));
            }
            ctx.emitter.instruction("mov rdi, rax");                            // the wrapper opener takes ptr/len in rdi/rsi
            ctx.emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(ctx.emitter, "__rt_php_wrapper_open");
            ctx.emitter.instruction(&format!("jmp {}", boxed));
            ctx.emitter.label(&open_file);
            abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
            ctx.emitter.label(&boxed);
            box_stream_fd_or_false_result(ctx, "fpc_filter");
            abi::emit_call_label(ctx.emitter, "__rt_diag_pop_suppression");     // preserves the boxed result: r10 only
            super::fopen_core::emit_dynamic_php_filter_restore(ctx);            // this route's own hand-off, not a nested open's
            abi::emit_call_label(ctx.emitter, "__rt_php_filter_attach_pending");
            super::fopen_core::emit_php_filter_unknown_report(ctx, "file_put_contents");
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed open result tag
            ctx.emitter.instruction("cmp r9, 9");
            ctx.emitter.instruction(&format!("jne {}", failed));
            ctx.emitter.instruction("mov r9, QWORD PTR [rax + 8]");             // the opaque stream handle
            ctx.emitter.instruction("sub rsp, 32");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r9");
            load_file_put_contents_payload(ctx, data)?;
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            ctx.emitter.instruction("mov rsi, rax");                            // the data pointer; the length is already in rdx
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");          // rax = the input byte count php reports
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_close_backend");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // the count is the route's raw result
            ctx.emitter.instruction("add rsp, 32");
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // drop the saved URL
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&failed);
            abi::emit_push_reg(ctx.emitter, "rax");                             // the boxed false, released below
            abi::emit_symbol_address(ctx.emitter, "rdi", "_diag_open_failed_fpc_prefix");
            ctx.emitter.instruction(&format!("mov rsi, {}", "Warning: file_put_contents(".len()));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // the saved full URL
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "rdi", "_fgc_filter_fail_tail");
            ctx.emitter.instruction(&format!(
                "mov rsi, {}",
                crate::codegen_support::runtime::data::FGC_FILTER_FAIL_TAIL.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_free");                // a fresh unaliased false cell owns nothing else
            ctx.emitter.instruction("mov rax, -1");                             // the shared boxing reads -1 as PHP false
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // drop the saved URL
            ctx.emitter.instruction(&format!("jmp {}", done));
            ctx.emitter.label(&drop_and_fall);
            abi::emit_release_temporary_stack(ctx.emitter, 16);                 // drop the saved URL on the plain path
        }
    }
    let past_throw = ctx.next_label("fpc_filter_resourced");
    abi::emit_jump(ctx.emitter, &past_throw);
    ctx.emitter.label(&format!("{}_no_resource", not_filter));
    // The saved URL pair stays on the stack through the throw, exactly as count()'s probe
    // leaves its pushed value: the unwinder walks frames, not the temporary stack.
    crate::codegen::lower_inst::exceptions::emit_error(ctx, "No URL resource specified");
    ctx.emitter.label(&past_throw);
    ctx.emitter.label(&not_filter);
    Ok(done)
}

/// Lowers one-shot `file_put_contents("phar://archive/entry", data)`.
pub(super) fn lower_literal_phar_file_put_contents(
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
        box_negative_int_or_false_result(ctx, "fpc_phar_unresolved");           // php reads the failure as false
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
    // Same `int|false` contract as the filesystem path: the -1 sentinel boxes to PHP false.
    box_negative_int_or_false_result(ctx, "fpc_phar");
    store_if_result(ctx, inst)
}

/// Lowers the compiler-internal native PHAR compression-control helper.
pub(crate) fn lower_elephc_phar_set_compression(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_phar_set_compression", 2)?;
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
pub(crate) fn lower_elephc_phar_get_metadata(
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
pub(crate) fn lower_elephc_phar_get_stub(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    publish_phar_get_stub_function_pointer(ctx);
    emit_phar_get_string_bridge(ctx, inst, "__elephc_phar_get_stub", "_elephc_phar_get_stub_fn")
}

/// Lowers `__elephc_phar_set_metadata()` into the metadata-write bridge call.
pub(crate) fn lower_elephc_phar_set_metadata(
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
pub(crate) fn lower_elephc_phar_set_stub(
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
pub(super) fn emit_phar_set_string_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 2)?;
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
pub(super) fn emit_phar_string_to_bool_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
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
pub(super) fn emit_phar_get_string_bridge(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    slot: &str,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
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
            ctx.emitter.instruction(&format!("jmp {}", persist));               // persist the returned bytes
            ctx.emitter.label(&empty);
            ctx.emitter.instruction("mov rdi, 0");                              // empty source pointer (length 0 is not dereferenced)
            ctx.emitter.instruction("mov rdx, 0");                              // empty string length
            ctx.emitter.label(&persist);
            ctx.emitter.instruction("call __rt_str_persist");                   // copy into an owned heap string -> rax=ptr, rdx=len
        }
    }
    store_if_result(ctx, inst)
}

