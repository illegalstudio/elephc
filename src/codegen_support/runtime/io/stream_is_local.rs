//! Purpose:
//! Emits `__rt_stream_is_local_path`, which classifies a path string the way
//! `stream_is_local()` does when its argument is not a stream resource.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `stream_is_local()` lowering, for any argument that is not a compile-time literal.
//!
//! Key details:
//! - php-src answers from the wrapper the path names, and only four remote wrappers plus the
//!   data wrapper are non-local: `http`, `https`, `ftp`, `ftps` and `data`. Everything else —
//!   `php://`, `phar://`, `glob://`, `compress.zlib://`, a bare path, an unknown scheme — is
//!   local, so the scan tests for the non-local set and defaults to local.
//! - Scheme matching is case-insensitive (`HTTP://` is non-local) and requires the full `://`
//!   separator, so `httpx://` and `http:/one-slash` stay local. The data wrapper is the one
//!   exception php-src registers for a scheme with no slashes, so `data:text/plain,x` and
//!   `data://text/plain,x` both match on the `data:` prefix alone.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// The prefixes php-src resolves to a non-local wrapper, longest first so `https://`
/// is tested before `http://` cannot shadow it (they diverge at byte 4 either way).
const NON_LOCAL_PREFIXES: [(&str, &[u8]); 5] = [
    ("https", b"https://"),
    ("http", b"http://"),
    ("ftps", b"ftps://"),
    ("ftp", b"ftp://"),
    ("data", b"data:"),
];

/// Emits `__rt_stream_is_local_path(ptr, len) -> 1 local / 0 remote`.
///
/// AArch64 receives `x1`/`x2` (the string pair) and answers in `x0`; x86_64 receives
/// `rax`/`rdx` and answers in `rax`.
pub fn emit_stream_is_local_path(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 classifier.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: classify a stream_is_local() path ---");
    emitter.label_global("__rt_stream_is_local_path");
    emitter.instruction("mov x9, x1");                                          // the candidate path
    emitter.instruction("mov x10, x2");                                         // and its byte length
    emitter.instruction("cbz x9, __rt_sil_local");                              // a null path names no wrapper
    for (tag, prefix) in NON_LOCAL_PREFIXES {
        let miss = format!("__rt_sil_not_{tag}");
        emitter.instruction(&format!("cmp x10, #{}", prefix.len()));            // long enough to carry the scheme?
        emitter.instruction(&format!("b.lt {miss}"));
        for (offset, byte) in prefix.iter().enumerate() {
            emitter.instruction(&format!("ldrb w12, [x9, #{offset}]"));         // one candidate scheme byte
            if byte.is_ascii_alphabetic() {
                emitter.instruction("orr w12, w12, #0x20");                     // fold case: PHP matches schemes case-insensitively
            }
            emitter.instruction(&format!("cmp w12, #{}", byte.to_ascii_lowercase()));
            emitter.instruction(&format!("b.ne {miss}"));
        }
        emitter.instruction("mov x0, #0");                                      // a remote wrapper is not local
        emitter.instruction("ret");
        emitter.label(&miss);
    }
    emitter.label("__rt_sil_local");
    emitter.instruction("mov x0, #1");                                          // every other wrapper, and every bare path, is local
    emitter.instruction("ret");
}

/// Emits the Linux x86_64 classifier.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: classify a stream_is_local() path ---");
    emitter.label_global("__rt_stream_is_local_path");
    emitter.instruction("mov r9, rax");                                         // the candidate path
    emitter.instruction("mov r10, rdx");                                        // and its byte length
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_sil_local_x86");                               // a null path names no wrapper
    for (tag, prefix) in NON_LOCAL_PREFIXES {
        let miss = format!("__rt_sil_not_{tag}_x86");
        emitter.instruction(&format!("cmp r10, {}", prefix.len()));             // long enough to carry the scheme?
        emitter.instruction(&format!("jl {miss}"));
        for (offset, byte) in prefix.iter().enumerate() {
            emitter.instruction(&format!("movzx r11d, BYTE PTR [r9 + {offset}]")); // one candidate scheme byte
            if byte.is_ascii_alphabetic() {
                emitter.instruction("or r11d, 0x20");                           // fold case: PHP matches schemes case-insensitively
            }
            emitter.instruction(&format!("cmp r11d, {}", byte.to_ascii_lowercase()));
            emitter.instruction(&format!("jne {miss}"));
        }
        emitter.instruction("xor eax, eax");                                    // a remote wrapper is not local
        emitter.instruction("ret");
        emitter.label(&miss);
    }
    emitter.label("__rt_sil_local_x86");
    emitter.instruction("mov eax, 1");                                          // every other wrapper, and every bare path, is local
    emitter.instruction("ret");
}
