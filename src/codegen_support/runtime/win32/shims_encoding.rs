//! Purpose:
//! Emits UTF-8/UTF-16 conversion helpers shared by native wide-path Win32 shims.
//!
//! Called from:
//! - `super::emit_win32_shims()` before filesystem shims that call `*W` APIs.
//!
//! Key details:
//! - PHP paths remain UTF-8 internally; Win32 receives allocated UTF-16 strings.
//! - Conversion is strict (`MB_ERR_INVALID_CHARS` / `WC_ERR_INVALID_CHARS`) and includes NUL.
//! - Buffers returned by `__rt_win_utf8_to_utf16` are owned by the caller.

use crate::codegen::emit::Emitter;

/// Emits strict UTF-8-to-UTF-16 and UTF-16-to-UTF-8 helpers for Win32 path APIs.
pub(super) fn emit_win32_encoding_helpers(emitter: &mut Emitter) {
    emit_utf8_to_utf16(emitter);
    emit_utf16_to_utf8(emitter);
}

/// Emits `__rt_win_utf8_to_utf16(const char*) -> WCHAR*`, allocating the result.
fn emit_utf8_to_utf16(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_utf8_to_utf16");
    emitter.instruction("sub rsp, 72");                                         // shadow space, two stack args, and aligned locals
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // preserve the UTF-8 source across Win32 calls
    emitter.instruction("mov ecx, 65001");                                      // CodePage = CP_UTF8
    emitter.instruction("mov edx, 8");                                          // flags = MB_ERR_INVALID_CHARS
    emitter.instruction("mov r8, rdi");                                         // source UTF-8 string
    emitter.instruction("mov r9d, -1");                                         // include the terminating NUL
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // query required size without an output buffer
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // output capacity is zero for the size query
    emitter.instruction("call MultiByteToWideChar");                            // obtain required WCHAR count
    emitter.instruction("test eax, eax");                                       // did strict UTF-8 validation succeed?
    emitter.instruction("jz .Lutf8_to_utf16_fail");                             // invalid input or Win32 conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve WCHAR count
    emitter.instruction("movsxd rax, eax");                                     // widen the allocation element count
    emitter.instruction("shl rax, 1");                                          // WCHAR uses two bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate the owned UTF-16 result
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lutf8_to_utf16_fail");                             // propagate allocation failure as NULL
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // preserve the destination pointer
    emitter.instruction("mov ecx, 65001");                                      // CodePage = CP_UTF8
    emitter.instruction("mov edx, 8");                                          // flags = MB_ERR_INVALID_CHARS
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // source UTF-8 string
    emitter.instruction("mov r9d, -1");                                         // include the terminating NUL
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // destination UTF-16 buffer
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // reload WCHAR capacity
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // destination capacity in WCHARs
    emitter.instruction("call MultiByteToWideChar");                            // perform strict conversion
    emitter.instruction("test eax, eax");                                       // conversion succeeded?
    emitter.instruction("jz .Lutf8_to_utf16_free_fail");                        // release the allocation on failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // return the owned UTF-16 buffer
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return WCHAR pointer
    emitter.label(".Lutf8_to_utf16_free_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // allocation to release
    emitter.instruction("call __rt_heap_free");                                 // avoid leaking a failed conversion buffer
    emitter.label(".Lutf8_to_utf16_fail");
    emitter.instruction("xor eax, eax");                                        // NULL signals conversion failure
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return NULL
    emitter.blank();
}

/// Emits `__rt_win_utf16_to_utf8(const WCHAR*, char*, int) -> int`.
fn emit_utf16_to_utf8(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_utf16_to_utf8");
    emitter.instruction("sub rsp, 72");                                         // shadow space and four stack args, aligned
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // destination buffer is Win32 argument five
    emitter.instruction("mov QWORD PTR [rsp + 40], rdx");                       // destination byte capacity is argument six
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // no replacement character under strict conversion
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // caller does not need the used-default-char flag
    emitter.instruction("mov ecx, 65001");                                      // CodePage = CP_UTF8
    emitter.instruction("mov edx, 128");                                        // flags = WC_ERR_INVALID_CHARS
    emitter.instruction("mov r8, rdi");                                         // source UTF-16 string
    emitter.instruction("mov r9d, -1");                                         // include the terminating UTF-16 NUL
    emitter.instruction("call WideCharToMultiByte");                            // convert the entry name to PHP's UTF-8 representation
    emitter.instruction("cdqe");                                                // return the signed Win32 byte count
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return bytes including NUL, or zero on failure
    emitter.blank();
}
