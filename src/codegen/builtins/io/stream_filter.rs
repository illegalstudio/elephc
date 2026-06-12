//! Purpose:
//! Emits PHP `stream_filter_append`, `stream_filter_prepend` and
//! `stream_filter_remove` calls.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - v1 attaches one built-in filter per stream per direction. The filter name
//!   is a compile-time literal mapped to a small id (1 = `string.toupper`,
//!   2 = `string.tolower`, 3 = `string.rot13`) and stored in the per-fd
//!   `_stream_read_filters` / `_stream_write_filters` tables.
//! - `append` and `prepend` are equivalent in this single-filter model; both
//!   return the stream re-boxed as a resource. `remove` clears both tables.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Maps a built-in stream filter name to its runtime filter id.
///
/// Ids 1..3 are simple byte-by-byte transforms implemented inside
/// `__rt_apply_stream_filter`. Ids 4..9 are richer state-machine /
/// ratio-changing transforms, each dispatched to its own explicit
/// `__rt_asf_*` case in `runtime/io/stream_filter.rs`: 4 strip_tags,
/// 5 dechunk, 6 base64-encode, 7 base64-decode, 8 quoted-printable-encode,
/// 9 quoted-printable-decode. All are real transforms (covered by the
/// `test_stream_filter_{base64,qp,strip_tags,dechunk}*` tests) and the
/// names also round-trip through `stream_get_filters`/`stream_filter_append`.
pub(super) fn filter_id(name: &str) -> Option<i64> {
    match name {
        "string.toupper" => Some(1),
        "string.tolower" => Some(2),
        "string.rot13" => Some(3),
        // Real state-machine / ratio-changing transforms (not stubs):
        // dechunk parses HTTP/1.1 chunked transfer encoding;
        // base64/quoted-printable change the 3:4 or 4:3 byte ratio;
        // strip_tags is a tag-aware state machine. Each has an explicit
        // `__rt_asf_*` case in runtime/io/stream_filter.rs.
        "string.strip_tags" => Some(4),
        "dechunk" => Some(5),
        "convert.base64-encode" => Some(6),
        "convert.base64-decode" => Some(7),
        "convert.quoted-printable-encode" => Some(8),
        "convert.quoted-printable-decode" => Some(9),
        _ => None,
    }
}

/// Extracts a compile-time-constant integer from the 4th
/// `stream_filter_append`/`prepend` argument (`$params`), honoring both of
/// PHP's literal forms:
///
/// - a bare int literal — `stream_filter_append($fp, 'zlib.deflate', $rw, 6)` —
///   which PHP treats as the single primary value (zlib level / bzip2 blocks);
///   returned only when `key` is the primary key (`"level"` / `"blocks"`), and
/// - the canonical array form — `['level' => 6]` / `['blocks' => 1, 'work' => 30]`
///   — from which the entry under `key` is read.
///
/// Returns `None` for a missing arg, a non-constant expression, or an array
/// with no static int under `key`, in which case the filter keeps its default.
/// Clamps the value into `[min, max]` so an out-of-range literal cannot reach
/// the C library. `primary` marks the key a bare scalar maps to (only the
/// primary key consumes a bare int; secondary keys like `"work"` come only from
/// the array form).
pub(super) fn const_int_param(
    args: &[Expr],
    key: &str,
    primary: bool,
    min: i64,
    max: i64,
) -> Option<i64> {
    match args.get(3).map(|a| &a.kind) {
        Some(ExprKind::IntLiteral(v)) if primary => Some((*v).clamp(min, max)),
        Some(ExprKind::ArrayLiteralAssoc(items)) => items.iter().find_map(|(k, v)| match (&k.kind, &v.kind) {
            (ExprKind::StringLiteral(name), ExprKind::IntLiteral(n)) if name == key => {
                Some((*n).clamp(min, max))
            }
            _ => None,
        }),
        _ => None,
    }
}

/// Emits `stream_filter_append` / `stream_filter_prepend`. In the single-filter
/// model the two are equivalent.
pub fn emit_attach(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    // The zlib.* filters call into libz, so they have dedicated emitters that
    // keep the libz dependency out of programs not using them.
    if let ExprKind::StringLiteral(name) = &args[1].kind {
        if name == "zlib.deflate" {
            return crate::codegen::builtins::io::stream_filter_zlib::emit_zlib_deflate_attach(
                args, emitter, ctx, data,
            );
        }
        if name == "zlib.inflate" {
            return crate::codegen::builtins::io::stream_filter_inflate::emit_zlib_inflate_attach(
                args, emitter, ctx, data,
            );
        }
        if let Some(spec) = name.strip_prefix("convert.iconv.") {
            // convert.iconv.<from>/<to> transcodes via libc iconv at attach time
            // (slurp + convert + dup2), like the zlib.inflate read filter.
            return crate::codegen::builtins::io::stream_filter_iconv::emit(
                spec, args, emitter, ctx, data,
            );
        }
        if name == "bzip2.compress" {
            // bzip2.compress streams writes through libbz2 (BZ2_bzCompress),
            // mirroring the zlib.deflate write filter.
            return crate::codegen::builtins::io::stream_filter_bzip2::emit_bzip2_compress_attach(
                args, emitter, ctx, data,
            );
        }
        if name == "bzip2.decompress" {
            // bzip2.decompress slurps + one-shot decompresses the stream at
            // attach time (reusing the compress.bzip2:// read core), like
            // zlib.inflate.
            return crate::codegen::builtins::io::stream_filter_bzip2::emit_bzip2_decompress_attach(
                args, emitter, ctx, data,
            );
        }
    }
    // Names that are not built-in filters route into the user-filter runtime
    // path: stream_filter_attach_user resolves the name through the registry,
    // instantiates the class, and stores per-(fd, dir) state.
    let built_in_id = match &args[1].kind {
        ExprKind::StringLiteral(name) => filter_id(name),
        _ => None,
    };
    if built_in_id.is_none() {
        return emit_attach_user(args, emitter, ctx, data);
    }
    emitter.comment("stream_filter_append()");
    let id = built_in_id.unwrap_or(0);
    emit_stream_fd_arg("stream_filter_append", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the descriptor
    if args.len() >= 3 {
        emit_expr(&args[2], emitter, ctx, data);
    } else {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("mov x0, #3"),                 // default mode STREAM_FILTER_ALL
            Arch::X86_64 => emitter.instruction("mov eax, 3"),                  // default mode STREAM_FILTER_ALL
        }
    }
    let skip_read = ctx.next_label("sf_skip_read");
    let skip_write = ctx.next_label("sf_skip_write");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x1"); // descriptor into x1, mode in x0
            emitter.instruction("tst x0, #1");                                  // STREAM_FILTER_READ bit set?
            emitter.instruction(&format!("b.eq {}", skip_read));                // skip the read-filter table otherwise
            abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
            emitter.instruction(&format!("mov w10, #{}", id));                  // built-in filter id
            emitter.instruction("strb w10, [x9, x1]");                          // record the read filter for this descriptor
            emitter.label(&skip_read);
            emitter.instruction("tst x0, #2");                                  // STREAM_FILTER_WRITE bit set?
            emitter.instruction(&format!("b.eq {}", skip_write));               // skip the write-filter table otherwise
            abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
            emitter.instruction(&format!("mov w10, #{}", id));                  // built-in filter id
            emitter.instruction("strb w10, [x9, x1]");                          // record the write filter for this descriptor
            emitter.label(&skip_write);
            emitter.instruction("mov x2, #0");                                  // resource mixed payloads have no high word
            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // re-box the stream as the filter resource
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "rcx"); // descriptor into rcx, mode in rax
            emitter.instruction("test rax, 1");                                 // STREAM_FILTER_READ bit set?
            emitter.instruction(&format!("jz {}", skip_read));                  // skip the read-filter table otherwise
            abi::emit_symbol_address(emitter, "r9", "_stream_read_filters");    // read-filter table base
            emitter.instruction(&format!("mov BYTE PTR [r9 + rcx], {}", id));   // record the read filter for this descriptor
            emitter.label(&skip_read);
            emitter.instruction("test rax, 2");                                 // STREAM_FILTER_WRITE bit set?
            emitter.instruction(&format!("jz {}", skip_write));                 // skip the write-filter table otherwise
            abi::emit_symbol_address(emitter, "r9", "_stream_write_filters");   // write-filter table base
            emitter.instruction(&format!("mov BYTE PTR [r9 + rcx], {}", id));   // record the write filter for this descriptor
            emitter.label(&skip_write);
            emitter.instruction("mov rdi, rcx");                                // resource payload = the descriptor
            emitter.instruction("xor esi, esi");                                // resource mixed payloads have no high word
            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // re-box the stream as the filter resource
        }
    }
    Some(PhpType::Mixed)
}

/// Emits the user-filter attach path: resolves the filter name through
/// the runtime registry, instantiates the wrapper class via
/// `__rt_new_by_name`, and records per-(fd, dir) state. Returns the
/// stream re-boxed as a filter resource on success, PHP `false`
/// (boxed bool) on miss (unknown filter / class).
fn emit_attach_user(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_filter_append() — user filter");
    emit_stream_fd_arg("stream_filter_append", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the descriptor across the name + mode expressions
    emit_expr(&args[1], emitter, ctx, data);                                    // filter-name string in elephc string-result regs
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2");                       // preserve filter-name ptr/len across the mode expression
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
            } else {
                emitter.instruction("mov x0, #3");                              // default mode STREAM_FILTER_ALL
            }
            abi::emit_push_reg(emitter, "x0");                                  // preserve mode while materializing legacy null params
            emit_legacy_user_filter_null_params(emitter);
            emitter.instruction("mov x4, x0");                                  // pass null params to the shared attach helper
            abi::emit_pop_reg(emitter, "x3");                                   // move mode into the helper's 4th arg
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                        // restore filter-name ptr/len → helper's 2nd/3rd args
            // Peek the descriptor: it remains on the stack so we can box
            // it as the filter resource after the helper returns.
            emitter.instruction("ldr x0, [sp]");                                // descriptor → helper's 1st arg (no pop yet)
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve filter-name ptr/len across the mode expression
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
            } else {
                emitter.instruction("mov eax, 3");                              // default mode STREAM_FILTER_ALL
            }
            abi::emit_push_reg(emitter, "rax");                                 // preserve mode while materializing legacy null params
            emit_legacy_user_filter_null_params(emitter);
            emitter.instruction("mov r8, rax");                                 // pass null params to the shared attach helper
            abi::emit_pop_reg(emitter, "rcx");                                  // move mode into the helper's 4th arg
            abi::emit_pop_reg_pair(emitter, "rsi", "rdx");                      // restore filter-name ptr/len → helper's 2nd/3rd args
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // peek the descriptor → helper's 1st arg
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_filter_attach_user");            // returns bool: 1 = registered+attached, 0 = unknown/instantiation-fail
    let fail_label = ctx.next_label("sfau_false");
    let done_label = ctx.next_label("sfau_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {}", fail_label));            // PHP false on unknown filter
            emitter.instruction("ldr x1, [sp]");                                // peek descriptor for the filter-resource payload
            abi::emit_release_temporary_stack(emitter, 16);                     // now drop the saved descriptor
            emitter.instruction("mov x2, #0");                                  // resource mixed payloads have no high word
            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // re-box the stream as the filter resource (value_lo = fd)
            emitter.instruction(&format!("b {}", done_label));                  // continue at target label
            emitter.label(&fail_label);
            abi::emit_release_temporary_stack(emitter, 16);                     // drop the saved descriptor on the failure path too
            emitter.instruction("mov x1, #0");                                  // bool payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box PHP false for callers that test `!== false`
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", fail_label));                 // PHP false on unknown filter
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // peek descriptor for resource payload
            abi::emit_release_temporary_stack(emitter, 16);                     // drop the saved descriptor
            emitter.instruction("xor esi, esi");                                // resource mixed payloads have no high word
            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // re-box the stream as the filter resource (value_lo = fd)
            emitter.instruction(&format!("jmp {}", done_label));                // continue at target label
            emitter.label(&fail_label);
            abi::emit_release_temporary_stack(emitter, 16);                     // drop the saved descriptor
            emitter.instruction("xor edi, edi");                                // bool payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box PHP false
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Mixed)
}

/// Materializes a boxed null params value for the frozen AST backend's shared
/// user-filter attach helper call.
fn emit_legacy_user_filter_null_params(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // null params have no payload
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // null params have no payload
        }
    }
    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Void);
}

/// Emits `stream_filter_remove`: clears the read and write filters of the
/// stream the filter resource refers to.
pub fn emit_remove(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_filter_remove()");
    emit_stream_fd_arg("stream_filter_remove", &args[0], emitter, ctx, data);
    // Run onClose() on any attached user-filter instance + clear the
    // _user_filter_instances slots for both directions. The helper
    // takes the fd in x0 / rdi and preserves it in the standard
    // int-result reg on return so the byte-table clear below still
    // sees the fd.
    if matches!(emitter.target.arch, Arch::X86_64) {
        emitter.instruction("mov rdi, rax");                                    // fd → SysV first arg
    }
    abi::emit_call_label(emitter, "__rt_user_filter_release_fd");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
            emitter.instruction("strb wzr, [x9, x0]");                          // clear the read filter for this descriptor
            abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
            emitter.instruction("strb wzr, [x9, x0]");                          // clear the write filter for this descriptor
            emitter.instruction("mov x0, #1");                                  // stream_filter_remove() returns true
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r9", "_stream_read_filters");    // read-filter table base
            emitter.instruction("mov BYTE PTR [r9 + rax], 0");                  // clear the read filter for this descriptor
            abi::emit_symbol_address(emitter, "r9", "_stream_write_filters");   // write-filter table base
            emitter.instruction("mov BYTE PTR [r9 + rax], 0");                  // clear the write filter for this descriptor
            emitter.instruction("mov eax, 1");                                  // stream_filter_remove() returns true
        }
    }
    Some(PhpType::Bool)
}
