//! Purpose:
//! File writes and PHAR compression or metadata bridge helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `file_put_contents(path, data)` through the target-aware runtime writer.
pub(crate) fn lower_file_put_contents(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count_between(inst, "file_put_contents", 2, 4)?;
    // php opens a stream internally for this call, so it consumes one PHP-visible resource
    // id even though the caller never sees a handle. elephc uses raw syscalls and minted
    // nothing, so every id AFTER such a call was one lower than php's — visible through
    // `var_dump($handle)`, `(int) $handle` and `get_resource_id()`. The cursor is never
    // reused, so advancing it is the whole of what php does here.
    abi::emit_call_label(ctx.emitter, "__rt_resource_id_burn");
    let path = expect_operand(inst, 0)?;
    let data = expect_operand(inst, 1)?;
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
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_file_put_contents_arm64(ctx, path, data, flags, helper)?,
        Arch::X86_64 => lower_file_put_contents_x86_64(ctx, path, data, flags, helper)?,
    }
    if let Some(done) = filter_done {
        ctx.emitter.label(&done);                                               // the route rejoins with a raw count or -1
    }
    // php answers `int|false`, and the runtime's -1 is the failure sentinel; the box is what
    // lets `file_put_contents($p, $d) === false` — the manual's own failure test — fire.
    box_negative_int_or_false_result(ctx, "fpc");
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
            load_string_to_result(ctx, data, "file_put_contents data")?;
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // the handle; the payload is already in x1/x2
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");                   // deflates through _stream_write_filters
            ctx.emitter.instruction("str x0, [sp, #8]");                        // php reports the INPUT byte count
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // the deflate tail is keyed by DESCRIPTOR
            super::close_crypto_arch::emit_zlib_flush_on_close_for_current_fd(ctx);
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closed");
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
            load_string_to_result(ctx, data, "file_put_contents data")?;
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // the handle
            ctx.emitter.instruction("mov rsi, rax");                            // the data pointer; the length is already in rdx
            abi::emit_call_label(ctx.emitter, "__rt_fwrite");                   // deflates through _stream_write_filters
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // php reports the INPUT byte count
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // the deflate tail is keyed by DESCRIPTOR
            super::close_crypto_arch::emit_zlib_flush_on_close_for_current_fd(ctx);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closed");
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
            load_string_to_result(ctx, data, "file_put_contents data")?;
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");          // x0 = the input byte count php reports
            ctx.emitter.instruction("str x0, [sp, #8]");
            ctx.emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closed");
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
            load_string_to_result(ctx, data, "file_put_contents data")?;
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            ctx.emitter.instruction("mov rsi, rax");                            // the data pointer; the length is already in rdx
            abi::emit_call_label(ctx.emitter, "__rt_fwrite_filtered");          // rax = the input byte count php reports
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");
            abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closed");
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

