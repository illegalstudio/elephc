//! Purpose:
//! Emits the `__rt_tempnam`, `__rt_tempnam_dir` runtime helper assembly for tempnam.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.
//! - The x86_64 `mkstemp` call site routes through `Emitter::emit_call_c`. On
//!   Windows, `__rt_sys_mkstemp` rewrites the trailing `XXXXXX`, creates the
//!   path atomically through `CreateFileW(CREATE_NEW)`, and returns a CRT
//!   descriptor. Other targets continue to use libc.

use crate::codegen_support::abi;
use crate::codegen_support::runtime::data::TEMPNAM_FALLBACK_NOTICE;
use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform}};

/// Emits the `__rt_tempnam` runtime helper for creating a unique temporary filename.
///
/// Uses libc `mkstemp` to create and immediately close a temp file — only the path is
/// returned, not an open descriptor. The returned string is heap-allocated and persisted.
///
/// Input (ARM64): x1/x2=dir string ptr/len, x3/x4=prefix string ptr/len
/// Output (ARM64): x1=temp filename ptr, x2=temp filename len
pub fn emit_tempnam(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_tempnam_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: tempnam ---");
    emitter.label_global("__rt_tempnam");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save inputs --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save dir ptr and len
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save prefix ptr and len

    // -- match PHP's basename(prefix) and 63-byte prefix limit --
    emitter.instruction("mov x12, x3");                                         // scan the original prefix without allocating an intermediate basename
    emitter.instruction("mov x13, x4");                                         // retain the remaining candidate basename byte length
    emitter.label("__rt_tempnam_prefix_scan");
    emitter.instruction("cbz x13, __rt_tempnam_prefix_limit");                  // finish once every input prefix byte has been inspected
    emitter.instruction("ldrb w14, [x12]");                                     // inspect the next prefix byte for a path separator
    emitter.instruction("cmp w14, #0x2F");                                      // slash starts a later basename component
    emitter.instruction("b.ne __rt_tempnam_prefix_advance");                    // retain the current basename candidate for ordinary bytes
    emitter.instruction("add x12, x12, #1");                                    // skip the slash so the following bytes become the basename candidate
    emitter.instruction("sub x13, x13, #1");                                    // remove the separator from the candidate length
    emitter.instruction("str x12, [sp, #16]");                                  // persist the latest basename starting pointer
    emitter.instruction("str x13, [sp, #24]");                                  // persist the bytes following that slash
    emitter.instruction("b __rt_tempnam_prefix_scan");                          // continue so the final slash wins
    emitter.label("__rt_tempnam_prefix_advance");
    emitter.instruction("add x12, x12, #1");                                    // advance past an ordinary prefix byte
    emitter.instruction("sub x13, x13, #1");                                    // consume that byte from the scan length
    emitter.instruction("b __rt_tempnam_prefix_scan");                          // inspect the following byte
    emitter.label("__rt_tempnam_prefix_limit");
    emitter.instruction("ldr x14, [sp, #24]");                                  // reload the selected basename byte length
    emitter.instruction("cmp x14, #63");                                        // PHP keeps at most 63 bytes of the basename prefix
    emitter.instruction("b.ls __rt_tempnam_prefix_ready");                      // no truncation is needed for short prefixes
    emitter.instruction("mov x14, #63");                                        // cap the basename prefix to PHP's documented limit
    emitter.instruction("str x14, [sp, #24]");                                  // use the capped length for template construction
    emitter.label("__rt_tempnam_prefix_ready");

    // -- build template path: dir + "/" + prefix + "XXXXXX" in _cstr_buf --
    abi::emit_symbol_address(emitter, "x9", "_cstr_buf");                       // load page address of cstr buffer
    emitter.instruction("mov x10, x9");                                         // save buffer start

    // -- copy dir bytes --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload dir ptr and len
    emitter.label("__rt_tempnam_dir");
    emitter.instruction("cbz x2, __rt_tempnam_slash");                          // if no bytes remain, add slash
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from dir, advance ptr
    emitter.instruction("strb w11, [x9], #1");                                  // store byte to buffer, advance ptr
    emitter.instruction("sub x2, x2, #1");                                      // decrement counter
    emitter.instruction("b __rt_tempnam_dir");                                  // continue copying

    // -- append '/' separator --
    emitter.label("__rt_tempnam_slash");
    emitter.instruction("mov w11, #0x2F");                                      // '/' character
    emitter.instruction("strb w11, [x9], #1");                                  // append slash to buffer

    // -- copy prefix bytes --
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload prefix ptr and len
    emitter.label("__rt_tempnam_pfx");
    emitter.instruction("cbz x2, __rt_tempnam_xx");                             // if no bytes remain, add XXXXXX
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from prefix, advance ptr
    emitter.instruction("strb w11, [x9], #1");                                  // store byte to buffer, advance ptr
    emitter.instruction("sub x2, x2, #1");                                      // decrement counter
    emitter.instruction("b __rt_tempnam_pfx");                                  // continue copying

    // -- append "XXXXXX" template suffix --
    emitter.label("__rt_tempnam_xx");
    emitter.instruction("mov w11, #0x58");                                      // 'X' character
    emitter.instruction("strb w11, [x9], #1");                                  // append X #1
    emitter.instruction("strb w11, [x9], #1");                                  // append X #2
    emitter.instruction("strb w11, [x9], #1");                                  // append X #3
    emitter.instruction("strb w11, [x9], #1");                                  // append X #4
    emitter.instruction("strb w11, [x9], #1");                                  // append X #5
    emitter.instruction("strb w11, [x9], #1");                                  // append X #6
    emitter.instruction("strb wzr, [x9]");                                      // null-terminate the template

    // -- call mkstemp to create the temp file (modifies XXXXXX in-place) --
    emitter.instruction("str x10, [sp, #32]");                                  // save buffer start (clobbered by mkstemp)
    emitter.instruction("mov x0, x10");                                         // pass template buffer to mkstemp
    emitter.bl_c("mkstemp");                                                    // mkstemp(template), x0=fd
    emitter.instruction("cmp x0, #0");                                          // a negative descriptor means temporary-file creation failed
    emitter.instruction("b.lt __rt_tempnam_fail");                              // return a null string sentinel instead of persisting the failed template

    // -- close the temp file (we only need the name) --
    emitter.syscall(6);

    // -- calculate length of resulting path --
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload buffer start (was clobbered by mkstemp)
    emitter.instruction("mov x2, #0");                                          // initialize length counter
    emitter.label("__rt_tempnam_len");
    emitter.instruction("ldrb w11, [x1, x2]");                                  // load byte at current position
    emitter.instruction("cbz w11, __rt_tempnam_copy");                          // if null, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_tempnam_len");                                  // continue scanning

    // -- copy result to concat_buf for safe return --
    emitter.label("__rt_tempnam_copy");
    emitter.instruction("bl __rt_str_persist");                                 // copy to heap, x1=new ptr, x2=len

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    emitter.label("__rt_tempnam_fail");
    emitter.instruction("mov x1, #0");                                          // use a null pointer to represent PHP false to the EIR caller
    emitter.instruction("mov x2, #0");                                          // clear the unused string length on failure
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the failed tempnam frame
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// x86_64-specific implementation of `__rt_tempnam`.
///
/// Builds a mkstemp template from dir + "/" + prefix + "XXXXXX", allocates a mutable owned
/// buffer, calls mkstemp to rewrite the Xs in-place, closes the file descriptor, and returns
/// the path as a heap-allocated persisted string via rax/rdx.
///
/// On Windows, a failed explicit directory retries once in the system temp
/// directory after a suppressible PHP notice. Other targets return the null
/// pointer sentinel that the EIR caller boxes as PHP `false`.
fn emit_tempnam_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: tempnam ---");
    emitter.label_global("__rt_tempnam");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while tempnam() uses path-component and template spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the preserved lengths, C strings, template pointer, and file descriptor
    emitter.instruction("sub rsp, 80");                                         // reserve aligned spill slots, retry state, and owned fallback directory storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // preserve the elephc directory string length across C-string conversion and template construction
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the elephc prefix string length across C-string conversion and template construction
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the elephc prefix string pointer across the directory C-string conversion helper call
    emitter.instruction("mov r8, rdi");                                         // start with the complete prefix as the basename candidate
    emitter.instruction("mov rcx, rsi");                                        // start with the complete prefix length as the candidate length
    emitter.instruction("xor r9d, r9d");                                        // prefix scan offset = 0
    emitter.label("__rt_tempnam_prefix_scan_x86");
    emitter.instruction("cmp r9, rsi");                                         // reached the end of the original prefix?
    emitter.instruction("jae __rt_tempnam_prefix_limit_x86");                   // select and cap the final basename candidate
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r9]");                     // load the next prefix byte without requiring a C string
    emitter.instruction("cmp r10b, 0x2F");                                      // slash begins a later basename component
    emitter.instruction("je __rt_tempnam_prefix_separator_x86");                // keep only the bytes after the latest slash
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("cmp r10b, 0x5C");                                  // backslash is also a Windows basename separator
        emitter.instruction("je __rt_tempnam_prefix_separator_x86");            // keep only the bytes after the latest backslash
    }
    emitter.instruction("inc r9");                                              // advance to the following ordinary prefix byte
    emitter.instruction("jmp __rt_tempnam_prefix_scan_x86");                    // continue finding the final path separator
    emitter.label("__rt_tempnam_prefix_separator_x86");
    emitter.instruction("lea r8, [rdi + r9 + 1]");                              // make the byte after this separator the new basename candidate
    emitter.instruction("mov rcx, rsi");                                        // recompute the remaining candidate length from the original prefix length
    emitter.instruction("sub rcx, r9");                                         // exclude bytes before and including this separator
    emitter.instruction("dec rcx");                                             // remove the separator itself from the candidate length
    emitter.instruction("inc r9");                                              // resume scanning after this separator
    emitter.instruction("jmp __rt_tempnam_prefix_scan_x86");                    // a later separator overrides this candidate
    emitter.label("__rt_tempnam_prefix_limit_x86");
    emitter.instruction("cmp rcx, 63");                                         // PHP truncates the basename prefix at 63 bytes
    emitter.instruction("jbe __rt_tempnam_prefix_ready_x86");                   // retain short basenames unchanged
    emitter.instruction("mov rcx, 63");                                         // cap the template prefix to PHP's documented limit
    emitter.label("__rt_tempnam_prefix_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // retain the selected basename pointer for template copying
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // retain the selected and capped basename byte length
    emitter.instruction("call __rt_cstr");                                      // convert the elephc directory string in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the C directory path pointer across the prefix conversion and template-construction loop
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // no system-temp fallback has been attempted yet
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // no owned fallback directory is awaiting release
    emitter.label("__rt_tempnam_build_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the directory string length before sizing the mutable mkstemp() template buffer
    emitter.instruction("add rax, QWORD PTR [rbp - 16]");                       // include the prefix string length in the mutable mkstemp() template buffer size
    emitter.instruction("add rax, 8");                                          // include '/', the six X template bytes, and the trailing null terminator in the mutable buffer size
    emitter.instruction("call __rt_heap_alloc");                                // allocate a mutable owned buffer for the mkstemp() template path
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the template buffer as a persisted elephc string in the uniform heap header
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the mutable template buffer pointer across the mkstemp() and close() calls
    emitter.instruction("mov r8, rax");                                         // keep a running destination cursor while copying the directory and prefix components into the template
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the C directory path pointer before copying its bytes into the template
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the directory string length before copying the directory bytes into the template
    emitter.label("__rt_tempnam_dir_copy_x86");
    emitter.instruction("test rcx, rcx");                                       // stop copying once every directory byte has been materialized into the mutable template buffer
    emitter.instruction("jz __rt_tempnam_dir_done_x86");                        // continue into the slash separator once the directory component has been fully copied
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load the next byte from the C directory path while constructing the mutable template
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the copied directory byte into the mutable mkstemp() template buffer
    emitter.instruction("add r9, 1");                                           // advance the C directory path cursor after copying one directory byte
    emitter.instruction("add r8, 1");                                           // advance the mutable template cursor after copying one directory byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of remaining directory bytes left to copy
    emitter.instruction("jmp __rt_tempnam_dir_copy_x86");                       // continue copying until the directory component is fully materialized
    emitter.label("__rt_tempnam_dir_done_x86");
    emitter.instruction("mov BYTE PTR [r8], 0x2F");                             // append the '/' separator between the directory component and the prefix component
    emitter.instruction("add r8, 1");                                           // advance the mutable template cursor past the inserted directory separator
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the basename prefix pointer before copying its bytes into the mutable template
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the prefix string length before copying the prefix bytes into the template
    emitter.label("__rt_tempnam_prefix_copy_x86");
    emitter.instruction("test rcx, rcx");                                       // stop copying once every prefix byte has been materialized into the mutable template buffer
    emitter.instruction("jz __rt_tempnam_xs_x86");                              // continue into the XXXXXX suffix once the prefix component has been fully copied
    emitter.instruction("mov r10b, BYTE PTR [r9]");                             // load the next byte from the C prefix string while constructing the mutable template
    emitter.instruction("mov BYTE PTR [r8], r10b");                             // store the copied prefix byte into the mutable mkstemp() template buffer
    emitter.instruction("add r9, 1");                                           // advance the C prefix cursor after copying one prefix byte
    emitter.instruction("add r8, 1");                                           // advance the mutable template cursor after copying one prefix byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of remaining prefix bytes left to copy
    emitter.instruction("jmp __rt_tempnam_prefix_copy_x86");                    // continue copying until the prefix component is fully materialized
    emitter.label("__rt_tempnam_xs_x86");
    emitter.instruction("mov BYTE PTR [r8], 0x58");                             // append template X #1 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 1], 0x58");                         // append template X #2 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 2], 0x58");                         // append template X #3 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 3], 0x58");                         // append template X #4 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 4], 0x58");                         // append template X #5 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 5], 0x58");                         // append template X #6 into the mutable mkstemp() buffer
    emitter.instruction("mov BYTE PTR [r8 + 6], 0");                            // append the trailing null terminator required by libc mkstemp()
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load any owned system-temp directory after its bytes were copied
    emitter.instruction("test rax, rax");                                       // did the fallback path allocate directory storage?
    emitter.instruction("jz __rt_tempnam_template_ready_x86");                  // caller-supplied directories use borrowed C-string storage
    emitter.instruction("call __rt_heap_free");                                 // release the copied fallback directory before creating the file
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // prevent duplicate cleanup if the retry also fails
    emitter.label("__rt_tempnam_template_ready_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // pass the mutable template buffer to libc mkstemp(), which rewrites the trailing XXXXXX in place
    emitter.emit_call_c("mkstemp");                                             // create the temp file (Windows: exclusive CreateFileW-backed shim)
    emitter.instruction("cmp eax, 0");                                          // detect a negative C int fd before trying to close it
    emitter.instruction("jl __rt_tempnam_fail_x86");                            // release the allocated template buffer and return false when mkstemp() fails
    emitter.instruction("cdqe");                                                // normalize the successful C int fd into the runtime's 64-bit descriptor value
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the returned file descriptor so the temp file can be closed before returning the path
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the mkstemp() file descriptor before closing the newly created temp file
    emitter.instruction("call close");                                          // close the temp file immediately because tempnam() returns only the path, not an open descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the owned mutable template buffer, now rewritten into the final temp path, in the x86_64 string result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // rebuild the temp path length from the original directory string length
    emitter.instruction("add rdx, QWORD PTR [rbp - 16]");                       // include the original prefix string length in the returned temp path length
    emitter.instruction("add rdx, 7");                                          // include the inserted '/' plus the six rewritten mkstemp() suffix characters in the returned temp path length
    emitter.instruction("add rsp, 80");                                         // release the temporary tempnam() spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the owned temp path string
    emitter.instruction("ret");                                                 // return the owned temp path string in the canonical x86_64 string result registers

    emitter.label("__rt_tempnam_fail_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the allocated template buffer pointer so the failed mkstemp() path can release it safely
    emitter.instruction("call __rt_heap_free");                                 // release the allocated template buffer when libc mkstemp() fails to create a temp file
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                     // has the system-temp fallback already been attempted?
        emitter.instruction("jne __rt_tempnam_final_fail_x86");                 // a failed retry is the terminal failure
        emitter.instruction("call __rt_sys_get_temp_dir");                      // retrieve PHP's Windows system temporary directory
        emitter.instruction("test rax, rax");                                   // did the native temp-directory query allocate a path?
        emitter.instruction("jz __rt_tempnam_final_fail_x86");                  // native lookup failure leaves tempnam false
        emitter.instruction("test rdx, rdx");                                   // is the returned directory non-empty?
        emitter.instruction("jz __rt_tempnam_release_empty_fallback_x86");      // release an unusable empty owned string
        emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                    // use the system-temp path length for the retry and return value
        emitter.instruction("mov QWORD PTR [rbp - 32], rax");                   // the owned UTF-8 result is already null terminated
        emitter.instruction("mov QWORD PTR [rbp - 72], rax");                   // retain ownership until the directory bytes are copied
        emitter.instruction("mov QWORD PTR [rbp - 64], 1");                     // permit exactly one fallback attempt
        emitter.instruction("lea rdi, [rip + _tempnam_fallback_notice]");       // PHP notice explaining the fallback directory
        emitter.instruction(&format!("mov esi, {}", TEMPNAM_FALLBACK_NOTICE.len())); // exact diagnostic byte length
        emitter.instruction("call __rt_diag_warning");                          // emit the notice unless @ suppression is active
        emitter.instruction("jmp __rt_tempnam_build_x86");                      // rebuild the template in the system directory
        emitter.label("__rt_tempnam_release_empty_fallback_x86");
        emitter.instruction("call __rt_heap_free");                             // release the unusable empty system-temp string
    }
    emitter.label("__rt_tempnam_final_fail_x86");
    emitter.instruction("xor eax, eax");                                        // return a null pointer sentinel when mkstemp() fails
    emitter.instruction("xor edx, edx");                                        // clear the unused string length on failure
    emitter.instruction("add rsp, 80");                                         // release the temporary tempnam() spill slots on the failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning false
    emitter.instruction("ret");                                                 // return the failure sentinel for PHP false boxing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Verifies Windows retries a failed explicit directory through the system temp helper.
    #[test]
    fn windows_emits_system_directory_fallback() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_tempnam(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("call __rt_sys_get_temp_dir"));
        assert!(asm.contains("lea rdi, [rip + _tempnam_fallback_notice]"));
        assert!(asm.contains("call __rt_diag_warning"));
        assert!(asm.contains("jmp __rt_tempnam_build_x86"));
    }
}
