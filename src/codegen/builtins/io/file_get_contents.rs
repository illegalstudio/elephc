//! Purpose:
//! Emits PHP `file_get_contents` file input builtin calls.
//! Coordinates filesystem paths, PHAR entries, and built-in URL wrappers with
//! runtime helpers that allocate and box returned string or false results.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Literal `http://`, `https://`, `ftp://`, and `ftps://` URLs reuse the same
//!   wrapper open helpers as `fopen()`, then slurp and close the descriptor.
//! - Dynamic paths route through a runtime URL dispatcher; when TLS is required,
//!   the program entry point publishes TLS entry points before user code runs.
//! - Failure paths must distinguish PHP false from empty string results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits code for the PHP `file_get_contents` builtin.
///
/// `args[0]` is evaluated and pushed as the filename argument, then `__rt_file_get_contents`
/// is called. On AArch64 the filename is passed in `x0`; on x86_64 in `rdi`. The result is
/// always boxed into `PhpType::Mixed` — a successful read yields a string (tag 1), while
/// failure yields bool false (tag 3). Returns `PhpType::Mixed` unconditionally.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("file_get_contents()");
    // A literal phar:// URL is read and decoded at compile time (uncompressed,
    // gzip, or bzip2) and the entry's bytes are embedded as the string result —
    // the same compile-time model as fopen("phar://...","r"). A missing archive
    // or entry yields PHP false. (A non-literal phar:// path is read at run time
    // through fopen + stream_get_contents instead.)
    if let crate::parser::ast::ExprKind::StringLiteral(url) = &args[0].kind {
        if url.starts_with("phar://") {
            match super::phar_stream::extract_phar_entry(url) {
                Some(bytes) => {
                    let (sym, len) = data.add_string(&bytes);
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            abi::emit_symbol_address(emitter, "x1", &sym);
                            emitter.instruction(&format!("mov x2, #{}", len));  // embedded entry length → string-result length register
                        }
                        Arch::X86_64 => {
                            abi::emit_symbol_address(emitter, "rax", &sym);
                            emitter.instruction(&format!("mov rdx, {}", len));  // embedded entry length → string-result length register
                        }
                    }
                }
                None => match emitter.target.arch {
                    Arch::AArch64 => emitter.instruction("mov x1, #0"),         // missing archive/entry → null string ptr → boxed false
                    Arch::X86_64 => emitter.instruction("xor eax, eax"),        // missing archive/entry → null string ptr → boxed false
                },
            }
            box_file_get_contents_result(emitter, ctx);
            return Some(PhpType::Mixed);
        }
        // Literal http/https/ftp/ftps URLs open the wrapper, slurp the whole body
        // into an owned string, and box it — the fopen() + stream_get_contents()
        // + fclose() model. A failed open boxes PHP false.
        if url.starts_with("http://") {
            super::http_stream::emit_open_fd(args, emitter, data);
            emit_url_slurp_and_box(emitter, ctx);
            return Some(PhpType::Mixed);
        }
        if url.starts_with("https://") {
            super::https_stream::emit_open_fd(args, emitter, data);
            emit_url_slurp_and_box(emitter, ctx);
            return Some(PhpType::Mixed);
        }
        if url.starts_with("ftps://") {
            super::ftps_stream::emit_open_fd(args, emitter, data);
            emit_url_slurp_and_box(emitter, ctx);
            return Some(PhpType::Mixed);
        }
        if url.starts_with("ftp://") {
            super::ftp_stream::emit_open_fd(args, emitter, data);
            emit_url_slurp_and_box(emitter, ctx);
            return Some(PhpType::Mixed);
        }
    }
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_file_get_contents_maybe_url");          // routes dynamic URL/phar paths before the filesystem reader
    box_file_get_contents_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the raw string result (pointer + length) into a `PhpType::Mixed` cell.
///
/// On AArch64 the runtime helper places the string pointer in `x1` and length in `x2`;
/// on x86_64 in `rax` and `rdx`. A null pointer signals failure — this path emits bool
/// false (tag 3) via `__rt_mixed_from_value`. On success, the string pointer/length are
/// stored directly into a heap-allocated mixed cell (tag 1) without copying the buffer.
pub(super) fn box_file_get_contents_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("fgc_false");
    let done_label = ctx.next_label("fgc_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null runtime string pointer means file_get_contents() failed
            abi::emit_push_reg_pair(emitter, "x1", "x2");                      // preserve the successful file payload while allocating the mixed box
            emitter.instruction("mov x0, #24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful string payload
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocated payload as a mixed cell
            emitter.instruction("mov x9, #1");                                  // runtime tag 1 = string
            emitter.instruction("str x9, [x0]");                                // store the string tag in the mixed result
            abi::emit_pop_reg_pair(emitter, "x10", "x11");                     // reload the owned file string pointer and length
            emitter.instruction("stp x10, x11, [x0, #8]");                      // store the string payload words without copying the owned file buffer
            emitter.instruction(&format!("b {}", done_label));                  // skip the false boxing path after a successful read
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for file_get_contents() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // a null runtime string pointer means file_get_contents() failed
            emitter.instruction(&format!("jz {}", false_label));                // box false when the runtime helper reports failure
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                    // preserve the successful file payload while allocating the mixed box
            emitter.instruction("mov rax, 24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful string payload
            emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the mixed-cell heap kind word with the x86_64 heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocated payload as a mixed cell
            emitter.instruction("mov r10, 1");                                  // runtime tag 1 = string
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the string tag in the mixed result
            abi::emit_pop_reg_pair(emitter, "r10", "r11");                     // reload the owned file string pointer and length
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the string pointer without copying the owned file buffer
            emitter.instruction("mov QWORD PTR [rax + 16], r11");               // store the string length without copying the owned file buffer
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false boxing path after a successful read
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for file_get_contents() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
    }
}

/// Given an open stream fd in the int-result register (`x0`/`rax`), or `-1` on a
/// failed open, slurps the whole stream into an **owned** string, closes the fd,
/// and boxes the result as `file_get_contents()`'s `PhpType::Mixed` (string on
/// success, bool `false` on a failed open). The slurp uses the TLS-aware
/// `__rt_stream_get_contents` read-all helper and then `__rt_str_persist` so the
/// boxed string owns its bytes and survives later `_concat_buf` reuse.
fn emit_url_slurp_and_box(emitter: &mut Emitter, ctx: &mut Context) {
    let fail_label = ctx.next_label("fgc_url_fail");
    let done_label = ctx.next_label("fgc_url_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // a failed wrapper open returns -1
            emitter.instruction(&format!("b.lt {}", fail_label));               // box false when the open failed
            emitter.instruction("sub sp, sp, #32");                             // [sp,#0]=fd, [sp,#8]=ptr, [sp,#16]=len
            emitter.instruction("str x0, [sp, #0]");                            // save the fd for the close below
            abi::emit_call_label(emitter, "__rt_stream_get_contents");          // (x0=fd) → x1=ptr, x2=len (concat_buf slice)
            emitter.instruction("stp x1, x2, [sp, #8]");                        // save the slurped ptr/len across the close
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the fd
            super::fclose::emit_tls_session_teardown(emitter, ctx);
            emitter.syscall(6);                                                 // close(fd)
            emitter.instruction("ldp x1, x2, [sp, #8]");                        // restore the slurped ptr/len
            abi::emit_call_label(emitter, "__rt_str_persist");                  // copy to owned heap → x1=ptr, x2=len
            emitter.instruction("add sp, sp, #32");                             // release the slurp frame
            emitter.instruction(&format!("b {}", done_label));                  // boxed string payload is ready
            emitter.label(&fail_label);
            emitter.instruction("mov x1, #0");                                  // null string ptr → boxed false
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // a failed wrapper open returns -1
            emitter.instruction(&format!("jl {}", fail_label));                 // box false when the open failed
            emitter.instruction("sub rsp, 32");                                 // [rsp+0]=fd, [rsp+8]=ptr, [rsp+16]=len
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the fd for the close below
            emitter.instruction("mov rdi, rax");                                // __rt_stream_get_contents takes the fd in rdi
            abi::emit_call_label(emitter, "__rt_stream_get_contents");          // rax=ptr, rdx=len (concat_buf slice)
            emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // save the slurped ptr across the close
            emitter.instruction("mov QWORD PTR [rsp + 16], rdx");               // save the slurped len across the close
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // reload the fd for TLS teardown
            super::fclose::emit_tls_session_teardown(emitter, ctx);
            emitter.instruction("mov rdi, rax");                                // move the restored fd into close()'s argument register
            emitter.instruction("call close");                                  // close(fd) via libc
            emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                // restore the slurped ptr
            emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");               // restore the slurped len
            abi::emit_call_label(emitter, "__rt_str_persist");                  // copy to owned heap → rax=ptr, rdx=len
            emitter.instruction("add rsp, 32");                                 // release the slurp frame
            emitter.instruction(&format!("jmp {}", done_label));                // boxed string payload is ready
            emitter.label(&fail_label);
            emitter.instruction("xor eax, eax");                                // null string ptr → boxed false
            emitter.label(&done_label);
        }
    }
    box_file_get_contents_result(emitter, ctx);
}
