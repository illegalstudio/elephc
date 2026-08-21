//! Purpose:
//! Core fopen dispatch and php filter URL parsing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `fopen(filename, mode)` and boxes stream resources or PHP false.
pub(crate) fn lower_fopen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "fopen", 2, 4)?;
    let filename = expect_operand(inst, 0)?;
    let mode = expect_operand(inst, 1)?;
    let filename_literal = optional_const_string_operand(ctx, filename)?;
    begin_fopen_context_scope(ctx, inst.operands.get(3).copied())?;
    if let Some(path) = filename_literal.as_deref() {
        let open_mode = LiteralOpenMode::Operand(mode);
        if path.starts_with("php://filter/") {
            emit_literal_php_filter_fopen_result(ctx, open_mode, path, "fopen")?;
        } else if let Some(underlying) = path.strip_prefix("compress.zlib://") {
            // A mode that is not a compile-time literal reads: it is the overwhelmingly common
            // open, and it is also what this branch did before it knew about `$mode` at all.
            let mode_text =
                optional_const_string_operand(ctx, mode)?.unwrap_or_else(|| "r".to_string());
            emit_literal_compress_wrapper_fopen_result(
                ctx,
                CompressUnderlying::Literal(underlying),
                path,
                CompressWrapper::Zlib,
                &mode_text,
            )?;
        } else if let Some(underlying) = path.strip_prefix("compress.bzip2://") {
            let mode_text =
                optional_const_string_operand(ctx, mode)?.unwrap_or_else(|| "r".to_string());
            emit_literal_compress_wrapper_fopen_result(
                ctx,
                CompressUnderlying::Literal(underlying),
                path,
                CompressWrapper::Bzip2,
                &mode_text,
            )?;
        } else {
            emit_literal_fopen_result(ctx, open_mode, path)?;
        }
        emit_record_stream_mode_after_boxed(ctx, mode)?;
        finish_fopen_context_scope(ctx);
        store_if_result(ctx, inst)?;
        if path.starts_with("http://") {
            publish_http_response_headers(ctx);
        }
        return Ok(());
    }
    // A `compress.*://` url only known at RUN time is opened here, before the ordinary dynamic
    // open it would otherwise fall into. The label it hands back is placed past that open, so a
    // url the wrappers claimed skips it.
    let compress_done = emit_dynamic_compress_wrapper_fopen(ctx, inst, filename, mode)?;
    publish_dynamic_phar_function_pointers(ctx);
    publish_dynamic_phar_write_function_pointer(ctx);
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
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x1", "x2"),
        Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx"),
    }
    emit_dynamic_fopen_result(ctx, inst)?;
    if let Some(label) = compress_done {
        ctx.emitter.label(&label);
    }
    Ok(())
}


/// The compression wrappers, with the prefix a run-time URL is recognised by.
const DYNAMIC_COMPRESS_WRAPPERS: &[(&str, CompressWrapper)] = &[
    ("compress.zlib://", CompressWrapper::Zlib),
    ("compress.bzip2://", CompressWrapper::Bzip2),
];

/// Opens a `compress.zlib://` or `compress.bzip2://` URL whose path is only known at run time.
///
/// `$name = "compress.zlib://out.gz"; fopen($name, "w");` answered `false` where the identical
/// call with the literal compresses, in both directions — the wrappers were reachable only from a
/// compile-time literal, because that is what the split into "wrapper" and "underlying path"
/// needed. A URL assembled at run time is ordinary PHP: a filename from config, a path built with
/// `sys_get_temp_dir()`.
///
/// The URL is compared against each prefix in turn, and a match runs the very sequence the literal
/// path runs — the same open, the same filter attach — with the underlying path taken from the
/// staged string registers instead of a baked data string.
///
/// `$mode` follows the literal path's rule: a compile-time literal decides the direction, and
/// anything else reads, which is the overwhelmingly common open.
///
/// Returns the label the caller must place AFTER the ordinary dynamic open, so a URL one of the
/// wrappers claimed jumps past it.
fn emit_dynamic_compress_wrapper_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    filename: ValueId,
    mode: ValueId,
) -> Result<Option<String>> {
    let mode_text = optional_const_string_operand(ctx, mode)?.unwrap_or_else(|| "r".to_string());
    let done_label = ctx.next_label("compress_dyn_done");
    for (prefix, kind) in DYNAMIC_COMPRESS_WRAPPERS {
        let next_label = ctx.next_label("compress_dyn_next");
        let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
        load_string_to_result(ctx, filename, "fopen filename")?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                // `__rt_str_starts_with` takes x1/x2 = haystack, which is where the load left the
                // url, and x3/x4 = needle.
                abi::emit_symbol_address(ctx.emitter, "x3", &prefix_label);
                ctx.emitter.instruction(&format!("mov x4, #{prefix_len}"));
                abi::emit_call_label(ctx.emitter, "__rt_str_starts_with");
                ctx.emitter.instruction(&format!("cbz x0, {}", next_label));
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // the url pointer
                ctx.emitter.instruction("mov rsi, rdx");                        // and its byte length
                ctx.emitter
                    .instruction(&format!("lea rdx, [rip + {prefix_label}]"));  // the wrapper prefix
                ctx.emitter.instruction(&format!("mov rcx, {prefix_len}"));
                abi::emit_call_label(ctx.emitter, "__rt_str_starts_with");
                ctx.emitter.instruction("test rax, rax");
                ctx.emitter.instruction(&format!("je {}", next_label));
            }
        }
        // Reload: the prefix probe consumed the staged registers, and the opener reads the url
        // from them to step past the prefix it just matched.
        load_string_to_result(ctx, filename, "fopen filename")?;
        emit_literal_compress_wrapper_fopen_result(
            ctx,
            CompressUnderlying::Staged { prefix_len },
            prefix,
            *kind,
            &mode_text,
        )?;
        emit_record_stream_mode_after_boxed(ctx, mode)?;
        store_if_result(ctx, inst)?;
        abi::emit_jump(ctx.emitter, &done_label);
        ctx.emitter.label(&next_label);
    }
    Ok(Some(done_label))
}

/// Where the open mode a `php://filter` URL is opened with comes from.
///
/// It decides how many times php walks a prefix-less filter list — once per direction the mode
/// selects — so it has to reach the run-time report, and only `fopen()` has a `$mode` argument
/// to read it from.
#[derive(Clone, Copy)]
pub(super) enum DynamicFilterMode {
    /// The mode sits in the staged mode registers, as `fopen()` leaves it: `x3`/`x4` on
    /// AArch64, `rdi`/`rsi` on x86_64. It is read at run time, because `fopen($url, $mode)`
    /// reaches here with BOTH assembled at run time.
    Staged,
    /// A caller with no `$mode` argument fixes the directions: bit 0 read, bit 1 write.
    Fixed(u8),
}

/// Replaces a run-time `php://filter/...` filename with the RESOURCE it wraps.
///
/// A filter URL is "open this, then filter it", so the open that follows is the ordinary one for
/// whatever the resource turns out to be — a file, `php://temp`, anything. The filter it named is
/// parked by the parse and attached after boxing, which is why nothing here needs to know how to
/// open the resource itself.
pub(super) fn emit_dynamic_php_filter_swap(
    ctx: &mut FunctionContext<'_>,
    mode: DynamicFilterMode,
) {
    let unchanged = ctx.next_label("fopen_dynamic_not_filter");
    let no_resource = ctx.next_label("fopen_dynamic_filter_no_resource");
    emit_publish_php_filter_open_dirs(ctx, mode);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // Both pairs must be saved, not just the mode: the parse takes its argument in x0/x1
            // and so destroys the filename pair the fall-through path still needs.
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                   // the filename
            abi::emit_push_reg_pair(ctx.emitter, "x3", "x4");                   // the fopen mode
            ctx.emitter.instruction("mov x0, x1");                              // the candidate filter URL
            ctx.emitter.instruction("mov x1, x2");                              // and its length
            abi::emit_call_label(ctx.emitter, "__rt_php_filter_parse");
            ctx.emitter.instruction("mov x9, x0");                              // did it parse as a filter URL?
            abi::emit_pop_reg_pair(ctx.emitter, "x3", "x4");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("cmp x9, #2");                              // a filter URL that names no resource?
            ctx.emitter.instruction(&format!("b.eq {}", no_resource));          // php throws for it
            ctx.emitter.instruction(&format!("cbz x9, {}", unchanged));         // no: the filename stands
            abi::emit_symbol_address(ctx.emitter, "x9", "_php_filter_res_ptr");
            ctx.emitter.instruction("ldr x1, [x9]");                            // open the resource instead
            abi::emit_symbol_address(ctx.emitter, "x9", "_php_filter_res_len");
            ctx.emitter.instruction("ldr x2, [x9]");                            // with its length
        }
        Arch::X86_64 => {
            // See the AArch64 counterpart: the parse takes rdi/rsi, so the filename pair in
            // rax/rdx has to be saved as well as the mode.
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                 // the filename
            abi::emit_push_reg_pair(ctx.emitter, "rdi", "rsi");                 // the fopen mode
            ctx.emitter.instruction("mov rdi, rax");                            // the candidate filter URL
            ctx.emitter.instruction("mov rsi, rdx");                            // and its length
            abi::emit_call_label(ctx.emitter, "__rt_php_filter_parse");
            ctx.emitter.instruction("mov r9, rax");                             // did it parse as a filter URL?
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("cmp r9, 2");                               // a filter URL that names no resource?
            ctx.emitter.instruction(&format!("je {}", no_resource));            // php throws for it
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jz {}", unchanged));              // no: the filename stands
            abi::emit_symbol_address(ctx.emitter, "r9", "_php_filter_res_ptr");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // open the resource instead
            abi::emit_symbol_address(ctx.emitter, "r9", "_php_filter_res_len");
            ctx.emitter.instruction("mov rdx, QWORD PTR [r9]");                 // with its length
        }
    }
    let past_throw = ctx.next_label("fopen_dynamic_filter_resourced");
    abi::emit_jump(ctx.emitter, &past_throw);
    ctx.emitter.label(&no_resource);
    // php's wording and class, and `@` does not soften it — the throw ignores the diagnostic
    // suppression depth, exactly as php's Error does.
    crate::codegen::lower_inst::exceptions::emit_error(ctx, "No URL resource specified");
    ctx.emitter.label(&past_throw);
    ctx.emitter.label(&unchanged);
}

/// Publishes the directions the open mode selects, for the run-time unknown-name report.
fn emit_publish_php_filter_open_dirs(ctx: &mut FunctionContext<'_>, mode: DynamicFilterMode) {
    match mode {
        DynamicFilterMode::Staged => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");               // the filename: the probe takes x0/x1
                ctx.emitter.instruction("mov x0, x3");                          // the fopen mode, whose letters pick the directions
                ctx.emitter.instruction("mov x1, x4");
                abi::emit_call_label(ctx.emitter, "__rt_php_filter_mode_dirs"); // preserves x3/x4: it reads x0/x1 only
                abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            }
            Arch::X86_64 => {
                abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");             // the filename: the probe takes rdi/rsi
                abi::emit_call_label(ctx.emitter, "__rt_php_filter_mode_dirs"); // the mode is already staged in rdi/rsi
                abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            }
        },
        DynamicFilterMode::Fixed(bits) => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("mov x9, #{bits}"));           // this caller's fixed direction
                abi::emit_symbol_address(ctx.emitter, "x10", "_php_filter_open_dirs");
                ctx.emitter.instruction("str x9, [x10]");
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(ctx.emitter, "r10", "_php_filter_open_dirs");
                ctx.emitter.instruction(&format!("mov QWORD PTR [r10], {bits}")); // this caller's fixed direction
            }
        },
    }
}

/// Finishes a run-time `php://filter` open: the failed-open line, the chain, the skipped names.
///
/// One call site's worth of order, in one place, because all four dynamic `fopen()` exits and
/// every path-reader route need exactly the same three steps and the same `callee` in the two
/// diagnostics php words with it.
pub(super) fn emit_dynamic_php_filter_finish(ctx: &mut FunctionContext<'_>, callee: &str) {
    // Before anything reads the hand-off: the open that just ran may have been a user wrapper
    // whose `stream_open` opened a filter URL of its own, and that inner parse published over
    // every slot this open's three steps are about to consume.
    emit_dynamic_php_filter_restore(ctx);
    // First, because it ends the suppression the open ran under and a failed open must print
    // its line ALONE — it drops the skipped names the report below would otherwise warn for.
    emit_php_filter_callee_call(ctx, callee, "__rt_php_filter_open_failed");
    emit_dynamic_php_filter_attach(ctx);
    emit_php_filter_unknown_report(ctx, callee);
}

/// Warns, in `callee`'s words, for every run-time `php://filter` name that named no filter.
///
/// Separate from [`emit_dynamic_php_filter_finish`] because the path readers compose their own
/// failed-open line from a URL they saved themselves, and only need this half.
pub(super) fn emit_php_filter_unknown_report(ctx: &mut FunctionContext<'_>, callee: &str) {
    emit_php_filter_callee_call(ctx, callee, "__rt_php_filter_unknown_report");
}

/// Calls a run-time `php://filter` reporter with the CALLING function's name staged.
///
/// php names the caller in every one of these diagnostics — `fopen(): Unable to locate filter`,
/// `file_get_contents(): Unable to create filter` — so the name travels as an ordinary string
/// pair rather than each route owning a copy of the composition.
fn emit_php_filter_callee_call(ctx: &mut FunctionContext<'_>, callee: &str, helper: &str) {
    let (label, len) = ctx.data.add_string(callee.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);                // the boxed result stays in x0
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);               // the boxed result stays in rax
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, helper);
}

/// Attaches the filter a run-time `php://filter` URL named, once the resource is open and boxed.
///
/// A no-op when nothing is pending, which is every open that did not come from a filter URL.
fn emit_dynamic_php_filter_attach(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_php_filter_attach_pending");
}

/// Parks the parse's whole hand-off for the length of the open that is about to run.
///
/// Paired with [`emit_dynamic_php_filter_restore`] on every path that reaches an opener, because
/// an opener can run PHP: a user wrapper's `stream_open` that `fopen()`s anything re-enters the
/// parse, and the parse publishes into fixed globals.
pub(super) fn emit_dynamic_php_filter_save(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_php_filter_pending_save");
}

/// Republishes the hand-off [`emit_dynamic_php_filter_save`] parked, before anything reads it.
pub(super) fn emit_dynamic_php_filter_restore(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_php_filter_pending_restore");
}

/// Opens a runtime filename that carries the `data://` prefix, falling through when it does not.
///
/// A literal `data://` URI is decoded during lowering and its bytes embedded, which left a
/// run-time URI with no path at all. `__rt_data_stream_dynamic` decodes from the bytes instead,
/// through the same base64 and percent decoders the rest of the runtime uses.
fn emit_dynamic_data_branch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    done: &str,
) -> Result<()> {
    let not_data = ctx.next_label("fopen_dynamic_not_data");
    // The scheme is `data:`, NOT `data://`: php-src's `php_stream_locate_url_wrapper` special-cases
    // this one wrapper so the `//` is optional, and `__rt_data_stream_dynamic` skips it when it is
    // there. Requiring it here sent `data:text/plain,hi` to the file opener.
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x2, #5");                              // "data:" is the whole scheme
            ctx.emitter.instruction(&format!("b.lt {}", not_data));             // too short to carry it
            for (offset, byte) in b"data:".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset)); // load one candidate scheme byte
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));         // compare against the canonical data: byte
                ctx.emitter.instruction(&format!("b.ne {}", not_data));         // a different prefix is not this wrapper
            }
            ctx.emitter.instruction("mov x0, x1");                              // pass the URI pointer
            ctx.emitter.instruction("mov x1, x2");                              // pass the URI length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdx, 5");                              // "data:" is the whole scheme
            ctx.emitter.instruction(&format!("jl {}", not_data));               // too short to carry it
            for (offset, byte) in b"data:".iter().enumerate() {
                ctx.emitter.instruction(&format!(
                    "cmp BYTE PTR [rax + {}], {}", offset, byte
                ));                                                             // compare one byte against the canonical data: prefix
                ctx.emitter.instruction(&format!("jne {}", not_data));          // a different prefix is not this wrapper
            }
            ctx.emitter.instruction("mov rdi, rax");                            // pass the URI pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the URI length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_data_stream_dynamic");
    box_stream_fd_or_false_result(ctx, "fopen_data_dynamic");
    emit_dynamic_php_filter_finish(ctx, "fopen");                               // a php://filter URL may wrap a data:// resource
    // Wrapper id 7 is `data:`. This said 2, which is `https`, so a data URI built at RUN TIME
    // reported `wrapper_type` = `https` and `stream_type` = `STDIO` where php says `RFC2397` for
    // both — while the literal route, recording 7, was right all along. The two routes have to
    // agree: nothing about the URI changes because its bytes were known earlier.
    emit_record_stream_meta_after_boxed_stashed(ctx, 7);
    emit_record_stream_mode_after_boxed(ctx, expect_operand(inst, 1)?)?;
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    finish_fopen_context_scope(ctx);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done);
    ctx.emitter.label(&not_data);
    Ok(())
}

/// Opens a runtime filename that carries the `php://` prefix, falling through when it does not.
///
/// The literal path resolves its wrapper at compile time; a runtime path had no such dispatch and
/// went to the file opener, so `fopen($path, 'r')` with `php://memory` searched for a FILE of
/// that name. This branch gives the runtime bytes the same treatment, through
/// `__rt_php_wrapper_open`, and leaves everything else — including a `php://` URL the helper does
/// not recognise, which answers `-1` and boxes as `false` — to the paths below.
fn emit_dynamic_php_wrapper_branch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    done: &str,
) -> Result<()> {
    let not_php = ctx.next_label("fopen_dynamic_not_php");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x2, #6");                              // is the filename long enough for php://?
            ctx.emitter.instruction(&format!("b.lt {}", not_php));              // shorter filenames cannot carry the scheme
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset)); // load one candidate scheme byte
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));         // compare against the canonical php:// byte
                ctx.emitter.instruction(&format!("b.ne {}", not_php));          // a different prefix is not this wrapper
            }
            ctx.emitter.instruction("mov x0, x1");                              // pass the filename pointer
            ctx.emitter.instruction("mov x1, x2");                              // pass the filename length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdx, 6");                              // is the filename long enough for php://?
            ctx.emitter.instruction(&format!("jl {}", not_php));                // shorter filenames cannot carry the scheme
            for (offset, byte) in b"php://".iter().enumerate() {
                ctx.emitter.instruction(&format!(
                    "cmp BYTE PTR [rax + {}], {}", offset, byte
                ));                                                             // compare one byte against the canonical php:// prefix
                ctx.emitter.instruction(&format!("jne {}", not_php));           // a different prefix is not this wrapper
            }
            ctx.emitter.instruction("mov rdi, rax");                            // pass the filename pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the filename length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_php_wrapper_open");
    // A php:// URL built at run time can still carry a LITERAL mode, and `php://memory`/`temp`
    // resolve to a bare `tmpfile()` descriptor here exactly as they do on the literal-path side.
    // See the literal branch for why the flag goes on the descriptor rather than into the write.
    if LiteralOpenMode::Operand(expect_operand(inst, 1)?).is_append(ctx)? {
        abi::emit_call_label(ctx.emitter, "__rt_fd_set_append");
    }
    box_stream_fd_or_false_result(ctx, "fopen_php_dynamic");
    emit_dynamic_php_filter_finish(ctx, "fopen");                               // the parked chain, and what php says about the names it could not resolve
    emit_record_stream_meta_after_boxed_stashed(ctx, 6);
    emit_record_stream_mode_after_boxed(ctx, expect_operand(inst, 1)?)?;
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    finish_fopen_context_scope(ctx);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done);
    ctx.emitter.label(&not_php);
    Ok(())
}

/// Refuses a runtime `glob://` filename the way php does, falling through when it is not one.
///
/// php-src registers `glob` with NO `stream_opener` at all, so the generic caller reports the
/// absence itself and no filesystem is ever consulted:
///   `Warning: fopen(glob://*.php): Failed to open stream: wrapper does not support stream open`
/// Without this the URL reached the file opener, which answered `No such file or directory` about
/// a path nothing had looked for. `glob://` still opens as a DIRECTORY; only `fopen()` is refused.
///
/// The line is assembled from three interned fragments rather than a run-time composition: only
/// the URL varies, and it is already in the string registers here.
fn emit_dynamic_glob_branch(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    done: &str,
) -> Result<()> {
    let not_glob = ctx.next_label("fopen_dynamic_not_glob");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x2, #7");                              // "glob://" is the whole scheme
            ctx.emitter.instruction(&format!("b.lt {}", not_glob));             // too short to carry it
            for (offset, byte) in b"glob://".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset)); // load one candidate scheme byte
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));         // compare against the canonical glob:// byte
                ctx.emitter.instruction(&format!("b.ne {}", not_glob));         // a different prefix is not this wrapper
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdx, 7");                              // "glob://" is the whole scheme
            ctx.emitter.instruction(&format!("jl {}", not_glob));               // too short to carry it
            for (offset, byte) in b"glob://".iter().enumerate() {
                ctx.emitter.instruction(&format!(
                    "cmp BYTE PTR [rax + {}], {}", offset, byte
                ));                                                             // compare one byte against the canonical glob:// prefix
                ctx.emitter.instruction(&format!("jne {}", not_glob));          // a different prefix is not this wrapper
            }
        }
    }
    let tail = format!(
        "{}{}\n",
        crate::codegen_support::runtime::io::OPEN_FAILED_MIDDLE,
        crate::codegen_support::runtime::io::GLOB_NO_STREAM_OPEN
    );
    let (tail_label, tail_len) = ctx.data.add_string(tail.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");                   // the URL, across the three calls
            abi::emit_symbol_address(ctx.emitter, "x1", "_diag_open_failed_fopen_prefix");
            abi::emit_load_int_immediate(ctx.emitter, "x2", "Warning: fopen(".len() as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");             // the URL is already the argument pair
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_symbol_address(ctx.emitter, "x1", &tail_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", tail_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                 // the URL, across the three calls
            abi::emit_symbol_address(ctx.emitter, "rdi", "_diag_open_failed_fopen_prefix");
            abi::emit_load_int_immediate(ctx.emitter, "rsi", "Warning: fopen(".len() as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("mov rdi, rax");                            // the URL
            ctx.emitter.instruction("mov rsi, rdx");
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_symbol_address(ctx.emitter, "rdi", &tail_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", tail_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
        }
    }
    emit_fd_result(ctx, -1);
    box_stream_fd_or_false_result(ctx, "fopen_glob_dynamic");
    emit_dynamic_php_filter_finish(ctx, "fopen");
    emit_record_stream_meta_after_boxed_stashed(ctx, 0);
    emit_record_stream_mode_after_boxed(ctx, expect_operand(inst, 1)?)?;
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    finish_fopen_context_scope(ctx);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done);
    ctx.emitter.label(&not_glob);
    Ok(())
}

/// Dispatches a runtime filename to the streaming HTTP opener or generic fopen helper.
fn emit_dynamic_fopen_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let plain = ctx.next_label("fopen_dynamic_plain");
    let done = ctx.next_label("fopen_dynamic_done");
    emit_dynamic_php_filter_swap(ctx, DynamicFilterMode::Staged);
    // Park the hand-off before any opener runs, and republish it at each of the four exits below.
    // Unconditional, exactly like the suppression push: the swap runs for EVERY dynamic filename,
    // and a push that only sometimes happens cannot be popped in one place.
    emit_dynamic_php_filter_save(ctx);
    // php-src's `php_stream_url_wrap_php` returns NULL the moment the INNER resource fails to
    // open, BEFORE a single filter is created, and the generic caller composes one fixed line
    // naming the WHOLE URL with the wrapper's own reason. The swap has just replaced the
    // filename with that resource, so every opener below would otherwise name a path the
    // program never wrote, with an errno php never shows. This silences them for a filter URL
    // only; `__rt_php_filter_open_failed` at each exit is the pop, and composes php's line.
    abi::emit_call_label(ctx.emitter, "__rt_php_filter_suppress_begin");
    emit_dynamic_php_wrapper_branch(ctx, inst, &done)?;
    emit_dynamic_data_branch(ctx, inst, &done)?;
    emit_dynamic_glob_branch(ctx, inst, &done)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x2, #7");                              // is the dynamic filename long enough for http://?
            ctx.emitter.instruction(&format!("b.lt {}", plain));                // shorter filenames use the generic opener
            for (offset, byte) in b"http://".iter().enumerate() {
                ctx.emitter.instruction(&format!("ldrb w9, [x1, #{}]", offset)); // load one dynamic wrapper-prefix byte
                ctx.emitter.instruction(&format!("cmp w9, #{}", byte));         // compare against the canonical http:// byte
                ctx.emitter.instruction(&format!("b.ne {}", plain));            // a different prefix uses the generic opener
            }
            abi::emit_call_label(ctx.emitter, "__rt_http_open_url");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdx, 7");                              // is the dynamic filename long enough for http://?
            ctx.emitter.instruction(&format!("jl {}", plain));                  // shorter filenames use the generic opener
            for (offset, byte) in b"http://".iter().enumerate() {
                ctx.emitter.instruction(&format!(
                    "cmp BYTE PTR [rax + {}], {}", offset, byte
                ));                                                             // compare one byte against the canonical http:// prefix
                ctx.emitter.instruction(&format!("jne {}", plain));             // a different prefix uses the generic opener
            }
            abi::emit_call_label(ctx.emitter, "__rt_http_open_url");
        }
    }
    box_stream_fd_or_false_result(ctx, "fopen_http_dynamic");
    emit_dynamic_php_filter_finish(ctx, "fopen");                               // the parked chain, and what php says about the names it could not resolve
    emit_record_stream_meta_after_boxed_stashed(ctx, 1);
    emit_record_stream_mode_after_boxed(ctx, expect_operand(inst, 1)?)?;
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    finish_fopen_context_scope(ctx);
    store_if_result(ctx, inst)?;
    publish_http_response_headers(ctx);
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&plain);
    abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
    box_stream_fd_or_false_result(ctx, "fopen");
    emit_dynamic_php_filter_finish(ctx, "fopen");                               // the parked chain, and what php says about the names it could not resolve
    emit_record_stream_meta_after_boxed_stashed(ctx, 0);
    emit_record_stream_mode_after_boxed(ctx, expect_operand(inst, 1)?)?;
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    finish_fopen_context_scope(ctx);
    store_if_result(ctx, inst)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Saves the active context bridges, selects and retains the fopen context, and publishes its state.
///
/// Shared with the path-based readers: any builtin taking a `$context` argument must
/// publish it for the duration of its own call, or the wrapper reads whatever context
/// happened to be published last.
pub(super) fn begin_fopen_context_scope(
    ctx: &mut FunctionContext<'_>,
    explicit_context: Option<ValueId>,
) -> Result<()> {
    abi::emit_reserve_temporary_stack(ctx.emitter, 48);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x10, [x9]");                           // save the previously active borrowed options pointer
            ctx.emitter.instruction("str x10, [sp, #0]");                       // preserve options for nested fopen restoration
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_notification_callback");
            ctx.emitter.instruction("ldr x10, [x9]");                           // save the previously active borrowed notifier descriptor
            ctx.emitter.instruction("str x10, [sp, #8]");                       // preserve notifier state for nested fopen restoration
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_current_context_handle");
            ctx.emitter.instruction("ldr x10, [x9]");                           // save the previously active borrowed context handle
            ctx.emitter.instruction("str x10, [sp, #32]");                      // preserve the active handle for nested wrapper restoration
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // save the previously active borrowed options pointer
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], r10");            // preserve options for nested fopen restoration
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_notification_callback");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // save the previously active borrowed notifier descriptor
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r10");            // preserve notifier state for nested fopen restoration
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_current_context_handle");
            ctx.emitter.instruction("mov r10, QWORD PTR [r9]");                 // save the previously active borrowed context handle
            ctx.emitter.instruction("mov QWORD PTR [rsp + 32], r10");           // preserve the active handle for nested wrapper restoration
        }
    }

    let use_default = ctx.next_label("fopen_context_use_default");
    let selected = ctx.next_label("fopen_context_selected");
    match explicit_context {
        None => abi::emit_jump(ctx.emitter, &use_default),
        Some(context) => {
            let raw_ty = ctx.raw_value_php_type(context)?;
            match raw_ty {
                PhpType::Void | PhpType::Never => {
                    abi::emit_jump(ctx.emitter, &use_default);
                }
                // NOTE: `PhpType::Int` deliberately does NOT join this arm. A resource
                // bound to an untyped parameter is narrowed to Int by `codegen_repr()`,
                // and while the handle value survives the call, routing it here still
                // fails `__rt_context_state` validation at runtime. Accepting Int would
                // turn an explicit unsupported-feature diagnostic into an uncaught
                // exception. The real fix is preserving Resource across untyped
                // parameter binding in the checker.
                PhpType::Resource(_) => {
                    ctx.load_value_to_result(context)?;
                    abi::emit_jump(ctx.emitter, &selected);
                }
                PhpType::Mixed | PhpType::Union(_) => {
                    let resource_payload =
                        ctx.next_label("fopen_context_resource_payload");
                    ctx.load_value_to_result(context)?;
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
                    match ctx.emitter.target.arch {
                        Arch::AArch64 => {
                            ctx.emitter.instruction("cmp x0, #8");              // does the explicit Mixed context contain null?
                            ctx.emitter.instruction(&format!("b.eq {}", use_default)); // explicit null selects the request default
                            ctx.emitter.instruction("cmp x0, #9");              // does the explicit Mixed context contain a resource?
                            ctx.emitter.instruction(&format!("b.eq {}", resource_payload)); // resource payload is available in x1
                        }
                        Arch::X86_64 => {
                            ctx.emitter.instruction("cmp rax, 8");              // does the explicit Mixed context contain null?
                            ctx.emitter.instruction(&format!("je {}", use_default)); // explicit null selects the request default
                            ctx.emitter.instruction("cmp rax, 9");              // does the explicit Mixed context contain a resource?
                            ctx.emitter.instruction(&format!("je {}", resource_payload)); // resource payload is available in rdi
                        }
                    }
                    emit_stream_type_error(ctx, "fopen");
                    ctx.emitter.label(&resource_payload);
                    match ctx.emitter.target.arch {
                        Arch::AArch64 => {
                            ctx.emitter.instruction("mov x0, x1");              // expose the unboxed context handle
                        }
                        Arch::X86_64 => {
                            ctx.emitter.instruction("mov rax, rdi");            // expose the unboxed context handle
                        }
                    }
                    abi::emit_jump(ctx.emitter, &selected);
                }
                other => {
                    return Err(CodegenIrError::unsupported(format!(
                        "fopen context argument PHP type {:?}",
                        other
                    )));
                }
            }
        }
    }

    ctx.emitter.label(&use_default);
    emit_request_default_stream_context_handle(ctx);
    abi::emit_jump(ctx.emitter, &selected);

    ctx.emitter.label(&selected);
    let resolved_context = ctx.next_label("fopen_context_resolved");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the selected handle for attach and release
            abi::emit_call_label(ctx.emitter, "__rt_resource_retain");
            abi::emit_call_label(ctx.emitter, "__rt_context_state");
            ctx.emitter.instruction(&format!("cbnz x0, {}", resolved_context)); // continue only with a live ContextState
            emit_closed_stream_type_error(ctx, "fopen");
            ctx.emitter.label(&resolved_context);
            ctx.emitter.instruction("ldr x10, [x0, #0]");                       // load the selected context options pointer
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("str x10, [x9]");                           // publish options, including an explicit empty context
            ctx.emitter.instruction("ldr x10, [x0, #16]");                      // load the selected context notifier descriptor
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_notification_callback");
            ctx.emitter.instruction("str x10, [x9]");                           // publish notifier, including an explicit empty context
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_current_context_handle");
            ctx.emitter.instruction("ldr x10, [sp, #16]");                      // reload the selected context handle
            ctx.emitter.instruction("str x10, [x9]");                           // expose the borrowed handle to user-wrapper construction
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the selected handle for attach and release
            ctx.emitter.instruction("mov rdi, rax");                            // pass the selected handle to registry retain
            abi::emit_call_label(ctx.emitter, "__rt_resource_retain");
            ctx.emitter.instruction("mov rdi, rax");                            // pass the retained handle to typed context lookup
            abi::emit_call_label(ctx.emitter, "__rt_context_state");
            ctx.emitter.instruction("test rax, rax");                           // did the selected handle resolve to ContextState?
            ctx.emitter.instruction(&format!("jnz {}", resolved_context));      // continue only with a live ContextState
            emit_closed_stream_type_error(ctx, "fopen");
            ctx.emitter.label(&resolved_context);
            ctx.emitter.instruction("mov r10, QWORD PTR [rax + 0]");            // load the selected context options pointer
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov QWORD PTR [r9], r10");                 // publish options, including an explicit empty context
            ctx.emitter.instruction("mov r10, QWORD PTR [rax + 16]");           // load the selected context notifier descriptor
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_notification_callback");
            ctx.emitter.instruction("mov QWORD PTR [r9], r10");                 // publish notifier, including an explicit empty context
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_current_context_handle");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");           // reload the selected context handle
            ctx.emitter.instruction("mov QWORD PTR [r9], r10");                 // expose the borrowed handle to user-wrapper construction
        }
    }
    Ok(())
}

/// Restores the prior context bridges and transfers one retained owner to a successful stream.
pub(super) fn finish_fopen_context_scope(ctx: &mut FunctionContext<'_>) {
    let restore = ctx.next_label("fopen_context_restore");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #24]");                       // preserve the boxed fopen result across context cleanup
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the boxed fopen result tag
            ctx.emitter.instruction("cmp x9, #9");                              // did fopen return a stream resource?
            ctx.emitter.instruction(&format!("b.ne {}", restore));              // false results have no StreamState to attach
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // load the opaque stream handle payload
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // load the selected context handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_attach_context");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rax");           // preserve the boxed fopen result across context cleanup
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // did fopen return a stream resource?
            ctx.emitter.instruction(&format!("jne {}", restore));               // false results have no StreamState to attach
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // load the opaque stream handle payload
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // load the selected context handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_attach_context");
        }
    }
    ctx.emitter.label(&restore);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_context_options");
            ctx.emitter.instruction("ldr x10, [sp, #0]");                       // reload the previously active options pointer
            ctx.emitter.instruction("str x10, [x9]");                           // restore the outer options bridge before release
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_notification_callback");
            ctx.emitter.instruction("ldr x10, [sp, #8]");                       // reload the previously active notifier descriptor
            ctx.emitter.instruction("str x10, [x9]");                           // restore the outer notifier bridge before release
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_current_context_handle");
            ctx.emitter.instruction("ldr x10, [sp, #32]");                      // reload the previously active borrowed context handle
            ctx.emitter.instruction("str x10, [x9]");                           // restore the outer wrapper context bridge
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // load the temporary selected-context owner
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("ldr x0, [sp, #24]");                       // restore the boxed fopen result
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_context_options");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 0]");            // reload the previously active options pointer
            ctx.emitter.instruction("mov QWORD PTR [r9], r10");                 // restore the outer options bridge before release
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_notification_callback");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 8]");            // reload the previously active notifier descriptor
            ctx.emitter.instruction("mov QWORD PTR [r9], r10");                 // restore the outer notifier bridge before release
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_current_context_handle");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 32]");           // reload the previously active borrowed context handle
            ctx.emitter.instruction("mov QWORD PTR [r9], r10");                 // restore the outer wrapper context bridge
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // load the temporary selected-context owner
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 24]");           // restore the boxed fopen result
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, 48);
}

/// Lazily creates the request-default context and leaves its global-owned handle in the result.
pub(super) fn emit_request_default_stream_context_handle(ctx: &mut FunctionContext<'_>) {
    let ready = ctx.next_label("fopen_default_context_ready");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_default_context_handle");
            ctx.emitter.instruction("ldr x0, [x9]");                            // load the request-global default context handle
            ctx.emitter.instruction(&format!("cbnz x0, {}", ready));            // reuse the existing request default
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_default_context_handle");
            ctx.emitter.instruction("mov rax, QWORD PTR [r9]");                 // load the request-global default context handle
            ctx.emitter.instruction("test rax, rax");                           // has the request default been allocated?
            ctx.emitter.instruction(&format!("jnz {}", ready));                 // reuse the existing request default
        }
    }
    clear_stream_context_options(ctx);
    clear_stream_notification_callback(ctx);
    emit_dynamic_stream_context_allocation(ctx, "fopen_default_context");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_default_context_handle");
            ctx.emitter.instruction("str x0, [x9]");                            // transfer the creator reference to the request-global owner
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_default_context_handle");
            ctx.emitter.instruction("mov QWORD PTR [r9], rax");                 // transfer the creator reference to the request-global owner
        }
    }
    ctx.emitter.label(&ready);
}

/// Emits the boxed `fopen()` result for a compile-time literal path without storing it.
/// Where a literal open gets its mode.
///
/// `fopen()` reads it from the call's second argument. `file_get_contents()` has no such
/// argument, and its second operand is `$use_include_path` — reading that as a mode is how
/// this helper stayed unusable from there.
#[derive(Clone, Copy)]
pub(super) enum LiteralOpenMode {
    /// The mode string the call passed.
    Operand(ValueId),
    /// A read-only open, fixed by a caller that has no `$mode` argument.
    ReadOnly,
}

impl LiteralOpenMode {
    /// Whether this open writes. A caller-fixed read-only open never does.
    fn is_write(self, ctx: &mut FunctionContext<'_>) -> Result<bool> {
        match self {
            LiteralOpenMode::Operand(mode) => literal_fopen_mode_is_write(ctx, mode),
            LiteralOpenMode::ReadOnly => Ok(false),
        }
    }

    /// Whether this open is an APPEND mode, in which php sends every write to the end.
    ///
    /// php-src searches the whole mode string, so `a`, `a+` and `ab+` all append. A mode that is
    /// not a compile-time literal answers false, which is the mode the temp-file backend already
    /// creates — that keeps a dynamic mode no worse than it was rather than guessing.
    fn is_append(self, ctx: &FunctionContext<'_>) -> Result<bool> {
        match self {
            LiteralOpenMode::ReadOnly => Ok(false),
            LiteralOpenMode::Operand(mode) => Ok(optional_const_string_operand(ctx, mode)?
                .is_some_and(|text| text.contains('a'))),
        }
    }

    /// The `(read, write)` filter directions php derives from the open mode.
    ///
    /// php-src searches the WHOLE mode string with `strchr`, so `rb` reads, `a` writes and
    /// `r+` does both — and a mode naming none of them, `x`, selects NEITHER, which is why
    /// `php://filter/no.such/resource=...` opened with `"x"` warns not at all while the same
    /// URL opened with `"r+"` warns TWICE over. Measured on `php -n` 8.5.6.
    ///
    /// A mode that is not a compile-time literal answers read-only: it is the overwhelmingly
    /// common open, and an explicit `read=`/`write=` list ignores the mode entirely anyway.
    fn filter_directions(self, ctx: &FunctionContext<'_>) -> Result<(bool, bool)> {
        let text = match self {
            LiteralOpenMode::ReadOnly => "r".to_string(),
            LiteralOpenMode::Operand(mode) => {
                optional_const_string_operand(ctx, mode)?.unwrap_or_else(|| "r".to_string())
            }
        };
        Ok((
            text.contains('r') || text.contains('+'),
            text.contains('w') || text.contains('a') || text.contains('+'),
        ))
    }
}

/// The warning LINES php prints for a URL a built-in wrapper refuses to open at all.
///
/// `None` means this URL is not one of them and the ordinary openers below decide. Every line is
/// complete, newline included, because all of it is known here — the URL is a literal.
///
/// All measured on `php -n` 8.5.6:
///
/// ```text
/// fopen("php://bogus","r")   Warning: fopen(): Invalid php:// URL specified
///                            Warning: fopen(php://bogus): Failed to open stream: operation failed
/// fopen("php://fd/","r")     Warning: fopen(php://fd/): Failed to open stream:
///                                     php://fd/ stream must be specified in the form php://fd/<orig fd>
/// fopen("php://fd/abc","r")  the same sentence: php-src refuses anything `strtol` does not
///                            consume WHOLE, so a trailing byte reads like no number at all
/// fopen("glob://*.php","r")  Warning: fopen(glob://*.php): Failed to open stream:
///                                     wrapper does not support stream open
/// ```
///
/// The php:// case is the only one that prints TWO lines, and the reason it does is structural:
/// `php_stream_url_wrap_php` reports through a DIRECT `php_error_docref`, which prints at once as
/// `fopen(): …` and leaves the wrapper error stack empty — so the generic failed-open line that
/// follows has nothing left to say but `operation failed`. Every other wrapper here goes through
/// `php_stream_wrapper_log_error`, whose message IS the reason in the single line.
///
/// elephc used to send all of these to the FILE opener, which reported `No such file or
/// directory` for a path no filesystem was ever asked about — or, for the dynamic route, said
/// nothing at all.
fn literal_wrapper_refusal(path: &str) -> Option<Vec<String>> {
    if let Some(target) = path.strip_prefix("php://") {
        // Everything php-src's `php_stream_url_wrap_php` knows how to open. `temp` takes an
        // optional `/maxmemory:N`, and `filter` is resolved long before this point.
        let known = matches!(
            target,
            "stdin" | "stdout" | "stderr" | "input" | "output" | "memory" | "temp"
        ) || target.starts_with("temp/")
            || target.starts_with("filter/")
            // `php://fd/` names a stream only when a NUMBER follows; the descriptor it names
            // may still be refused, but by the run-time opener and in php's other wording.
            || target
                .strip_prefix("fd/")
                .is_some_and(|number| php_fd_number(number).is_some());
        if known {
            return None;
        }
        if target.starts_with("fd/") {
            return Some(vec![format!(
                "Warning: fopen({path}): Failed to open stream: \
                 php://fd/ stream must be specified in the form php://fd/<orig fd>\n"
            )]);
        }
        return Some(vec![
            "Warning: fopen(): Invalid php:// URL specified\n".to_string(),
            format!("Warning: fopen({path}): Failed to open stream: operation failed\n"),
        ]);
    }
    if path.starts_with("glob://") {
        // php-src registers `glob` with no `stream_opener` at all, so the generic caller reports
        // the absence rather than any wrapper of its own. `glob://` still opens as a DIRECTORY.
        return Some(vec![format!(
            "Warning: fopen({path}): Failed to open stream: {}\n",
            crate::codegen_support::runtime::io::GLOB_NO_STREAM_OPEN
        )]);
    }
    None
}

pub(super) fn emit_literal_fopen_result(
    ctx: &mut FunctionContext<'_>,
    mode: LiteralOpenMode,
    path: &str,
) -> Result<()> {
    if let Some(fd) = php_standard_stream_fd(path) {
        emit_dup_fd_result(ctx, fd);
        box_stream_fd_or_false_result(ctx, "fopen");
        emit_record_stream_meta_after_boxed_literal(ctx, 6, path);
        return Ok(());
    }
    if let Some(fd) = php_fd_stream(path) {
        emit_php_fd_open_result(ctx, fd, path);
        box_stream_fd_or_false_result(ctx, "fopen");
        emit_record_stream_meta_after_boxed_literal(ctx, 6, path);
        return Ok(());
    }
    if is_php_memory_stream(path) {
        abi::emit_call_label(ctx.emitter, "__rt_tmpfile");
        // php's `a` modes send every write to the END of the stream, whatever `fseek()` did:
        // `fopen("php://temp","a+")`, write `hello`, `fseek(0)`, write `world` answers
        // `helloworld`. A real file gets that from `O_APPEND` at `open()`, but this backend is a
        // `tmpfile()` descriptor created with no mode at all, so the second write OVERWROTE and
        // the stream silently lost the first one. Setting the flag on the descriptor reuses the
        // append bookkeeping files already have rather than adding a second one.
        if mode.is_append(ctx)? {
            abi::emit_call_label(ctx.emitter, "__rt_fd_set_append");
        }
        box_stream_fd_or_false_result(ctx, "fopen");
        emit_record_stream_meta_after_boxed_literal(ctx, 6, path);
        return Ok(());
    }
    if let Some(lines) = literal_wrapper_refusal(path) {
        for line in &lines {
            emit_static_diag_warning(ctx, line);
        }
        emit_fd_result(ctx, -1);
        box_stream_fd_or_false_result(ctx, "fopen_wrapper_refused");
        return Ok(());
    }
    // `data:` is the whole scheme; php-src makes the `//` optional for this one wrapper.
    if path.starts_with("data:") {
        return emit_literal_data_fopen_result(ctx, path);
    }
    if path.starts_with("ftp://") {
        return emit_literal_ftp_fopen_result(ctx, path);
    }
    if path.starts_with("phar://") {
        if mode.is_write(ctx)? {
            return emit_literal_phar_fopen_write_result(ctx, path);
        }
        return emit_literal_phar_fopen_read_result(ctx, path);
    }
    if path.starts_with("zip://") {
        // Unlike `phar://`, a literal `zip://` URL is NOT extracted at compile time: php's zip
        // wrapper reads the archive when the program runs, so an archive the program itself
        // wrote a line earlier has to be visible. Publishing the bridge and falling through to
        // the generic runtime open is what makes the run-time read happen; `__rt_fopen_maybe_phar`
        // recognises the scheme and hands the whole URL to the bridge.
        publish_zip_bridge_function_pointer(ctx);
        return emit_runtime_fopen_literal_result(ctx, path, mode);
    }
    if path.starts_with("http://") {
        return emit_literal_http_fopen_result(ctx, path);
    }
    emit_runtime_fopen_literal_result(ctx, path, mode)
}

/// Emits a runtime `fopen()` call for a literal path and the caller's mode operand.
/// Loads the open mode into the string result registers, materializing `"r"` for a
/// caller-fixed read-only open.
fn emit_literal_open_mode_string(
    ctx: &mut FunctionContext<'_>,
    mode: LiteralOpenMode,
) -> Result<()> {
    match mode {
        LiteralOpenMode::Operand(mode) => load_string_to_result(ctx, mode, "fopen mode"),
        LiteralOpenMode::ReadOnly => {
            let (label, len) = ctx.data.add_string(b"r");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x1", &label);
                    ctx.emitter.instruction(&format!("mov x2, #{}", len));      // the fixed read-only mode
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rax", &label);
                    ctx.emitter.instruction(&format!("mov rdx, {}", len));      // the fixed read-only mode
                }
            }
            Ok(())
        }
    }
}

pub(super) fn emit_runtime_fopen_literal_result(
    ctx: &mut FunctionContext<'_>,
    path: &str,
    mode: LiteralOpenMode,
) -> Result<()> {
    let (path_label, path_len) = ctx.data.add_string(path.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &path_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", path_len));         // pass the literal fopen path byte length
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            emit_literal_open_mode_string(ctx, mode)?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the fopen mode pointer with the literal path
            ctx.emitter.instruction("mov x4, x2");                              // pass the fopen mode length with the literal path
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rax", &path_label);
            ctx.emitter.instruction(&format!("mov rdx, {}", path_len));         // pass the literal fopen path byte length
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            emit_literal_open_mode_string(ctx, mode)?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the fopen mode pointer with the literal path
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the fopen mode length with the literal path
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_fopen_maybe_phar");
    box_stream_fd_or_false_result(ctx, "fopen");
    emit_record_stream_meta_after_boxed_literal(ctx, 0, path);
    Ok(())
}

/// Emits a literal `fopen("php://filter/...", ...)` result without storing it.
/// `callee` is the function php NAMES in the two diagnostics this can print. Every route that
/// reaches a literal filter URL owns one — `fopen`, `file_get_contents` — and php words both the
/// failed-open line and the unresolved-name warnings with it, so it cannot be hardcoded here.
pub(super) fn emit_literal_php_filter_fopen_result(
    ctx: &mut FunctionContext<'_>,
    mode: LiteralOpenMode,
    path: &str,
    callee: &str,
) -> Result<()> {
    let Some(parsed) = parse_php_filter_url(path) else {
        // php THROWS for a filter URL that names no resource — `Error: No URL resource
        // specified`, not a warning, and `@` does not soften it. A NESTED resource is the
        // other reason the parse declines; php recurses there, which is a separate,
        // still-open divergence, so it keeps the loud failed-open path.
        if literal_filter_url_names_no_resource(path) {
            crate::codegen::lower_inst::exceptions::emit_error(ctx, "No URL resource specified");
            return Ok(());
        }
        emit_fd_result(ctx, -1);
        box_stream_fd_or_false_result(ctx, "fopen_php_filter");
        return Ok(());
    };
    let (mode_read, mode_write) = mode.filter_directions(ctx)?;
    // php-src's `php_stream_url_wrap_php` returns NULL the moment the INNER resource fails to
    // open, BEFORE a single filter is created, and the generic caller composes one fixed line
    // naming the WHOLE URL with the wrapper's own reason:
    //   Warning: fopen(php://filter/read=string.toupper/resource=missing.txt):
    //            Failed to open stream: operation failed
    // The inner opener names ITSELF and the bare resource with its own errno — this used to
    // print `fopen(missing.txt): ... No such file or directory`, which names a path the program
    // never wrote. Its warnings are suppressed through the FILTER counter — not the one `@`
    // raises — and the php-worded line is composed from the literal URL below, exactly as the
    // literal `file_get_contents` route already does for the same URLs. The resource may be a
    // user wrapper, whose `stream_open` is PHP that php lets warn; `__rt_fopen` stands this
    // scope down for the dispatch, which only works because `@` does not share the counter.
    abi::emit_call_label(ctx.emitter, "__rt_diag_push_filter_suppression");
    emit_literal_fopen_result(ctx, mode, &parsed.resource)?;
    abi::emit_call_label(ctx.emitter, "__rt_diag_pop_filter_suppression");      // preserves the boxed result: x9/x10 (r10) only
    let opened = ctx.next_label("fopen_filter_lit_opened");
    let done = ctx.next_label("fopen_filter_lit_done");
    let (url_label, url_len) = ctx.data.add_string(path.as_bytes());
    let fail_prefix = format!("Warning: {callee}(");
    let (prefix_label, prefix_len) = ctx.data.add_string(fail_prefix.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // the boxed open result tag
            ctx.emitter.instruction("cmp x9, #9");                              // a resource has nothing to warn about
            ctx.emitter.instruction(&format!("b.eq {}", opened));
            abi::emit_push_reg(ctx.emitter, "x0");                              // hold the boxed false across the fragments
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "x1", &url_label);            // the literal URL, not the resource
            abi::emit_load_int_immediate(ctx.emitter, "x2", url_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "x1", "_fgc_filter_fail_tail");
            ctx.emitter.instruction(&format!(
                "mov x2, #{}",
                crate::codegen_support::runtime::data::FGC_FILTER_FAIL_TAIL.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // the boxed open result tag
            ctx.emitter.instruction("cmp r9, 9");                               // a resource has nothing to warn about
            ctx.emitter.instruction(&format!("je {}", opened));
            abi::emit_push_reg(ctx.emitter, "rax");                             // hold the boxed false across the fragments
            abi::emit_symbol_address(ctx.emitter, "rdi", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", prefix_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "rdi", &url_label);           // the literal URL, not the resource
            abi::emit_load_int_immediate(ctx.emitter, "rsi", url_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_symbol_address(ctx.emitter, "rdi", "_fgc_filter_fail_tail");
            ctx.emitter.instruction(&format!(
                "mov rsi, {}",
                crate::codegen_support::runtime::data::FGC_FILTER_FAIL_TAIL.len()
            ));
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    // A failed open never reaches the filters, so the unknown-name warnings below belong to the
    // SUCCESS path only — measured: `fopen("php://filter/read=no.such/resource=missing.txt")`
    // prints the failed-open line and nothing else.
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&opened);
    // The URL was resolved by `php_stream_url_wrap_php`, so the stream belongs to the `php`
    // wrapper however ordinary the resource behind it is: measured on `php -n` 8.5.6, a filter
    // over a plain file reports `wrapper_type` `PHP`. elephc left the INNER opener's identity on
    // the handle and called it `plainfile`.
    //
    // Only a plain-path resource is re-stamped. `stream_type` is derived from the wrapper id and
    // the recorded URI, and php keeps the INNER one there — a filter over `php://memory` still
    // reports `MEMORY`. Those resources already record wrapper id 6 themselves, so re-stamping
    // them would buy nothing and would rewrite the URI the namer reads. What stays divergent for
    // them is `uri`, which php reports as the whole filter URL and elephc as the inner one.
    if !parsed.resource.contains("://") && !parsed.resource.starts_with("data:") {
        emit_record_stream_meta_after_boxed_literal(ctx, 6, path);
    }
    if parsed.mode_bits != 0 {
        emit_php_filter_table_stamps(ctx, parsed.mode_bits, &parsed.filter_ids);
    }
    emit_unknown_filter_warnings(ctx, &parsed.unknown, mode_read, mode_write, callee);
    ctx.emitter.label(&done);
    Ok(())
}

/// Warns for every `php://filter` name that named no filter, and STILL keeps the stream.
///
/// php answers an unknown name with TWO lines — `php_stream_filter_create` reports that it
/// cannot locate the filter, then `php_stream_apply_filter_list` reports that it cannot create
/// it — and neither cancels the open, so the caller still receives a live stream. elephc
/// resolved the same URL, quietly skipped the name and said nothing, which turns a typo in a
/// filter name into a silently unfiltered read.
///
/// The count is not one pair per name: php walks the list once per DIRECTION it applies, so a
/// no-prefix chain opened `r+` warns twice per name (read attempt, then write attempt) while
/// the same chain opened `x` — a mode naming neither direction — warns not at all. An explicit
/// `read=`/`write=` list is always applied exactly once, whatever the mode. All measured on
/// `php -n` 8.5.6.
fn emit_unknown_filter_warnings(
    ctx: &mut FunctionContext<'_>,
    unknown: &[UnknownFilterName],
    mode_read: bool,
    mode_write: bool,
    callee: &str,
) {
    if unknown.is_empty() {
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(ctx.emitter, "x0"),                 // hold the boxed stream across the fragments
        Arch::X86_64 => abi::emit_push_reg(ctx.emitter, "rax"),
    }
    for entry in unknown {
        let attempts = if entry.direction == 3 {
            usize::from(mode_read) + usize::from(mode_write)
        } else {
            1
        };
        // Every fragment is fully known here, so each warning is ONE interned string and one
        // call — nothing is assembled at run time.
        let locate =
            format!("Warning: {callee}(): Unable to locate filter \"{}\"\n", entry.name);
        let create =
            format!("Warning: {callee}(): Unable to create filter ({})\n", entry.name);
        for _ in 0..attempts {
            emit_static_diag_warning(ctx, &locate);
            emit_static_diag_warning(ctx, &create);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_pop_reg(ctx.emitter, "x0"),
        Arch::X86_64 => abi::emit_pop_reg(ctx.emitter, "rax"),
    }
}

/// Emits one whole warning line whose text is known at compile time.
///
/// Goes through `__rt_diag_warning` like every other warning, so `@` suppresses it through the
/// shared depth counter rather than through a rule of its own.
pub(super) fn emit_static_diag_warning(ctx: &mut FunctionContext<'_>, text: &str) {
    let (label, len) = ctx.data.add_string(text.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Returns whether a literal `php://filter/...` URL names NO resource — missing or empty.
///
/// This is the case php answers with `Error: No URL resource specified`; a nested resource
/// also makes [`parse_php_filter_url`] decline, but that is a different php behavior.
pub(super) fn literal_filter_url_names_no_resource(path: &str) -> bool {
    match path
        .strip_prefix("php://filter/")
        .map(|spec| spec.split_once("/resource="))
    {
        Some(None) => true,                                  // no separator at all
        Some(Some((_, resource))) => resource.is_empty(),    // an empty resource names nothing
        None => false,                                       // not a filter URL: not this error
    }
}

/// Parses `php://filter/[read=|write=]a|b|.../resource=path` for literal `fopen`.
///
/// Every name in the `|` chain is resolved, in order: php-src runs the record through all
/// of them. An unrecognised name is skipped and the rest still apply, which is what
/// `php -n` does — it is not an error and it does not cancel the chain. It is not SILENT
/// either, which is why the unresolved names come back in [`PhpFilterUrl::unknown`].
pub(super) fn parse_php_filter_url(path: &str) -> Option<PhpFilterUrl> {
    let spec = path.strip_prefix("php://filter/")?;
    let (filter_part, resource) = spec.split_once("/resource=")?;
    if resource.is_empty() {
        return None;
    }
    let (mode_bits, filters) = if let Some(filters) = filter_part.strip_prefix("read=") {
        (1u8, filters)
    } else if let Some(filters) = filter_part.strip_prefix("write=") {
        (2u8, filters)
    } else {
        (3u8, filter_part)
    };
    // An unrecognised name is SKIPPED, not fatal, and does not cancel its neighbours:
    // `php -n` 8.5.6 opens `read=string.toupper|no.such.filter` successfully and returns the
    // uppercased bytes. Measured, because the opposite reading is just as plausible.
    let filter_ids: Vec<u8> = filters.split('|').filter_map(stream_filter_id).collect();
    // An EMPTY segment names nothing at all and php says nothing about it: `read=` on its own
    // opens in silence, because php-src walks the list with `php_strtok_r`, which skips empty
    // tokens rather than trying to create a filter called "".
    let unknown: Vec<UnknownFilterName> = filters
        .split('|')
        .filter(|name| !name.is_empty() && stream_filter_id(name).is_none())
        .map(|name| UnknownFilterName { name: name.to_string(), direction: mode_bits })
        .collect();
    let mode_bits = if filter_ids.is_empty() { 0 } else { mode_bits };
    // A NESTED resource recurses, as php does: the inner level sits closest to the bytes, so
    // its chain applies FIRST and the outer chain sees what the inner one produced —
    // `read=string.rot13/resource=php://filter/read=string.toupper/resource=x` uppercases and
    // THEN rot13s, measured. Levels with conflicting explicit directions stay refused (the
    // pending hand-off carries one direction), which keeps that exotic spelling loudly failing
    // rather than half-filtered.
    if resource.starts_with("php://filter/") {
        let inner = parse_php_filter_url(resource)?;
        if inner.mode_bits != 0 && mode_bits != 0 && inner.mode_bits != mode_bits {
            return None;
        }
        let bits = if mode_bits == 0 { inner.mode_bits } else { mode_bits };
        let mut ids = inner.filter_ids;
        ids.extend(filter_ids);
        // The inner level is opened first, so php reaches its filter names first as well.
        let mut names = inner.unknown;
        names.extend(unknown);
        return Some(PhpFilterUrl {
            mode_bits: bits,
            filter_ids: ids,
            unknown: names,
            resource: inner.resource,
        });
    }
    Some(PhpFilterUrl { mode_bits, filter_ids, unknown, resource: resource.to_string() })
}

/// A name from a `php://filter` chain that resolves to no built-in filter.
///
/// php does not drop these silently — it warns twice for every creation that fails — so the
/// parse has to hand the names back, which is all the old `filter_map` threw away.
pub(super) struct UnknownFilterName {
    /// The name exactly as the URL spelled it.
    pub(super) name: String,
    /// The direction its OWN level named: 1 = `read=`, 2 = `write=`, 3 = no prefix.
    ///
    /// Kept per name because a nested URL can spell a different direction at each level, and
    /// the no-prefix spelling is the only one whose warning count depends on the open mode.
    pub(super) direction: u8,
}

/// Everything a literal `php://filter/...` URL resolves to at compile time.
pub(super) struct PhpFilterUrl {
    /// Direction bits for the resolved filters: 1 = read, 2 = write, 3 = both, 0 = none resolved.
    pub(super) mode_bits: u8,
    /// The built-in filter ids to stamp, innermost level first.
    pub(super) filter_ids: Vec<u8>,
    /// The names that resolved to nothing, in the order php tries to create them.
    pub(super) unknown: Vec<UnknownFilterName>,
    /// The resource the whole URL finally opens.
    pub(super) resource: String,
}

