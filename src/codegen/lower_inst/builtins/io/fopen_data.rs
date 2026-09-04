//! Purpose:
//! PHP filter stamps and compile-time data URI streams.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Attaches the `php://filter` filters to a successfully opened resource.
///
/// The boxed `fopen` result stays in the int result register; the filter nodes
/// are linked onto the stream's chains.
///
/// This used to stamp the legacy per-descriptor slots using the boxed payload as
/// an index. Since the registry migration that payload is an opaque handle, not
/// a descriptor, so indexing a 512-byte table with it wrote far out of bounds and
/// crashed every `php://filter` open.
pub(super) fn emit_php_filter_table_stamps(
    ctx: &mut FunctionContext<'_>,
    filter_ids: &[(u8, u8)],
) {
    // `php://filter/a|b/resource=x` runs a THROUGH b. Each name gets its own node appended
    // at the chain tail, so applying only the first — which is what this used to do — both
    // dropped a transform and left the output looking plausible.
    //
    // The direction travels WITH each filter rather than once for the URL: a spec may name both
    // (`read=a/write=b`), and only the open's own direction is meant to apply.
    for &(mode_bits, filter_id) in filter_ids {
        emit_one_php_filter_stamp(ctx, mode_bits, filter_id);
    }
}

/// Links one built-in filter onto the freshly opened stream's chain.
fn emit_one_php_filter_stamp(ctx: &mut FunctionContext<'_>, mode_bits: u8, filter_id: u8) {
    let done_label = ctx.next_label("php_filter_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the boxed fopen result tag
            ctx.emitter.instruction("cmp x9, #9");                              // test whether fopen returned a resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // leave false results unmodified
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the boxed fopen result
            ctx.emitter.instruction("ldr x9, [x0, #8]");                        // opaque stream handle from the Mixed payload
            ctx.emitter.instruction("str x9, [sp, #8]");                        // preserve the stream handle
            ctx.emitter.instruction(&format!("mov x0, #{filter_id}"));          // built-in filter id
            ctx.emitter.instruction("mov x1, #0");                              // built-ins carry no user-filter object
            ctx.emitter.instruction(&format!("mov x2, #{mode_bits}"));          // direction bits from the URL
            ctx.emitter.instruction("mov x3, #0");                              // built-ins retain no params value
            abi::emit_call_label(ctx.emitter, "__rt_filter_create");
            ctx.emitter.instruction("str x0, [sp, #16]");                       // preserve the filter handle
            if mode_bits & 1 != 0 {
                ctx.emitter.instruction("ldr x0, [sp, #8]");                    // stream handle
                ctx.emitter.instruction("ldr x1, [sp, #16]");                   // filter handle
                ctx.emitter.instruction(&format!("mov x2, #{STREAM_READ_FILTER_HEAD_OFFSET}"));
                ctx.emitter.instruction("mov x3, #0");                          // append at the chain tail
                abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            }
            if mode_bits & 2 != 0 {
                ctx.emitter.instruction("ldr x0, [sp, #8]");                    // stream handle
                ctx.emitter.instruction("ldr x1, [sp, #16]");                   // filter handle
                ctx.emitter.instruction(&format!("mov x2, #{STREAM_WRITE_FILTER_HEAD_OFFSET}"));
                ctx.emitter.instruction("mov x3, #0");                          // append at the chain tail
                abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            }
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // restore the boxed fopen result
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // load the boxed fopen result tag
            ctx.emitter.instruction("cmp r9, 9");                               // test whether fopen returned a resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // leave false results unmodified
            abi::emit_reserve_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the boxed fopen result
            ctx.emitter.instruction("mov r9, QWORD PTR [rax + 8]");             // opaque stream handle from the Mixed payload
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r9");             // preserve the stream handle
            ctx.emitter.instruction(&format!("mov rdi, {filter_id}"));          // built-in filter id
            ctx.emitter.instruction("xor esi, esi");                            // built-ins carry no user-filter object
            ctx.emitter.instruction(&format!("mov rdx, {mode_bits}"));          // direction bits from the URL
            ctx.emitter.instruction("xor ecx, ecx");                            // built-ins retain no params value
            abi::emit_call_label(ctx.emitter, "__rt_filter_create");
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // preserve the filter handle
            if mode_bits & 1 != 0 {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");        // stream handle
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");       // filter handle
                ctx.emitter.instruction(&format!("mov rdx, {STREAM_READ_FILTER_HEAD_OFFSET}"));
                ctx.emitter.instruction("xor ecx, ecx");                        // append at the chain tail
                abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            }
            if mode_bits & 2 != 0 {
                ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");        // stream handle
                ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");       // filter handle
                ctx.emitter.instruction(&format!("mov rdx, {STREAM_WRITE_FILTER_HEAD_OFFSET}"));
                ctx.emitter.instruction("xor ecx, ecx");                        // append at the chain tail
                abi::emit_call_label(ctx.emitter, "__rt_stream_filter_link");
            }
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // restore the boxed fopen result
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.label(&done_label);
        }
    }
}

/// Emits the boxed result for a literal `data://` stream open.
pub(super) fn emit_literal_data_fopen_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
    // Whatever php refuses this URI with is already known here, so the whole line is one interned
    // string and one call — the run-time opener composes the identical text from the URI's bytes.
    if let Some(DataUriOutcome::Refused(reason)) = classify_data_uri_for_fopen(path) {
        emit_static_diag_warning(
            ctx,
            &format!("Warning: fopen({path}): Failed to open stream: {reason}\n"),
        );
    }
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
    emit_record_stream_meta_after_boxed_literal(ctx, 7, path);
    Ok(())
}

/// What php-src makes of a literal `data:` URI: its bytes, or the sentence it refuses with.
pub(super) enum DataUriOutcome {
    /// The payload, decoded exactly as php would decode it.
    Decoded(Vec<u8>),
    /// php refuses the URI and words the failure like this, with no errno behind it.
    Refused(&'static str),
}

/// Classifies a literal `data:[//][mediatype][;base64],payload` URL for EIR `fopen`.
///
/// The `//` is OPTIONAL. php-src's `php_stream_locate_url_wrapper` special-cases this one scheme
/// (`n == 4 && !memcmp("data:", path, 5)`), so `data:text/plain,hi` opens exactly like
/// `data://text/plain,hi` — measured, both answer `'hi'`.
///
/// Every refusal carries php's own wording; the four are distinct and were measured one by one,
/// because which one applies is not guessable from the URI's shape: `data://text/plain;,hi` and
/// `data://text/plain;BASE64,SGk=` are `illegal parameter`, not `illegal media type`, since the
/// TYPE is only the first `;`-segment.
pub(super) fn classify_data_uri_for_fopen(path: &str) -> Option<DataUriOutcome> {
    let rest = path.strip_prefix("data:")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let Some(comma) = rest.find(',') else {
        return Some(DataUriOutcome::Refused("rfc2397: no comma in URL"));
    };
    let meta = &rest[..comma];
    let payload = &rest[comma + 1..];
    match data_uri_media_type_shape(meta) {
        DataUriMetaShape::IllegalMediaType => {
            Some(DataUriOutcome::Refused("rfc2397: illegal media type"))
        }
        DataUriMetaShape::IllegalParameter => {
            Some(DataUriOutcome::Refused("rfc2397: illegal parameter"))
        }
        // `;base64` counts only as the LAST parameter and only in lower case, which is what
        // `data_uri_media_type_shape` has just established.
        DataUriMetaShape::Base64 => match base64_decode_for_data_uri(payload) {
            Some(bytes) => Some(DataUriOutcome::Decoded(bytes)),
            None => Some(DataUriOutcome::Refused("rfc2397: unable to decode")),
        },
        DataUriMetaShape::Plain => {
            Some(DataUriOutcome::Decoded(percent_decode_for_data_uri(payload)))
        }
    }
}

/// Decodes a literal `data:` URL, or answers `None` for anything php refuses.
///
/// Kept for callers that only need the bytes and have no line to print.
pub(super) fn decode_data_uri_for_fopen(path: &str) -> Option<Vec<u8>> {
    match classify_data_uri_for_fopen(path)? {
        DataUriOutcome::Decoded(bytes) => Some(bytes),
        DataUriOutcome::Refused(_) => None,
    }
}

/// How php-src reads a `data:` media type, refusals kept apart because their wordings differ.
pub(super) enum DataUriMetaShape {
    /// Accepted; the payload is percent-encoded.
    Plain,
    /// Accepted; the final parameter was `base64`.
    Base64,
    /// The TYPE segment is neither empty nor carries a `/`.
    IllegalMediaType,
    /// A later segment is neither `name=value` nor a final `base64`.
    IllegalParameter,
}

/// Reports whether php-src would accept this `data://` media type.
///
/// Measured against php 8.5.6, which is stricter than "anything before the comma": the type is
/// either empty or must carry a `/`, every parameter must be `name=value`, and `base64` is
/// accepted only as the final parameter and only in lower case. So `text`, `text/plain;` and
/// `text/plain;base64;charset=utf-8` are all refused, while `text/plain;bogus=1` is accepted —
/// php-src validates the SHAPE, it does not police parameter names.
///
/// The same rule lives in `__rt_data_stream_dynamic` for a URI built at run time; one runs at
/// compile time and the other on the bytes, so neither can serve both. They are pinned together.
pub(super) fn data_uri_media_type_shape(meta: &str) -> DataUriMetaShape {
    if meta.is_empty() {
        return DataUriMetaShape::Plain;
    }
    let mut segments = meta.split(';');
    let first = segments.next().unwrap_or("");
    if !first.is_empty() && !first.contains('/') {
        return DataUriMetaShape::IllegalMediaType;
    }
    let mut rest = segments.peekable();
    while let Some(segment) = rest.next() {
        if segment == "base64" && rest.peek().is_none() {
            return DataUriMetaShape::Base64;
        }
        if !segment.contains('=') {
            return DataUriMetaShape::IllegalParameter;
        }
    }
    DataUriMetaShape::Plain
}

/// Decodes a base64 payload for a compile-time `data://` stream.
pub(super) fn base64_decode_for_data_uri(input: &str) -> Option<Vec<u8>> {
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
pub(super) fn base64_sextet_for_data_uri(c: u8) -> Option<u32> {
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
pub(super) fn percent_decode_for_data_uri(input: &str) -> Vec<u8> {
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

