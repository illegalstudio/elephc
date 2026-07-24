//! Purpose:
//! Emits Windows error-message formatting helpers for PHP-visible diagnostics.
//!
//! Called from:
//! - `super::emit_win32_shims()` after the UTF conversion helpers.
//!
//! Key details:
//! - Native messages come from `FormatMessageW`, never from the active ANSI code page.
//! - The returned UTF-8 buffer is heap-owned and must be released by its consumer.
//! - The helper trims neither punctuation nor whitespace; callers own PHP-specific presentation.

use crate::codegen::emit::Emitter;

/// Emits `__rt_win32_error_message(code)`, returning an owned UTF-8 system message or NULL.
pub(super) fn emit_win32_error_message(emitter: &mut Emitter) {
    emitter.label_global("__rt_win32_error_message");
    emitter.instruction("sub rsp, 1112");                                       // shadow, FormatMessageW stack args, 512 WCHARs, and aligned locals
    emitter.instruction("mov ecx, 0x1200");                                     // FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS
    emitter.instruction("xor edx, edx");                                        // no message source module
    emitter.instruction("mov r8d, edi");                                        // native Win32 or Winsock message identifier
    emitter.instruction("xor r9d, r9d");                                        // use the caller's default UI language
    emitter.instruction("lea rax, [rsp + 64]");                                 // stack-backed 512-WCHAR message buffer
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // lpBuffer
    emitter.instruction("mov QWORD PTR [rsp + 40], 512");                       // nSize in WCHARs
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // no insert arguments
    emitter.instruction("call FormatMessageW");                                 // obtain the localized Unicode system message
    emitter.instruction("test eax, eax");                                       // did Windows know this message identifier?
    emitter.instruction("jz .Lwin_error_message_fail");                         // unknown code or formatting failure
    // -- query the exact UTF-8 allocation size, including the terminator --
    emitter.instruction("mov ecx, 65001");                                      // CodePage = CP_UTF8
    emitter.instruction("xor edx, edx");                                        // valid system UTF-16 needs no special flags
    emitter.instruction("lea r8, [rsp + 64]");                                  // formatted UTF-16 message
    emitter.instruction("mov r9d, -1");                                         // include terminating NUL
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // size query has no destination
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // destination capacity zero requests required bytes
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // no replacement character
    emitter.instruction("mov QWORD PTR [rsp + 56], 0");                         // caller does not need used-default-char state
    emitter.instruction("call WideCharToMultiByte");                            // obtain required UTF-8 byte count
    emitter.instruction("test eax, eax");                                       // conversion size query succeeded?
    emitter.instruction("jz .Lwin_error_message_fail");                         // propagate conversion failure
    emitter.instruction("mov DWORD PTR [rsp + 1092], eax");                     // preserve byte capacity
    emitter.instruction("cdqe");                                                // widen allocation size
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned UTF-8 message
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lwin_error_message_fail");                         // return NULL on allocation failure
    emitter.instruction("mov QWORD PTR [rsp + 1096], rax");                     // preserve output pointer
    emitter.instruction("lea rdi, [rsp + 64]");                                 // helper arg1 = UTF-16 message
    emitter.instruction("mov rsi, rax");                                        // helper arg2 = allocated UTF-8 destination
    emitter.instruction("mov edx, DWORD PTR [rsp + 1092]");                     // helper arg3 = byte capacity
    emitter.instruction("call __rt_win_utf16_to_utf8");                         // perform strict UTF-8 conversion
    emitter.instruction("test eax, eax");                                       // conversion succeeded?
    emitter.instruction("jz .Lwin_error_message_free_fail");                    // release allocation on failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 1096]");                     // return owned UTF-8 message
    emitter.instruction("add rsp, 1112");                                       // restore stack
    emitter.instruction("ret");                                                 // return message pointer
    emitter.label(".Lwin_error_message_free_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 1096]");                     // failed output allocation
    emitter.instruction("call __rt_heap_free");                                 // avoid leaking a partially converted message
    emitter.label(".Lwin_error_message_fail");
    emitter.instruction("xor eax, eax");                                        // NULL means no system message was available
    emitter.instruction("add rsp, 1112");                                       // restore stack
    emitter.instruction("ret");                                                 // return NULL
    emitter.blank();
}

/// Emits failure-only capture helpers for Win32 and Winsock shims.
pub(super) fn emit_native_errno_capture_helpers(emitter: &mut Emitter) {
    emitter.label_global("__rt_win32_capture_errno");
    emitter.instruction("sub rsp, 40");                                         // align and reserve Win32 shadow space
    emitter.instruction("call GetLastError");                                   // fetch the failing kernel32 operation's native code
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], eax");    // retain native Win32 state separately
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate to the runtime's POSIX errno space
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno for observable failure handling
    emitter.instruction("mov rax, -1");                                         // standard POSIX failure sentinel
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.blank();

    emitter.label_global("__rt_wsa_capture_errno");
    emitter.instruction("sub rsp, 40");                                         // align and reserve Winsock shadow space
    emitter.instruction("call WSAGetLastError");                                // fetch the failing Winsock operation's native code
    emitter.instruction("mov DWORD PTR [rip + __rt_wsa_last_error], eax");      // retain Winsock state separately from Win32 state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate WSA error to POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno for PHP stream/socket code
    emitter.instruction("mov rax, -1");                                         // standard SOCKET_ERROR/POSIX failure sentinel
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.blank();
}
