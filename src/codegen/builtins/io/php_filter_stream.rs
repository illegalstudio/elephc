//! Purpose:
//! Lowers `fopen()` calls whose path is a `php://filter/...` URL.
//! Opens the underlying `resource=` stream (reusing all of `fopen`'s scheme
//! handling) and then attaches a built-in filter to its descriptor.
//!
//! Called from:
//! - `crate::codegen::builtins::io::fopen::emit()` when the path literal
//!   begins with `php://filter/`.
//!
//! Key details:
//! - URL form: `php://filter/[read=|write=]<filter>/resource=<path>`. A bare
//!   filter (no `read=`/`write=`) applies to both directions (`STREAM_FILTER_ALL`).
//! - The filter name is mapped at compile time through [`super::stream_filter::filter_id`]
//!   (`string.toupper`/`tolower`/`rot13`, etc.). The id is written into the per-fd
//!   `_stream_read_filters` / `_stream_write_filters` byte tables, exactly like
//!   `stream_filter_append`, so `__rt_fread`/`__rt_fwrite` apply it.
//! - elephc's filter model is single-filter-per-direction, so a chained
//!   `read=F1|F2` list keeps only the first filter (documented limitation).
//! - Unparseable URL, missing `resource=`, or an unknown filter lower to PHP
//!   `false` (matching PHP's `fopen()` failure for a bad filter spec).
//! - The underlying open is reused via `super::fopen::emit`, which already boxes
//!   the descriptor as a resource Mixed cell; this wrapper only stamps the filter
//!   table on the cell's fd and returns the same cell, so it does not re-box.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits a `fopen("php://filter/...", mode)` call. The path is known to be a
/// string literal beginning with `php://filter/`.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() php://filter stream");
    let spec = match &args[0].kind {
        ExprKind::StringLiteral(p) => p.strip_prefix("php://filter/"),
        _ => None,
    };
    let parsed = spec.and_then(parse_filter_url);
    let (mode_bits, id, resource) = match parsed {
        Some(v) => v,
        None => {
            super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
            return emit_false(emitter, ctx);
        }
    };

    // Open the underlying resource with the caller's mode, reusing fopen's full
    // scheme handling (plain paths, php://temp, data://, http://, ...).
    let resource_expr = Expr {
        kind: ExprKind::StringLiteral(resource),
        span: args[0].span,
    };
    let mut synthetic = vec![resource_expr, args[1].clone()];
    synthetic.extend(args.iter().skip(2).cloned());
    super::fopen::emit(name, &synthetic, emitter, ctx, data);

    // The result is a boxed Mixed cell: tag 9 (resource) on success, tag 3
    // (false) on open failure. Only stamp the filter table when it is a resource.
    let done_label = ctx.next_label("phpf_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // boxed Mixed tag
            emitter.instruction("cmp x9, #9");                                  // runtime tag 9 = resource?
            emitter.instruction(&format!("b.ne {}", done_label));               // open failed (false) → return it unchanged
            emitter.instruction("ldr x1, [x0, #8]");                            // descriptor from the resource payload
            if mode_bits & 1 != 0 {
                abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
                emitter.instruction(&format!("mov w10, #{}", id));              // built-in filter id
                emitter.instruction("strb w10, [x9, x1]");                      // record the read filter for this descriptor
            }
            if mode_bits & 2 != 0 {
                abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
                emitter.instruction(&format!("mov w10, #{}", id));              // built-in filter id
                emitter.instruction("strb w10, [x9, x1]");                      // record the write filter for this descriptor
            }
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rax]");                     // boxed Mixed tag
            emitter.instruction("cmp r9, 9");                                   // runtime tag 9 = resource?
            emitter.instruction(&format!("jne {}", done_label));                // open failed (false) → return it unchanged
            emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                // descriptor from the resource payload
            if mode_bits & 1 != 0 {
                abi::emit_symbol_address(emitter, "r8", "_stream_read_filters"); // read-filter table base
                emitter.instruction(&format!("mov BYTE PTR [r8 + rcx], {}", id)); // record the read filter for this descriptor
            }
            if mode_bits & 2 != 0 {
                abi::emit_symbol_address(emitter, "r8", "_stream_write_filters"); // write-filter table base
                emitter.instruction(&format!("mov BYTE PTR [r8 + rcx], {}", id)); // record the write filter for this descriptor
            }
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Mixed)
}

/// Parses the portion after `php://filter/` into `(mode_bits, filter_id, resource)`.
/// Returns `None` only for a malformed URL (missing `resource=`, empty resource,
/// or a self-referential `resource=php://filter...`). An unrecognized filter name
/// is NOT a hard error: PHP emits a warning but still returns the unfiltered
/// stream, so we report `mode_bits = 0` (no filter table write) to match.
fn parse_filter_url(spec: &str) -> Option<(i64, i64, String)> {
    let (filter_part, resource) = spec.split_once("/resource=")?;
    if resource.is_empty() || resource.starts_with("php://filter") {
        return None;
    }
    let (mode_bits, list) = if let Some(f) = filter_part.strip_prefix("read=") {
        (1i64, f)
    } else if let Some(f) = filter_part.strip_prefix("write=") {
        (2i64, f)
    } else {
        (3i64, filter_part)
    };
    // Single-filter-per-direction model: keep the first filter of a `|` chain.
    let first = list.split('|').next().unwrap_or("");
    match super::stream_filter::filter_id(first) {
        Some(id) => Some((mode_bits, id, resource.to_string())),
        // Unknown filter → open the resource unfiltered (PHP returns the stream).
        None => Some((0, 0, resource.to_string())),
    }
}

/// Emits a boxed PHP `false`, reusing fopen's failure boxing (negative fd).
fn emit_false(emitter: &mut Emitter, ctx: &mut Context) -> Option<PhpType> {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #-1"),                    // negative fd sentinel → boxes PHP false
        Arch::X86_64 => emitter.instruction("mov rax, -1"),                     // negative fd sentinel → boxes PHP false
    }
    super::fopen::box_fopen_result(emitter, ctx);
    Some(PhpType::Mixed)
}
