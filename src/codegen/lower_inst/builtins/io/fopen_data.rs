//! Purpose:
//! PHP filter stamps and compile-time data URI streams.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;
use elephc_memoir::buffers::stream_tables;

/// Records `php://filter` read/write filter ids on a successfully opened resource.
pub(super) fn emit_php_filter_table_stamps(ctx: &mut FunctionContext<'_>, mode_bits: u8, filter_id: u8) {
    let done_label = ctx.next_label("php_filter_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the boxed fopen result tag
            ctx.emitter.instruction("cmp x9, #9");                              // test whether fopen returned a resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // leave false results unmodified
            ctx.emitter.instruction("ldr x1, [x0, #8]");                        // load the descriptor payload from the boxed resource
            if mode_bits & 1 != 0 {
                stream_tables::READ_FILTERS.emit_address(ctx.emitter, "x9");
                ctx.emitter.instruction(&format!("mov w10, #{}", filter_id));   // materialize the php://filter read filter id
                ctx.emitter.instruction("strb w10, [x9, x1]");                  // attach the read filter to this descriptor
            }
            if mode_bits & 2 != 0 {
                stream_tables::WRITE_FILTERS.emit_address(ctx.emitter, "x9");
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
                stream_tables::READ_FILTERS.emit_address(ctx.emitter, "r8");
                ctx.emitter.instruction(
                    &format!("mov BYTE PTR [r8 + rcx], {}", filter_id)
                );                                                              // attach the read filter to this descriptor
            }
            if mode_bits & 2 != 0 {
                stream_tables::WRITE_FILTERS.emit_address(ctx.emitter, "r8");
                ctx.emitter.instruction(
                    &format!("mov BYTE PTR [r8 + rcx], {}", filter_id)
                );                                                              // attach the write filter to this descriptor
            }
            ctx.emitter.label(&done_label);
        }
    }
}

/// Lowers a literal `fopen("data://...", ...)` through an in-memory data stream.
pub(super) fn lower_literal_data_fopen(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    path: &str,
) -> Result<()> {
    emit_literal_data_fopen_result(ctx, path)?;
    store_if_result(ctx, inst)
}

/// Emits the boxed result for a literal `data://` stream open.
pub(super) fn emit_literal_data_fopen_result(ctx: &mut FunctionContext<'_>, path: &str) -> Result<()> {
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
pub(super) fn decode_data_uri_for_fopen(path: &str) -> Option<Vec<u8>> {
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

