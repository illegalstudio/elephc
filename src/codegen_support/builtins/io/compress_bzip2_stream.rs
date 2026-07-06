//! Purpose:
//! Lowers `fopen("compress.bzip2://path", ...)` calls. Opens the underlying
//! file read-only, slurps the bzip2-compressed payload, decompresses it via
//! libbz2's one-shot `BZ2_bzBuffToBuffDecompress`, writes the plain bytes
//! to an anonymous temp file, then `dup2`s that fd onto the original
//! descriptor so subsequent fread/fseek/feof see the decompressed bytes
//! transparently.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::fopen::emit()` when the path literal
//!   begins with `compress.bzip2://`.
//!
//! Key details:
//! - The URL must be a string literal; the prefix is stripped at compile
//!   time and the underlying path is opened with mode "r".
//! - Slurp cap = 64 KiB (`_stream_filter_buf`), output cap = 256x the
//!   compressed size (min 64 KiB) — matches the `zlib.inflate` budget.
//! - libbz2 is referenced only from this builtin's USER asm, so programs
//!   that don't use `compress.bzip2://` neither link against nor reference
//!   libbz2. The checker emits `require_builtin_library("bz2")` for
//!   programs that do.
//! - On decompress failure (non-zero return) we skip the dup2 and let the
//!   source fd stay positioned at end-of-file — fread returns empty bytes,
//!   matching how the `zlib.inflate` filter degrades on broken input.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

const FILTER_BUF_SIZE: i64 = 65536;

/// Emits a `fopen("compress.bzip2://...", ...)` call. The path is known to
/// be a string literal beginning with `compress.bzip2://`.
pub fn emit(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fopen() compress.bzip2:// stream");
    let underlying = match &args[0].kind {
        ExprKind::StringLiteral(path) => path.strip_prefix("compress.bzip2://").map(str::to_string),
        _ => None,
    };
    super::fopen::emit_mode_and_ignored_optional_args(args, emitter, ctx, data);
    let underlying = match underlying {
        Some(p) if !p.is_empty() => p,
        _ => {
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction("mov x0, #-1"),            // negative fd sentinel for PHP false
                Arch::X86_64 => emitter.instruction("mov rax, -1"),             // negative fd sentinel for PHP false
            }
            super::fopen::box_fopen_result(emitter, ctx);
            return Some(PhpType::Mixed);
        }
    };

    let (path_sym, path_len) = data.add_string(underlying.as_bytes());
    let (mode_sym, mode_len) = data.add_string(b"r");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &path_sym);
            emitter.instruction(&format!("mov x2, #{}", path_len));             // path length
            abi::emit_symbol_address(emitter, "x3", &mode_sym);
            emitter.instruction(&format!("mov x4, #{}", mode_len));             // mode length
            abi::emit_call_label(emitter, "__rt_fopen");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rax", &path_sym);
            emitter.instruction(&format!("mov rdx, {}", path_len));             // path length
            abi::emit_symbol_address(emitter, "rdi", &mode_sym);
            emitter.instruction(&format!("mov rsi, {}", mode_len));             // mode length
            abi::emit_call_label(emitter, "__rt_fopen");
        }
    }

    let false_label = ctx.next_label("cbz2_false");
    let done_label = ctx.next_label("cbz2_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // negative fd = source open failed
            emitter.instruction(&format!("b.lt {}", false_label));              // box false when the source open failed
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // negative fd = source open failed
            emitter.instruction(&format!("js {}", false_label));                // sign bit set = negative fd
        }
    }

    match emitter.target.arch {
        Arch::AArch64 => emit_arm64(emitter, |prefix| ctx.next_label(prefix)),
        Arch::X86_64 => emit_x86_64(emitter, |prefix| ctx.next_label(prefix)),
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", done_label)),     // skip false boxing after bzip2 setup
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", done_label)),    // skip false boxing after bzip2 setup
    }
    emitter.label(&false_label);
    super::fopen::box_fopen_result(emitter, ctx);
    emitter.label(&done_label);
    Some(PhpType::Mixed)
}

/// ARM64: 96-byte stack frame.
/// Layout:
///   [sp,  0..8)   source fd
///   [sp,  8..16)  slurp offset / compressed length
///   [sp, 16..24)  decompressed buffer pointer
///   [sp, 24..32)  decompressed length (after BZ2 call)
///   [sp, 32..40)  temp fd
///   [sp, 40..48)  write offset
///   [sp, 48..56)  destLen u32 spill (in: capacity, out: bytes written)
///   [sp, 56..64)  padding
///   [sp, 64..72)  saved x29
///   [sp, 72..80)  saved x30
pub(super) fn emit_arm64<F>(emitter: &mut Emitter, mut next_label: F)
where
    F: FnMut(&str) -> String,
{
    let slurp = next_label("bz2_slurp");
    let slurp_done = next_label("bz2_slurped");
    let write = next_label("bz2_write");
    let write_done = next_label("bz2_written");
    let decompress_fail = next_label("bz2_decompress_fail");
    let common_done = next_label("bz2_done_arm");

    emitter.instruction("sub sp, sp, #96");                                     // reserve the bzip2 scratch frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source fd

    // Slurp every compressed byte from the descriptor into _stream_filter_buf.
    emitter.instruction("str xzr, [sp, #8]");                                   // slurp offset = 0
    emitter.label(&slurp);
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd to read from
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");
    emitter.instruction("ldr x9, [sp, #8]");                                    // current compressed-byte count
    emitter.instruction("add x1, x1, x9");                                      // write ptr = buf + offset
    emitter.instruction(&format!("mov x2, #{}", FILTER_BUF_SIZE));              // total slurp buffer capacity
    emitter.instruction("sub x2, x2, x9");                                      // remaining capacity
    emitter.syscall(3);                                                         // read
    emitter.instruction("cmp x0, #0");                                          // did read return EOF or an error?
    emitter.instruction(&format!("b.le {}", slurp_done));                       // EOF or error
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload compressed-byte count
    emitter.instruction("add x9, x9, x0");                                      // include the bytes just read
    emitter.instruction("str x9, [sp, #8]");                                    // persist the compressed-byte count
    emitter.instruction(&format!("mov x10, #{}", FILTER_BUF_SIZE));             // slurp buffer capacity
    emitter.instruction("cmp x9, x10");                                         // is there still room to read more?
    emitter.instruction(&format!("b.lt {}", slurp));                            // continue slurping until buffer is full or EOF
    emitter.label(&slurp_done);

    // Size + allocate output buffer (256x input, min 64 KiB).
    emitter.instruction("ldr x9, [sp, #8]");                                    // compressed input length
    emitter.instruction("lsl x9, x9, #8");                                      // 256x compressed
    emitter.instruction(&format!("mov x10, #{}", FILTER_BUF_SIZE));             // minimum output capacity
    emitter.instruction("cmp x9, x10");                                         // compare computed capacity with minimum
    emitter.instruction("csel x9, x9, x10, gt");                                // max(256x, 64KiB)
    emitter.instruction("str w9, [sp, #48]");                                   // destLen = capacity (u32)
    emitter.instruction("mov x0, x9");                                          // allocation size for decompressed output
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate output buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap kind for the output buffer
    emitter.instruction("str x0, [sp, #16]");                                   // save output buffer ptr

    // BZ2_bzBuffToBuffDecompress(dest, &destLen, source, sourceLen, 0, 0).
    emitter.instruction("ldr x0, [sp, #16]");                                   // dest
    emitter.instruction("add x1, sp, #48");                                     // &destLen
    abi::emit_symbol_address(emitter, "x2", "_stream_filter_buf");              // source
    emitter.instruction("ldr x3, [sp, #8]");                                    // sourceLen (passed as w3 below)
    emitter.instruction("mov w4, #0");                                          // small = 0
    emitter.instruction("mov w5, #0");                                          // verbosity = 0
    emitter.bl_c("BZ2_bzBuffToBuffDecompress");                                 // libbz2 one-shot decompress
    emitter.instruction("cmp w0, #0");                                          // did libbz2 report an error?
    emitter.instruction(&format!("b.ne {}", decompress_fail));                  // non-zero = error → skip dup2

    emitter.instruction("ldr w9, [sp, #48]");                                   // destLen now holds bytes written
    emitter.instruction("str x9, [sp, #24]");                                   // save decompressed length

    // Back the descriptor with an anonymous temp file of the plain bytes.
    emitter.instruction("bl __rt_tmpfile");                                     // x0 = temp fd
    emitter.instruction("str x0, [sp, #32]");                                   // save temp fd

    // Write loop.
    emitter.instruction("str xzr, [sp, #40]");                                  // write offset = 0
    emitter.label(&write);
    emitter.instruction("ldr x10, [sp, #24]");                                  // total decompressed length
    emitter.instruction("ldr x9, [sp, #40]");                                   // write offset
    emitter.instruction("cmp x9, x10");                                         // has every decompressed byte been written?
    emitter.instruction(&format!("b.ge {}", write_done));                       // finish when output is fully written
    emitter.instruction("ldr x0, [sp, #32]");                                   // temp fd
    emitter.instruction("ldr x1, [sp, #16]");                                   // decompressed output buffer
    emitter.instruction("add x1, x1, x9");                                      // src = buf + offset
    emitter.instruction("sub x2, x10, x9");                                     // remaining bytes
    emitter.syscall(4);                                                         // write
    emitter.instruction("cmp x0, #0");                                          // did write make progress?
    emitter.instruction(&format!("b.le {}", write_done));                       // bail on error or short write
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload write offset
    emitter.instruction("add x9, x9, x0");                                      // advance by bytes written
    emitter.instruction("str x9, [sp, #40]");                                   // persist write offset
    emitter.instruction(&format!("b {}", write));                               // continue writing decompressed bytes
    emitter.label(&write_done);

    // lseek(temp_fd, 0, SEEK_SET) — rewind so reads start at byte 0.
    emitter.instruction("ldr x0, [sp, #32]");                                   // temp fd to rewind
    emitter.instruction("mov x1, #0");                                          // offset
    emitter.instruction("mov x2, #0");                                          // whence = SEEK_SET
    emitter.syscall(199);                                                       // lseek

    // dup2(temp_fd, source_fd) so subsequent reads see decompressed bytes.
    emitter.instruction("ldr x0, [sp, #32]");                                   // oldfd = temp fd
    emitter.instruction("ldr x1, [sp, #0]");                                    // newfd = source fd
    emitter.bl_c("dup2");                                                       // libc dup2
    emitter.instruction("ldr x0, [sp, #32]");                                   // close temp fd
    emitter.syscall(6);                                                         // close

    emitter.label(&decompress_fail);
    emitter.label(&common_done);
    emitter.instruction("ldr x0, [sp, #0]");                                    // return source fd
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the bzip2 scratch frame
    emitter.instruction("mov x1, x0");                                          // resource payload = fd
    emitter.instruction("mov x2, #0");                                          // resource mixed payloads have no high word
    emitter.instruction("mov x0, #9");                                          // tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// x86_64: same shape as ARM64. Frame is rbp-relative (-88 bytes).
pub(super) fn emit_x86_64<F>(emitter: &mut Emitter, mut next_label: F)
where
    F: FnMut(&str) -> String,
{
    let slurp = next_label("bz2_slurp_x");
    let slurp_done = next_label("bz2_slurped_x");
    let write = next_label("bz2_write_x");
    let write_done = next_label("bz2_written_x");
    let decompress_fail = next_label("bz2_decompress_fail_x");
    let common_done = next_label("bz2_done_x");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 88");                                         // reserve frame; 88≡8 mod 16 so rsp is 16-aligned at libc calls (push rbp made it 8)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save source fd

    // Slurp.
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // slurp offset
    emitter.label(&slurp);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd
    abi::emit_symbol_address(emitter, "rsi", "_stream_filter_buf");             // slurp buffer base
    emitter.instruction("add rsi, QWORD PTR [rbp - 16]");                       // ptr = buf + offset
    emitter.instruction(&format!("mov rdx, {}", FILTER_BUF_SIZE));              // total slurp buffer capacity
    emitter.instruction("sub rdx, QWORD PTR [rbp - 16]");                       // remaining
    emitter.instruction("call read");                                           // read
    emitter.instruction("cmp rax, 0");                                          // did read return EOF or an error?
    emitter.instruction(&format!("jle {}", slurp_done));                        // stop slurping on EOF or error
    emitter.instruction("add QWORD PTR [rbp - 16], rax");                       // bump offset
    emitter.instruction(&format!("cmp QWORD PTR [rbp - 16], {}", FILTER_BUF_SIZE)); // is there still room to read more?
    emitter.instruction(&format!("jl {}", slurp));                              // continue slurping until buffer is full or EOF
    emitter.label(&slurp_done);

    // Size + allocate output buffer.
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // compressed len
    emitter.instruction("shl rax, 8");                                          // 256x
    emitter.instruction(&format!("mov rcx, {}", FILTER_BUF_SIZE));              // minimum output capacity
    emitter.instruction("cmp rax, rcx");                                        // compare computed capacity with minimum
    emitter.instruction("cmovl rax, rcx");                                      // max(256x, 64KiB)
    emitter.instruction("mov DWORD PTR [rbp - 48], eax");                       // destLen u32 = capacity
    emitter.instruction("mov rdi, rax");                                        // allocation size for decompressed output
    emitter.instruction("call __rt_heap_alloc");                                // allocate output buffer
    emitter.instruction("mov QWORD PTR [rax - 8], 1");                          // heap kind = string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save output buffer ptr

    // BZ2_bzBuffToBuffDecompress(dest, &destLen, source, sourceLen, 0, 0).
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // dest
    emitter.instruction("lea rsi, [rbp - 48]");                                 // &destLen
    abi::emit_symbol_address(emitter, "rdx", "_stream_filter_buf");             // source
    emitter.instruction("mov ecx, DWORD PTR [rbp - 16]");                       // sourceLen u32 (compressed len)
    emitter.instruction("xor r8d, r8d");                                        // small = 0
    emitter.instruction("xor r9d, r9d");                                        // verbosity = 0
    emitter.bl_c("BZ2_bzBuffToBuffDecompress");                                 // libbz2 one-shot decompress
    emitter.instruction("test eax, eax");                                       // did libbz2 report an error?
    emitter.instruction(&format!("jnz {}", decompress_fail));                   // non-zero = error

    emitter.instruction("mov eax, DWORD PTR [rbp - 48]");                       // destLen now holds decompressed length
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save decompressed length

    // Temp file backing.
    emitter.instruction("call __rt_tmpfile");                                   // rax = temp fd
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save temp fd

    // Write loop.
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // write offset
    emitter.label(&write);
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // total
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // offset
    emitter.instruction("cmp rax, rcx");                                        // has every decompressed byte been written?
    emitter.instruction(&format!("jge {}", write_done));                        // finish when output is fully written
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // temp fd
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // decompressed output buffer
    emitter.instruction("add rsi, rax");                                        // src = buf + offset
    emitter.instruction("mov rdx, rcx");                                        // total decompressed length
    emitter.instruction("sub rdx, rax");                                        // remaining bytes
    emitter.instruction("call write");                                          // copy decompressed bytes into the temp fd
    emitter.instruction("cmp rax, 0");                                          // did write make progress?
    emitter.instruction(&format!("jle {}", write_done));                        // bail on error or short write
    emitter.instruction("add QWORD PTR [rbp - 56], rax");                       // advance by bytes written
    emitter.instruction(&format!("jmp {}", write));                             // continue writing decompressed bytes
    emitter.label(&write_done);

    // lseek + dup2 + close.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // temp fd to rewind
    emitter.instruction("xor esi, esi");                                        // offset = 0
    emitter.instruction("xor edx, edx");                                        // whence = SEEK_SET
    emitter.instruction("call lseek");                                          // libc lseek
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // oldfd = temp fd
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // newfd = source fd
    emitter.instruction("call dup2");                                           // replace source fd with temp fd contents
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // temp fd to close after dup2
    emitter.instruction("call close");                                          // close the temporary descriptor

    emitter.label(&decompress_fail);
    emitter.label(&common_done);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return source fd
    emitter.instruction("add rsp, 88");                                         // release the 88-byte frame (matches the aligned sub rsp, 88)
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("mov rdi, rax");                                        // resource payload = fd
    emitter.instruction("xor esi, esi");                                        // resource mixed payloads have no high word
    emitter.instruction("mov eax, 9");                                          // tag 9 = resource
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}
