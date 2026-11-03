//! Purpose:
//! File and stream builtins: the `FileBuiltin` helper table, its shape predicate, lowering and the stream tuner calls.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates the three stream tuners: a stream handle and an integer size in, an int out.
///
/// The handle must be a BOXED resource, which is what `fopen` answers here and what
/// `__rt_stream_fd` unboxes. php raises a `TypeError` for anything else, and this target has
/// no such raise at a builtin argument, so the audit refuses the call instead.
pub(super) fn stream_tuner_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [handle, size] = call.operands.as_slice() else {
        return Some(format!(
            "{target:?} expects a stream and a size, got {} operands",
            call.operands.len()
        ));
    };
    let Some(handle) = function.value(*handle) else {
        return Some("stream operand is missing from the value table".to_string());
    };
    if handle.ir_type != IrType::Heap(IrHeapKind::Mixed) {
        return Some(format!(
            "{target:?} takes a boxed stream handle, not {:?}",
            handle.ir_type
        ));
    }
    let Some(size) = function.value(*size) else {
        return Some("size operand is missing from the value table".to_string());
    };
    if size.ir_type != IrType::I64 || size.php_type.codegen_repr() != PhpType::Int {
        return Some(format!(
            "{target:?} takes an int size, not {:?}/{:?}",
            size.ir_type,
            size.php_type.codegen_repr()
        ));
    }
    if call.result_type != IrType::I64 {
        return Some(format!(
            "{target:?} answers an int, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers the three stream tuners.
///
/// Only `stream_set_chunk_size` carries state; the other two answer a CONSTANT php picked and
/// this target has no way to make untrue. Their operands are still evaluated and dropped,
/// because a discarded argument may be a call with effects of its own.
///
/// The constants are measured, not assumed: php-src 8.5.10 answers `0` for the read buffer and
/// **-1** for the write buffer, on every stream kind this target can produce. -1 is php's "the
/// stream has no such option", which is exactly true here — and it is also where the NATIVE
/// backend diverges, answering 0.
pub(super) fn lower_stream_tuner(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    if target != RuntimeFnId::StreamSetChunkSize {
        for index in 0..2 {
            ctx.emit_load_value(operand(inst, index)?)?;
            if index == 0 {
                ctx.fb.ins("drop", "the handle is not consulted");
            } else {
                ctx.fb.ins("drop", "the size cannot take effect here");
            }
        }
        ctx.fb.ins(
            if target == RuntimeFnId::StreamSetReadBuffer {
                "i64.const 0"
            } else {
                "i64.const -1"
            },
            "php's answer for this stream option",
        );
        return store_result(ctx, inst);
    }

    // php raises before recording, and leaves the previous size in place — measured: after a
    // refused call the next one still reports what was there before it. So the guard runs
    // first and the helper is never reached with a non-positive size.
    let size = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.set {size}"), "spill the requested size");
    ctx.fb.ins(&format!("local.get {size}"), "requested size");
    ctx.fb.ins("i64.const 1", "php's lower bound");
    ctx.fb.ins("i64.lt_s", "non-positive size?");
    ctx.fb.ins("if", "stream_set_chunk_size() rejects a non-positive size");
    crate::codegen_wasm::inst::emit_runtime_failure(
        ctx,
        crate::codegen_wasm::streamtune::CHUNK_SIZE_FAILURE_CODE,
        "stream_set_chunk_size() non-positive size",
    );
    ctx.fb.ins("end", "end non-positive-size guard");
    ctx.fb.ins(&format!("local.get {size}"), "requested size");
    ctx.fb.ins("i64.const 2147483647", "php's INT_MAX upper bound");
    ctx.fb.ins("i64.gt_s", "past INT_MAX?");
    ctx.fb.ins("if", "stream_set_chunk_size() rejects a size past INT_MAX");
    crate::codegen_wasm::inst::emit_runtime_failure(
        ctx,
        crate::codegen_wasm::streamtune::CHUNK_SIZE_TOO_LARGE_FAILURE_CODE,
        "stream_set_chunk_size() size past INT_MAX",
    );
    ctx.fb.ins("end", "end too-large-size guard");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.get {size}"), "the validated size");
    ctx.fb.ins(
        "call $__rt_stream_chunk_size",
        "record it and answer the previous one",
    );
    store_result(ctx, inst)
}

/// One file builtin: the runtime helper serving it, and the storage its call site must have.
///
/// The whole family is a straight forwarding of operands to a WASI-backed helper, so the only
/// per-builtin facts are the symbol and the shape. `operands` and `result` are IR types, checked
/// by the capability audit before this module emits anything, so the emitter can load and call
/// without re-deciding what each position holds.
pub(in crate::codegen_wasm) struct FileBuiltin {
    /// The `__rt_*` helper the call lowers to.
    pub(in crate::codegen_wasm) helper: &'static str,
    /// The IR type each operand must have, in order.
    pub(in crate::codegen_wasm) operands: &'static [IrType],
    /// The IR type the result must have.
    pub(in crate::codegen_wasm) result: IrType,
    /// Whether the helper takes a trailing warn flag the call site does not carry.
    pub(in crate::codegen_wasm) warns: bool,
    /// Defaults for the trailing operands a call site may omit, in declaration order.
    ///
    /// `fseek($h, $off)` and `stream_get_contents($h)` are both legal PHP — the registry
    /// declares the missing arguments with defaults — but the call carries fewer operands than
    /// the helper takes, so the defaults are materialized at the call rather than requiring
    /// every caller to spell them.
    pub(in crate::codegen_wasm) optional_tail: &'static [i64],
}

/// Returns the file-runtime contract for `target`, and nothing for any other builtin.
///
/// A stream handle is a boxed Mixed cell carrying the WASI fd as a resource payload, which is
/// why `fopen` answers `Heap(Mixed)` and the three that take a handle read one. `fopen` and
/// `file_get_contents` also answer `false` through that same cell when the path cannot be
/// opened, which is exactly what PHP answers.
pub(in crate::codegen_wasm) fn file_builtin_helper(target: RuntimeFnId) -> Option<FileBuiltin> {
    const STREAM: IrType = IrType::Heap(IrHeapKind::Mixed);
    let (helper, operands, result): (_, &'static [IrType], _) = match target {
        RuntimeFnId::Fopen => ("$__rt_fopen", &[IrType::Str, IrType::Str], STREAM),
        // `fwrite` answers `int|false` upstream now, so the count comes back boxed.
        RuntimeFnId::Fwrite => ("$__rt_fwrite_boxed", &[STREAM, IrType::Str], STREAM),
        RuntimeFnId::Fread => ("$__rt_fread", &[STREAM, IrType::I64], IrType::Str),
        RuntimeFnId::Fclose => ("$__rt_fclose", &[STREAM], IrType::I64),
        // The position queries. `ftell` answers both stream kinds: WASI has no `fd_tell`, but a
        // zero-length seek from the current position is the same question.
        RuntimeFnId::Feof => ("$__rt_feof", &[STREAM], IrType::I64),
        RuntimeFnId::Ftell => ("$__rt_ftell", &[STREAM], IrType::I64),
        RuntimeFnId::Rewind => ("$__rt_rewind", &[STREAM], IrType::I64),
        RuntimeFnId::Fseek => (
            "$__rt_fseek",
            &[STREAM, IrType::I64, IrType::I64],
            IrType::I64,
        ),
        // Answers `string|false`, so the result is a boxed cell like `file_get_contents`.
        RuntimeFnId::StreamGetContents => (
            "$__rt_stream_get_contents",
            &[STREAM, IrType::I64, IrType::I64],
            STREAM,
        ),
        // Answers `int|false`, so its result is a boxed cell too.
        RuntimeFnId::StreamCopyToStream => (
            "$__rt_stream_copy_to_stream",
            &[STREAM, STREAM, IrType::I64, IrType::I64],
            STREAM,
        ),
        // Answers `string|false`. The delimiter's PHP default is the STRING `""`,
        // which the i64-only optional tail cannot spell — so all three operands are
        // required here and a call that omits the delimiter keeps refusing fail-closed.
        RuntimeFnId::StreamGetLine => (
            "$__rt_stream_get_line",
            &[STREAM, IrType::I64, IrType::Str],
            STREAM,
        ),
        // Not a FILE, but the same WASI-forwarding pattern: the environment is process
        // state `environ_get` serves. Answers `string|false`, so the result is a boxed
        // cell; the registry's single form always carries the one Str operand.
        RuntimeFnId::Getenv => ("$__rt_getenv", &[IrType::Str], STREAM),
        // Answers php's nine-key metadata array, boxed as a hash-tagged cell.
        RuntimeFnId::StreamGetMetaData => (
            "$__rt_stream_get_meta_data",
            &[STREAM],
            STREAM,
        ),
        // `string|false`, so a boxed cell: php distinguishes a NUL byte from end of file.
        RuntimeFnId::Fgetc => ("$__rt_fgetc", &[STREAM], STREAM),
        // `int|false` — the byte count written to stdout.
        RuntimeFnId::Readfile => ("$__rt_readfile", &[IrType::Str], STREAM),
        RuntimeFnId::Flock => ("$__rt_flock", &[STREAM, IrType::I64], IrType::I64),
        // No operands at all: php's `tmpfile()` takes none and answers a handle or false.
        RuntimeFnId::Tmpfile => ("$__rt_tmpfile", &[], STREAM),
        RuntimeFnId::FileExists => ("$__rt_file_exists", &[IrType::Str], IrType::I64),
        RuntimeFnId::Unlink => ("$__rt_unlink", &[IrType::Str], IrType::I64),
        RuntimeFnId::FileGetContents => ("$__rt_file_get_contents", &[IrType::Str], STREAM),
        RuntimeFnId::FilePutContents => (
            "$__rt_file_put_contents",
            &[IrType::Str, IrType::Str],
            IrType::I64,
        ),
        _ => return None,
    };
    Some(FileBuiltin {
        helper,
        operands,
        result,
        // `fopen` is the only one whose helper is also called internally, by
        // `file_get_contents` and `file_put_contents`, which name themselves in their own
        // diagnostics. The flag tells it whose message to emit.
        warns: matches!(target, RuntimeFnId::Fopen),
        optional_tail: match target {
            // PHP's SEEK_SET.
            RuntimeFnId::Fseek => &[0][..],
            // `length` is declared `Int = Null` and `offset` `Int = -1`; the helper reads
            // "everything remaining" for a negative length and "do not seek" for a negative
            // offset, so -1 carries both defaults.
            RuntimeFnId::StreamGetContents => &[-1, -1][..],
            // Same two defaults, after the two stream operands.
            RuntimeFnId::StreamCopyToStream => &[-1, -1][..],
            _ => &[][..],
        },
    })
}

/// Validates one file-builtin call against the exact storage its helper reads and answers.
///
/// The helpers take raw representations — a string is a pointer and a length, a stream handle is
/// a boxed cell, a count is an i64 — so a call site holding anything else would have its bytes
/// reinterpreted rather than converted. Every position is therefore pinned, and the module must
/// carry the command runtime, which is where the WASI imports these call live.
pub(super) fn file_builtin_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    file: &FileBuiltin,
) -> Option<String> {
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some("the file runtime is part of the command runtime".to_string());
    }
    // PHP dispatches a path carrying a `scheme://` prefix to a STREAM WRAPPER — `ftp://` opens a
    // network connection, `phar://` reads inside an archive, `php://memory` is not a file at all
    // — and this runtime resolves paths against a preopened directory and nothing else. Treating
    // a wrapper URL as a filename answers `false` where PHP answers a handle, so a literal one is
    // refused here rather than compiled into a wrong answer. A computed path cannot be inspected;
    // there the runtime's `false` is what PHP itself answers for a wrapper it has not registered.
    if let Some(path) = call
        .operands
        .first()
        .and_then(|id| sprintf_literal_format(function, module, *id))
    {
        // Two parts of `php://` are exceptions. `stdout`, `stderr`, `stdin` and `output` name
        // fds the process already holds, which the runtime answers without touching the
        // filesystem. And `memory`/`temp` are not files either — they are an in-memory buffer
        // this runtime implements outright, so they need no preopen and cannot fail.
        //
        // `php://temp/maxmemory:N` is accepted under the same arm: that suffix only chooses when
        // php-src spills the buffer to a real file, which this implementation never does.
        let names_a_standard_stream = matches!(
            path.as_slice(),
            b"php://stdout" | b"php://stderr" | b"php://stdin" | b"php://output"
        ) || path == b"php://memory"
            || path.starts_with(b"php://temp")
            // An RFC 2397 `data:` URI is not a wrapper over anything: it CARRIES its payload,
            // so `__rt_fopen` decodes it inline and never touches the filesystem.
            || path.starts_with(b"data:");
        if let Some(scheme_end) = path.windows(3).position(|window| window == b"://") {
            if scheme_end > 0
                && !names_a_standard_stream
                && path[..scheme_end].iter().all(|byte| byte.is_ascii_alphanumeric())
            {
                return Some(format!(
                    "path \"{}://\" names a stream wrapper, not a file",
                    String::from_utf8_lossy(&path[..scheme_end])
                ));
            }
        }
    }
    let lowest_arity = file.operands.len() - file.optional_tail.len();
    if call.operands.len() > file.operands.len() || call.operands.len() < lowest_arity {
        return Some(format!(
            "expected {} operands, got {}",
            file.operands.len(),
            call.operands.len()
        ));
    }
    for (index, expected) in file.operands.iter().enumerate() {
        // The optional trailing operand is supplied by the lowering when the call omits it.
        if index >= call.operands.len() {
            break;
        }
        let Some(value) = call.operands.get(index).and_then(|id| function.value(*id)) else {
            return Some(format!("operand #{index} is missing from the value table"));
        };
        // A stream reaches these two ways. `fopen` answers a boxed cell carrying the fd as a
        // resource payload, but the predefined `STDOUT`/`STDERR`/`STDIN` are raw integers the
        // EIR already types `resource<stream>`. Both name an fd; the lowering resolves which.
        // Matched on the DECLARED type, not `codegen_repr()`, which folds every resource to
        // `Int` — the whole point here is to tell a stream from an ordinary integer.
        let stream_constant = *expected == IrType::Heap(IrHeapKind::Mixed)
            && value.ir_type == IrType::I64
            && matches!(value.php_type, PhpType::Resource(_));
        if value.ir_type != *expected && !stream_constant {
            return Some(format!(
                "operand #{index} storage {:?} is not the expected {expected:?}",
                value.ir_type
            ));
        }
    }
    // `stream_get_meta_data` always answers a hash; a site the checker NARROWED to raw
    // hash storage is served by the lowering's unbox adaptation, so both storages pass.
    // Keyed on the helper symbol because this validator does not carry the target id.
    let narrowed_meta_hash = file.helper == "$__rt_stream_get_meta_data"
        && call.result_type == IrType::Heap(IrHeapKind::Hash);
    if call.result_type != file.result && !narrowed_meta_hash {
        return Some(format!(
            "result storage {:?} is not the expected {:?}",
            call.result_type, file.result
        ));
    }
    // php answers `int|false` here, but the shared builtin contract types the result `int`, so
    // this call site has no cell to hold the `false` a failed write answers: it would read as 0.
    // A site that never looks at the result cannot tell the difference and keeps compiling, and
    // so does one that only PRINTS it (`echo "n:", file_put_contents(...)`, the documented idiom:
    // a failed write prints `0` where php prints nothing). One that tests it (`=== false`, a
    // branch, an assignment that is compared later) would silently take the wrong arm, so it is
    // refused by name.
    if file.helper == "$__rt_file_put_contents" {
        if let Some(result) = call.result {
            if function_uses_value_beyond_printing(function, result) {
                return Some(
                    "file_put_contents() answers int|false in php, but this target types the result \
                     int, so a failed write would read as 0; use it only as a statement or print it"
                        .to_string(),
                );
            }
        }
    }
    None
}

/// Lowers one file builtin: load every operand in order, call the helper, store the result.
pub(super) fn lower_file_builtin(
    ctx: &mut FnCtx,
    inst: &Instruction,
    file: FileBuiltin,
    target: RuntimeFnId,
) -> Result<()> {
    // `fclose` must leave a mark later queries can see: php-src answers `Unknown` from
    // `get_resource_type` once a handle is closed. The box is stamped HERE rather than inside
    // the helper because the loop below resolves a stream operand to its raw fd, so the helper
    // never sees the box — and a raw predefined fd has no box to stamp at all.
    let closed_box = (target == RuntimeFnId::Fclose)
        .then(|| operand(inst, 0).ok())
        .flatten()
        .filter(|value| {
            ctx.function
                .value(*value)
                .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed))
        });
    // Closing a handle that is already closed is php's `TypeError`, checked here, on the handle
    // as the script passed it, before the helper can free a descriptor a second time. The box's
    // -1 stamp (below) is not enough on its own: a copy of the handle — re-boxed from an array
    // that stores it raw, or a raw resource operand — never saw it, so `__rt_handle_closed` also
    // asks the stream itself.
    let raw_handle = (target == RuntimeFnId::Fclose)
        .then(|| operand(inst, 0).ok())
        .flatten()
        .filter(|value| {
            ctx.function
                .value(*value)
                .is_some_and(|value| value.ir_type == IrType::I64)
        });
    if closed_box.is_some() || raw_handle.is_some() {
        if let Some(handle) = closed_box {
            ctx.emit_load_value(handle)?;
            ctx.fb.ins("i32.const 8", "the resource payload word");
            ctx.fb.ins("i32.add", "");
            ctx.fb.ins("i64.load", "the handle's payload");
        } else if let Some(handle) = raw_handle {
            ctx.emit_load_value(handle)?;
        }
        ctx.fb
            .ins("call $__rt_handle_closed", "already closed, through this copy or another?");
        ctx.fb.ins("if", "fclose() of a closed stream raises a TypeError");
        crate::codegen_wasm::inst::emit_runtime_failure(
            ctx,
            crate::codegen_wasm::streamtune::FCLOSE_CLOSED_FAILURE_CODE,
            "fclose() on a stream that is already closed",
        );
        ctx.fb.ins("end", "end already-closed guard");
    }
    for (index, expected) in file.operands.iter().enumerate() {
        if index >= inst.operands.len() {
            // The call omitted trailing defaulted arguments, so the registry's defaults are
            // pushed here, in order. Any other shortfall was already refused by the audit.
            let first_default = file.operands.len() - file.optional_tail.len();
            for default in &file.optional_tail[index - first_default..] {
                ctx.fb
                    .ins(&format!("i64.const {default}"), "defaulted trailing argument");
            }
            break;
        }
        let value = operand(inst, index)?;
        ctx.emit_load_value(value)?;
        // The stream-taking helpers work on a raw fd, so a handle is resolved here rather than
        // inside each of them: a boxed cell yields its resource payload, and a predefined
        // `STDOUT`/`STDERR`/`STDIN` already IS that fd, one word too wide.
        if *expected == IrType::Heap(IrHeapKind::Mixed) {
            let boxed = ctx
                .function
                .value(value)
                .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed));
            if boxed {
                ctx.fb
                    .ins("call $__rt_stream_fd", "the fd inside a stream handle");
            } else {
                ctx.fb
                    .ins("call $__rt_handle_fd", "a predefined stream is already its fd");
            }
        }
    }
    if file.warns {
        ctx.fb
            .ins("i32.const 1", "a PHP-level open warns when it fails");
    }
    ctx.fb
        .ins(&format!("call {}", file.helper), "WASI-backed file builtin");
    if let Some(handle) = closed_box.or(raw_handle) {
        // The stream's per-open record (or a predefined std constant's mark) is what every copy
        // of the handle reads, so it is closed from the payload as the script passed it, before
        // the box below is stamped. The host fd was closed by the helper, exactly once.
        ctx.emit_load_value(handle)?;
        if closed_box.is_some() {
            ctx.fb.ins("i32.const 8", "the resource payload word");
            ctx.fb.ins("i32.add", "");
            ctx.fb.ins("i64.load", "the handle's payload");
        }
        ctx.fb
            .ins("call $__rt_stream_record_close", "every copy of this handle now reads closed");
    }
    if let Some(handle) = closed_box {
        // A negative payload is the closed predicate, the same shape the native backend uses.
        // The helper's result stays underneath on the stack; this only adds and removes its own.
        ctx.emit_load_value(handle)?;
        ctx.fb.ins("i32.const 8", "the resource payload word");
        ctx.fb.ins("i32.add", "");
        ctx.fb.ins("i64.const -1", "closed sentinel");
        ctx.fb.ins("i64.store", "mark the handle closed for get_resource_type");
    }
    // A site the checker narrowed to RAW hash storage takes the hash out of the fresh
    // cell: both pointers are i32, so a plain store would VALIDATE while storing the
    // cell as the hash and corrupting every later read — the unbox is not optional.
    if target == RuntimeFnId::StreamGetMetaData
        && inst.result_type == IrType::Heap(IrHeapKind::Hash)
    {
        let cell = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        let hash = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
        ctx.fb
            .ins(&format!("local.set {cell}"), "the fresh metadata cell");
        ctx.fb.ins(&format!("local.get {cell}"), "the metadata cell");
        ctx.fb.ins("i64.load offset=8", "the hash pointer is the cell's lo word");
        ctx.fb.ins("i32.wrap_i64", "narrow to the hash pointer");
        ctx.fb.ins(&format!("local.tee {hash}"), "the hash itself");
        ctx.fb.ins("call $__rt_incref", "the result owns the hash");
        ctx.fb
            .ins(&format!("local.get {cell}"), "the now-redundant cell");
        ctx.fb.ins("call $__rt_decref_any", "release it (frees its hash ref)");
        ctx.fb.ins(&format!("local.get {hash}"), "the raw hash result");
    }
    store_result(ctx, inst)
}
