//! Purpose:
//! Emits UTF-8/UTF-16 conversion helpers shared by native wide-path Win32 shims.
//!
//! Called from:
//! - `super::emit_win32_shims()` before filesystem shims that call `*W` APIs.
//!
//! Key details:
//! - PHP paths remain UTF-8 internally; Win32 receives allocated UTF-16 strings.
//! - Input follows PHP's ASCII -> strict UTF-8 -> active-ACP compatibility sequence.
//! - UTF-8 conversion uses `MB_ERR_INVALID_CHARS`; the ACP fallback uses its normal flags.
//! - Buffers returned by `__rt_win_utf8_to_utf16` are owned by the caller.

use crate::codegen::emit::Emitter;

/// Emits PHP-compatible byte-to-UTF-16 and strict UTF-16-to-UTF-8 helpers for Win32 APIs.
pub(super) fn emit_win32_encoding_helpers(emitter: &mut Emitter) {
    emit_utf8_to_utf16(emitter);
    emit_utf8_path_to_utf16(emitter);
    emit_utf16_to_utf8(emitter);
}

/// Emits `__rt_win_utf8_to_utf16(const char*) -> WCHAR*`, allocating the result.
///
/// The historical symbol name is retained because every wide-path shim calls it. Its
/// semantics deliberately match php-src's `php_win32_cp_conv_any_to_w`: direct ASCII,
/// strict UTF-8, then the process ACP for legacy byte strings. Every success returns a
/// fresh runtime-heap allocation containing a terminating UTF-16 NUL.
fn emit_utf8_to_utf16(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_utf8_to_utf16");
    // -- reject the paths Win32 would silently rewrite, before converting --
    // php-src applies PHP_WIN32_IOUTIL_CHECK_PATH_W at every ioutil entry point
    // (win32/ioutil.h): a path ending in a space, or ending in '.' where the
    // preceding byte is neither a separator nor another '.', is refused with
    // ERROR_ACCESS_DENIED. Win32 otherwise strips those trailing characters, so
    // unlink('keep.txt.') would delete 'keep.txt' -- a different file than the one
    // named. Every caller of this helper is a path-taking filesystem shim, which is
    // exactly the set php-src guards, so the check belongs at this choke point.
    emitter.instruction("xor eax, eax");                                        // scan index for the NUL terminator
    emitter.label(".Lpathchk_len");
    emitter.instruction("cmp BYTE PTR [rdi + rax], 0");                         // reached the end of the UTF-8 path?
    emitter.instruction("je .Lpathchk_have_len");                               // rax now holds the byte length
    emitter.instruction("inc rax");                                             // advance one byte
    emitter.instruction("jmp .Lpathchk_len");                                   // keep scanning
    emitter.label(".Lpathchk_have_len");
    emitter.instruction("test rax, rax");                                       // empty path?
    emitter.instruction("jz .Lpathchk_ok");                                     // nothing to reject
    emitter.instruction("mov cl, BYTE PTR [rdi + rax - 1]");                    // final byte
    emitter.instruction("cmp cl, 0x20");                                        // trailing space is always refused
    emitter.instruction("je .Lpathchk_reject");                                 // -> EACCES
    emitter.instruction("cmp cl, 0x2E");                                        // trailing dot needs the neighbour test
    emitter.instruction("jne .Lpathchk_ok");                                    // any other final byte is fine
    emitter.instruction("cmp rax, 1");                                          // a lone "." names the current directory
    emitter.instruction("jbe .Lpathchk_ok");                                    // and is legal
    emitter.instruction("mov cl, BYTE PTR [rdi + rax - 2]");                    // byte before the trailing dot
    emitter.instruction("cmp cl, 0x2E");                                        // ".." is legal
    emitter.instruction("je .Lpathchk_ok");                                     // parent directory
    emitter.instruction("cmp cl, 0x2F");                                        // "foo/." is legal
    emitter.instruction("je .Lpathchk_ok");                                     // trailing current-directory component
    emitter.instruction("cmp cl, 0x5C");                                        // "foo\\." likewise, both separators count
    emitter.instruction("je .Lpathchk_ok");                                     // trailing current-directory component
    emitter.label(".Lpathchk_reject");
    emitter.instruction("mov DWORD PTR [rip + __rt_win32_last_error], 5");      // ERROR_ACCESS_DENIED, as php sets
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 13");                // EACCES
    emitter.instruction("xor eax, eax");                                        // NULL signals a refused path
    emitter.instruction("ret");                                                 // no frame has been established yet
    emitter.label(".Lpathchk_ok");
    emitter.instruction("sub rsp, 88");                                         // shadow space, conversion args, and aligned locals
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // preserve the UTF-8 source across Win32 calls
    emitter.instruction("mov DWORD PTR [rsp + 56], eax");                       // preserve the source byte length for the ASCII fast path
    // -- php_win32_cp_conv_ascii_to_w: avoid code-page APIs for pure ASCII --
    emitter.instruction("xor ecx, ecx");                                        // begin scanning source bytes at index zero
    emitter.label(".Lutf8_to_utf16_ascii_scan");
    emitter.instruction("cmp ecx, DWORD PTR [rsp + 56]");                       // inspected every source byte?
    emitter.instruction("jae .Lutf8_to_utf16_ascii_alloc");                     // yes: direct widening is safe
    emitter.instruction("movzx edx, BYTE PTR [rdi + rcx]");                     // inspect the next source byte
    emitter.instruction("test edx, 0x80");                                      // byte outside the ASCII range?
    emitter.instruction("jnz .Lutf8_to_utf16_utf8_query");                      // defer non-ASCII input to strict UTF-8
    emitter.instruction("inc ecx");                                             // continue ASCII validation
    emitter.instruction("jmp .Lutf8_to_utf16_ascii_scan");                      // scan the remaining bytes
    emitter.label(".Lutf8_to_utf16_ascii_alloc");
    emitter.instruction("mov eax, DWORD PTR [rsp + 56]");                       // source byte length equals ASCII WCHAR count
    emitter.instruction("inc eax");                                             // reserve the UTF-16 terminator
    emitter.instruction("movsxd rax, eax");                                     // widen the allocation element count
    emitter.instruction("shl rax, 1");                                          // WCHAR uses two bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate the fresh owned UTF-16 result
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lutf8_to_utf16_fail");                             // NULL preserves allocation failure ownership
    emitter.instruction("xor ecx, ecx");                                        // begin byte-to-WCHAR widening at index zero
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // reload the ASCII source after allocation
    emitter.label(".Lutf8_to_utf16_ascii_copy");
    emitter.instruction("cmp ecx, DWORD PTR [rsp + 56]");                       // copied all source bytes?
    emitter.instruction("jae .Lutf8_to_utf16_ascii_done");                      // terminate the resulting WCHAR string
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load one known-ASCII byte
    emitter.instruction("mov WORD PTR [rax + rcx * 2], dx");                    // widen it without a code-page rewrite
    emitter.instruction("inc ecx");                                             // advance the shared byte/WCHAR index
    emitter.instruction("jmp .Lutf8_to_utf16_ascii_copy");                      // copy the remaining ASCII bytes
    emitter.label(".Lutf8_to_utf16_ascii_done");
    emitter.instruction("mov WORD PTR [rax + rcx * 2], 0");                     // append the required UTF-16 NUL
    emitter.instruction("add rsp, 88");                                         // restore stack before returning ownership
    emitter.instruction("ret");                                                 // return the fresh ASCII UTF-16 buffer
    // -- php_win32_cp_conv_utf8_to_w: reject malformed UTF-8 before compatibility fallback --
    emitter.label(".Lutf8_to_utf16_utf8_query");
    emitter.instruction("mov ecx, 65001");                                      // CodePage = CP_UTF8
    emitter.instruction("mov edx, 8");                                          // flags = MB_ERR_INVALID_CHARS
    emitter.instruction("mov r8, rdi");                                         // source UTF-8 string
    emitter.instruction("mov r9d, -1");                                         // include the terminating NUL
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // query required size without an output buffer
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // output capacity is zero for the size query
    emitter.instruction("call MultiByteToWideChar");                            // obtain required WCHAR count
    emitter.instruction("test eax, eax");                                       // did strict UTF-8 validation succeed?
    emitter.instruction("jz .Lutf8_to_utf16_acp_query");                        // invalid UTF-8 may be a legacy ACP byte string
    emitter.instruction("mov DWORD PTR [rsp + 60], eax");                       // preserve strict UTF-8 WCHAR count
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
    emitter.instruction("mov eax, DWORD PTR [rsp + 60]");                       // reload strict UTF-8 WCHAR capacity
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // destination capacity in WCHARs
    emitter.instruction("call MultiByteToWideChar");                            // perform strict conversion
    emitter.instruction("test eax, eax");                                       // conversion succeeded?
    emitter.instruction("jz .Lutf8_to_utf16_free_fail");                        // release the allocation on improbable conversion failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // return the owned UTF-16 buffer
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return WCHAR pointer
    emitter.label(".Lutf8_to_utf16_free_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // allocation to release
    emitter.instruction("call __rt_heap_free");                                 // avoid leaking a failed conversion buffer
    emitter.instruction("jmp .Lutf8_to_utf16_acp_query");                       // preserve PHP's ACP fallback after strict conversion failure
    // -- php_win32_cp_conv_to_w(GetACP(), flags): retain legacy script compatibility --
    emitter.label(".Lutf8_to_utf16_acp_query");
    emitter.instruction("call GetACP");                                         // obtain the process ANSI code page used by legacy PHP scripts
    emitter.instruction("mov DWORD PTR [rsp + 72], eax");                       // preserve ACP across the size query
    emitter.instruction("mov ecx, eax");                                        // CodePage = GetACP()
    emitter.instruction("xor edx, edx");                                        // ACP conversion uses its normal php-src-compatible flags
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // source byte string
    emitter.instruction("mov r9d, -1");                                         // include the terminating NUL
    emitter.instruction("mov QWORD PTR [rsp + 32], 0");                         // query required WCHAR count
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // no output buffer during size query
    emitter.instruction("call MultiByteToWideChar");                            // ask the active ACP for the output size
    emitter.instruction("test eax, eax");                                       // ACP recognizes and converted the source?
    emitter.instruction("jz .Lutf8_to_utf16_fail");                             // no compatible byte conversion exists
    emitter.instruction("mov DWORD PTR [rsp + 60], eax");                       // preserve ACP WCHAR capacity
    emitter.instruction("movsxd rax, eax");                                     // widen the allocation element count
    emitter.instruction("shl rax, 1");                                          // WCHAR uses two bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate a fresh owned ACP result
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lutf8_to_utf16_fail");                             // NULL leaves no buffer to release
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // preserve destination ownership across conversion
    emitter.instruction("mov ecx, DWORD PTR [rsp + 72]");                       // reload the active ANSI code page
    emitter.instruction("xor edx, edx");                                        // ACP conversion uses its normal php-src-compatible flags
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // source byte string
    emitter.instruction("mov r9d, -1");                                         // include the terminating NUL
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // fresh UTF-16 destination
    emitter.instruction("mov eax, DWORD PTR [rsp + 60]");                       // reload ACP WCHAR capacity
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // pass destination capacity in WCHARs
    emitter.instruction("call MultiByteToWideChar");                            // perform ACP compatibility conversion
    emitter.instruction("test eax, eax");                                       // conversion succeeded?
    emitter.instruction("jz .Lutf8_to_utf16_acp_free_fail");                    // release the owned allocation before failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // return the owned ACP-converted UTF-16 buffer
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return WCHAR pointer
    emitter.label(".Lutf8_to_utf16_acp_free_fail");
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // allocation to release after ACP conversion failure
    emitter.instruction("call __rt_heap_free");                                 // balance failed ACP conversion ownership
    emitter.label(".Lutf8_to_utf16_fail");
    emitter.instruction("xor eax, eax");                                        // NULL signals conversion failure
    emitter.instruction("add rsp, 88");                                         // restore stack
    emitter.instruction("ret");                                                 // return NULL
    emitter.blank();
}

/// Emits the PHP filesystem boundary `__rt_win_utf8_path_to_utf16`.
///
/// `php_win32_ioutil_any_to_w()` keeps ordinary short paths untouched, but
/// resolves paths which would cross `MAX_PATH`, canonicalizes separators and
/// dot components, and adds the extended-length prefix (`\\?\` or
/// `\\?\UNC\`).  Keep that policy out of the generic encoding helper: the
/// latter is also a byte-to-wide conversion primitive for non-filesystem
/// callers.  The filesystem shims own the returned allocation and release it
/// on every path.
fn emit_utf8_path_to_utf16(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_utf8_path_to_utf16");
    emitter.instruction("sub rsp, 176");                                        // shadow space and owned conversion/path locals
    emitter.instruction("mov QWORD PTR [rsp + 56], rdi");                       // preserve the PHP byte path across the generic conversion
    emitter.instruction("call __rt_win_utf8_to_utf16");                         // retain PHP's ASCII/UTF-8/ACP conversion semantics
    emitter.instruction("test rax, rax");                                       // conversion failed?
    emitter.instruction("jz .Lpathconv_fail");                                  // caller maps the failure through its existing errno path
    emitter.instruction("mov QWORD PTR [rsp + 48], rax");                       // owned UTF-16 source path

    // Measure the converted path without imposing a limit on ordinary short
    // paths.  PHP only enters the normalization/prefixing path at MAX_PATH.
    emitter.instruction("xor ecx, ecx");                                        // WCHAR index
    emitter.instruction("mov rdx, rax");                                        // converted path
    emitter.label(".Lpathconv_len");
    emitter.instruction("cmp WORD PTR [rdx + rcx * 2], 0");                     // terminating WCHAR reached?
    emitter.instruction("je .Lpathconv_len_done");                              // preserve the measured length
    emitter.instruction("inc rcx");                                             // advance one WCHAR
    emitter.instruction("jmp .Lpathconv_len");                                  // continue scanning
    emitter.label(".Lpathconv_len_done");
    emitter.instruction("mov QWORD PTR [rsp + 64], rcx");                       // source WCHAR count

    // Existing extended and NT junction paths are already in the form PHP
    // expects.  Do not prepend a second extended prefix.
    emitter.instruction("cmp rcx, 4");                                          // enough WCHARs for an extended prefix?
    emitter.instruction("jb .Lpathconv_prefix_probe_done");                     // short paths cannot be extended
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // reload converted path
    emitter.instruction("mov rdx, 0x005c003f005c005c");                         // L\\\\?\\
    emitter.instruction("cmp QWORD PTR [rax], rdx");                            // ordinary extended path?
    emitter.instruction("je .Lpathconv_return_original");                       // preserve it verbatim
    emitter.instruction("mov rdx, 0x005c003f003f005c");                         // L\\??\\ (NT junction form)
    emitter.instruction("cmp QWORD PTR [rax], rdx");                            // junction path?
    emitter.instruction("je .Lpathconv_return_original");                       // preserve it verbatim
    emitter.label(".Lpathconv_prefix_probe_done");

    // Absolute drive and UNC paths do not need the current directory in the
    // length calculation.  Relative paths use GetCurrentDirectoryW(0, NULL),
    // exactly as php_win32_ioutil_conv_any_to_w() does.
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // converted path
    emitter.instruction("cmp QWORD PTR [rsp + 64], 2");                         // enough WCHARs for a UNC prefix?
    emitter.instruction("jb .Lpathconv_relative");                              // otherwise this is relative
    emitter.instruction("cmp WORD PTR [rax], 0x5c");                            // first UNC separator
    emitter.instruction("jne .Lpathconv_drive_probe");                          // not UNC
    emitter.instruction("cmp WORD PTR [rax + 2], 0x5c");                        // second UNC separator
    emitter.instruction("je .Lpathconv_absolute");                              // UNC length is self-contained
    emitter.label(".Lpathconv_drive_probe");
    emitter.instruction("cmp QWORD PTR [rsp + 64], 3");                         // enough WCHARs for a drive root?
    emitter.instruction("jb .Lpathconv_relative");                              // otherwise this is relative
    emitter.instruction("cmp WORD PTR [rax + 2], 0x3a");                        // drive-letter separator ':'
    emitter.instruction("jne .Lpathconv_relative");                             // drive-relative C:foo still needs the current directory
    emitter.instruction("cmp WORD PTR [rax + 4], 0x2f");                        // forward-slash drive root
    emitter.instruction("je .Lpathconv_absolute");                              // C:/...
    emitter.instruction("cmp WORD PTR [rax + 4], 0x5c");                        // backslash drive root
    emitter.instruction("je .Lpathconv_absolute");                              // C:\\...
    emitter.label(".Lpathconv_relative");
    emitter.instruction("xor ecx, ecx");                                        // nSize=0 asks for the required WCHAR count
    emitter.instruction("xor edx, edx");                                        // lpBuffer=NULL
    emitter.instruction("xor r8d, r8d");                                        // unused third MS ABI register slot
    emitter.instruction("xor r9d, r9d");                                        // unused fourth MS ABI register slot
    emitter.instruction("call GetCurrentDirectoryW");                           // obtain PHP's relative-path base length
    emitter.instruction("test eax, eax");                                       // current-directory query failed?
    emitter.instruction("jz .Lpathconv_fail_owned");                            // release the converted source
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // dir_len includes the terminating NUL
    emitter.instruction("mov rdx, QWORD PTR [rsp + 64]");                       // mb_len
    emitter.instruction("add rdx, rax");                                        // dir_len + mb_len
    emitter.instruction("cmp rdx, 260");                                        // PHP's _MAX_PATH threshold
    emitter.instruction("jb .Lpathconv_return_original");                       // short relative paths stay relative
    emitter.instruction("jmp .Lpathconv_long");                                 // normalize and prefix the long relative path
    emitter.label(".Lpathconv_absolute");
    emitter.instruction("mov QWORD PTR [rsp + 72], 0");                         // absolute/UNC paths have no relative base
    emitter.instruction("cmp QWORD PTR [rsp + 64], 260");                       // PHP prefixes only when the path is long
    emitter.instruction("jb .Lpathconv_return_original");                       // preserve short absolute and UNC spelling

    // Allocate a tight upper bound from the current-directory and source lengths.
    // A fixed 32768-WCHAR temporary would consume 64 KiB from elephc's bounded
    // PHP heap even for a 260-character path and could turn long-path support
    // into a false heap-exhaustion fatal under a small `--heap-size`.
    emitter.label(".Lpathconv_long");
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // current-directory WCHAR count or zero for absolute paths
    emitter.instruction("add rax, QWORD PTR [rsp + 64]");                       // include every source-path WCHAR
    emitter.instruction("jc .Lpathconv_fail_owned");                            // reject length arithmetic overflow
    emitter.instruction("inc rax");                                             // reserve an explicit terminating WCHAR
    emitter.instruction("jz .Lpathconv_fail_owned");                            // reject wrapped terminator capacity
    emitter.instruction("cmp rax, 32768");                                      // Win32 extended-path WCHAR ceiling
    emitter.instruction("jae .Lpathconv_fail_owned");                           // reject an unrepresentable path instead of truncating it
    emitter.instruction("mov DWORD PTR [rsp + 88], eax");                       // retain the canonical buffer capacity
    emitter.instruction("shl rax, 1");                                          // WCHAR uses two bytes
    emitter.instruction("call __rt_heap_alloc");                                // allocate a path-sized canonical temporary
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lpathconv_fail_owned");                            // release the converted source
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // temporary buffer ownership
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // lpFileName = converted PHP path
    emitter.instruction("mov edx, DWORD PTR [rsp + 88]");                       // exact nBufferLength in WCHARs
    emitter.instruction("mov r8, rax");                                         // lpBuffer = temporary canonical path
    emitter.instruction("xor r9d, r9d");                                        // lpFilePart is not needed
    emitter.instruction("call GetFullPathNameW");                               // resolve relative paths and dot components
    emitter.instruction("test eax, eax");                                       // canonicalization failed?
    emitter.instruction("jz .Lpathconv_fail_temp");                             // release both owned buffers
    emitter.instruction("cmp eax, DWORD PTR [rsp + 88]");                       // buffer too small (return is required length)
    emitter.instruction("jae .Lpathconv_fail_temp");                            // fail closed rather than truncate a path
    emitter.instruction("mov QWORD PTR [rsp + 96], rax");                       // canonical WCHAR count

    // Plain UNC paths become \\\\?\\UNC\\ + path without its two leading
    // separators.  Drive paths become \\\\?\\ + path unchanged.
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // canonical path
    emitter.instruction("cmp WORD PTR [rax], 0x5c");                            // UNC first separator?
    emitter.instruction("jne .Lpathconv_drive_output");                         // drive/absolute output
    emitter.instruction("cmp WORD PTR [rax + 2], 0x5c");                        // UNC second separator?
    emitter.instruction("jne .Lpathconv_drive_output");                         // not a plain UNC path
    emitter.instruction("mov QWORD PTR [rsp + 112], 6");                        // UNC prefix adds eight and removes two WCHARs
    emitter.instruction("mov rax, QWORD PTR [rsp + 96]");                       // canonical length
    emitter.instruction("add rax, 7");                                          // +6 prefix delta + terminating NUL
    emitter.instruction("jmp .Lpathconv_output_alloc");                         // allocate the extended UNC result
    emitter.label(".Lpathconv_drive_output");
    emitter.instruction("mov QWORD PTR [rsp + 112], 4");                        // drive prefix adds four WCHARs
    emitter.instruction("mov rax, QWORD PTR [rsp + 96]");                       // canonical length
    emitter.instruction("add rax, 5");                                          // +4 prefix + terminating NUL
    emitter.label(".Lpathconv_output_alloc");
    emitter.instruction("shl rax, 1");                                          // WCHAR byte count
    emitter.instruction("call __rt_heap_alloc");                                // final extended path buffer
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lpathconv_fail_temp");                             // release temporary + source
    emitter.instruction("mov QWORD PTR [rsp + 104], rax");                      // final path ownership
    emitter.instruction("mov rdx, QWORD PTR [rsp + 112]");                      // prefix kind (4 drive / 6 UNC)
    emitter.instruction("cmp rdx, 6");                                          // plain UNC output?
    emitter.instruction("jne .Lpathconv_write_drive_prefix");                   // ordinary extended drive prefix
    emitter.instruction("mov rdx, 0x005c003f005c005c");                         // L\\\\?\\
    emitter.instruction("mov QWORD PTR [rax], rdx");                            // first four WCHARs
    emitter.instruction("mov rdx, 0x005c0043004e0055");                         // LUNC\\
    emitter.instruction("mov QWORD PTR [rax + 8], rdx");                        // next four WCHARs
    emitter.instruction("mov r9, 2");                                           // source offset in WCHARs
    emitter.instruction("mov r8, 8");                                           // destination offset in WCHARs
    emitter.instruction("jmp .Lpathconv_copy_prefix");                          // copy the canonical remainder
    emitter.label(".Lpathconv_write_drive_prefix");
    emitter.instruction("mov rdx, 0x005c003f005c005c");                         // L\\\\?\\
    emitter.instruction("mov QWORD PTR [rax], rdx");                            // four-WCHAR drive prefix
    emitter.instruction("xor r9d, r9d");                                        // source offset in WCHARs
    emitter.instruction("mov r8, 4");                                           // destination offset in WCHARs
    emitter.label(".Lpathconv_copy_prefix");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 80]");                       // canonical source
    emitter.instruction("mov rsi, QWORD PTR [rsp + 104]");                      // extended destination
    emitter.instruction("mov rcx, QWORD PTR [rsp + 96]");                       // total source length
    emitter.instruction("sub rcx, r9");                                         // remainder length after UNC separators
    emitter.instruction("xor rdx, rdx");                                        // remainder index
    emitter.label(".Lpathconv_copy_loop");
    emitter.instruction("cmp rdx, rcx");                                        // remainder copied?
    emitter.instruction("jae .Lpathconv_copy_done");                            // append the terminating NUL
    emitter.instruction("mov r10, r9");                                         // source base offset (x86 has one scaled index)
    emitter.instruction("add r10, rdx");                                        // source base + remainder index
    emitter.instruction("movzx eax, WORD PTR [rdi + r10 * 2]");                 // copy one canonical WCHAR
    emitter.instruction("cmp ax, 0x2f");                                        // PHP normalizes forward slashes to backslashes
    emitter.instruction("jne .Lpathconv_copy_store");                           // already a backslash/ordinary WCHAR
    emitter.instruction("mov ax, 0x5c");                                        // native Win32 separator
    emitter.label(".Lpathconv_copy_store");
    emitter.instruction("mov r10, r8");                                         // destination base offset
    emitter.instruction("add r10, rdx");                                        // destination base + remainder index
    emitter.instruction("mov WORD PTR [rsi + r10 * 2], ax");                    // store normalized WCHAR
    emitter.instruction("inc rdx");                                             // advance remainder index
    emitter.instruction("jmp .Lpathconv_copy_loop");                            // continue copying
    emitter.label(".Lpathconv_copy_done");
    emitter.instruction("mov r10, r8");                                         // destination base offset
    emitter.instruction("add r10, rcx");                                        // destination base + copied length
    emitter.instruction("mov WORD PTR [rsi + r10 * 2], 0");                     // terminate the owned extended path
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // release canonical temporary
    emitter.instruction("call __rt_heap_free");                                 // balance temporary ownership
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // release original conversion
    emitter.instruction("call __rt_heap_free");                                 // balance source ownership
    emitter.instruction("mov rax, QWORD PTR [rsp + 104]");                      // return final extended path
    emitter.instruction("add rsp, 176");                                        // restore caller stack
    emitter.instruction("ret");                                                 // return owned filesystem path

    emitter.label(".Lpathconv_return_original");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // short/already-extended path remains unchanged
    emitter.instruction("add rsp, 176");                                        // restore caller stack
    emitter.instruction("ret");                                                 // transfer source ownership to caller
    emitter.label(".Lpathconv_fail_temp");
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // temporary canonical path
    emitter.instruction("call __rt_heap_free");                                 // release it before returning failure
    emitter.label(".Lpathconv_fail_owned");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // converted source path
    emitter.instruction("call __rt_heap_free");                                 // release source ownership
    emitter.label(".Lpathconv_fail");
    emitter.instruction("xor eax, eax");                                        // NULL signals conversion/normalization failure
    emitter.instruction("add rsp, 176");                                        // restore caller stack
    emitter.instruction("ret");                                                 // return failure
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
