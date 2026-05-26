//! Purpose:
//! Emits the `__rt_basename`, `__rt_pathinfo_str` runtime helper assembly for pathinfo str.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};

/// pathinfo (component-flag form): return one component of a path as a string.
/// Input:  x1/x2 = path, x3 = flag (1=DIRNAME, 2=BASENAME, 4=EXTENSION, 8=FILENAME)
/// Output: x1/x2 = component string (empty when the requested component is absent)
///
/// PHP accepts component bitmasks; when several component bits are present it
/// returns the first component in DIRNAME → BASENAME → EXTENSION → FILENAME
/// order. Exact PATHINFO_ALL is handled by the array helper before reaching
/// this routine; if a caller reaches this helper with exact 15 anyway, it
/// fails closed to an empty string rather than returning a misleading
/// component.
///
/// EXTENSION / FILENAME are computed by first reducing the path to its
/// basename (via `__rt_basename`) and then locating the last `.` in the
/// resulting slice.
pub fn emit_pathinfo_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pathinfo_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: pathinfo (single-flag form) ---");
    emitter.label_global("__rt_pathinfo_str");

    // Reserve a frame because we tail-call helpers that establish their own.
    emitter.instruction("sub sp, sp, #16");                                     // allocate frame for the saved frame regs
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- dispatch on the flag value --
    emitter.instruction("cmp x3, #15");                                         // guard against direct PATHINFO_ALL calls to the string helper
    emitter.instruction("b.eq __rt_pathinfo_empty");                            // fail closed instead of returning a misleading component string
    emitter.instruction("and x9, x3, #1");                                      // does the bitmask request PATHINFO_DIRNAME first?
    emitter.instruction("cbnz x9, __rt_pathinfo_dirname");                      // delegate to dirname runtime when dirname is present
    emitter.instruction("and x9, x3, #2");                                      // does the bitmask request PATHINFO_BASENAME next?
    emitter.instruction("cbnz x9, __rt_pathinfo_basename");                     // delegate to basename runtime when basename is present
    emitter.instruction("and x9, x3, #4");                                      // does the bitmask request PATHINFO_EXTENSION next?
    emitter.instruction("cbnz x9, __rt_pathinfo_extension");                    // compute extension from basename when requested
    emitter.instruction("and x9, x3, #8");                                      // does the bitmask request PATHINFO_FILENAME last?
    emitter.instruction("cbnz x9, __rt_pathinfo_filename");                     // compute filename when requested
    emitter.label("__rt_pathinfo_empty");
    emitter.instruction("mov x1, #0");                                          // return empty pointer
    emitter.instruction("mov x2, #0");                                          // return empty length
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_dirname");
    emitter.instruction("cbz x2, __rt_pathinfo_empty");                         // pathinfo("", PATHINFO_DIRNAME) returns "" rather than dirname("") = "."
    emitter.instruction("bl __rt_dirname");                                     // run dirname; result in x1/x2
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_basename");
    emitter.instruction("mov x3, #0");                                          // basename takes optional suffix; pass empty
    emitter.instruction("mov x4, #0");                                          // suffix length 0
    emitter.instruction("bl __rt_basename");                                    // run basename; result in x1/x2
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_extension");
    emitter.instruction("mov x3, #0");                                          // basename with empty suffix
    emitter.instruction("mov x4, #0");                                          // suffix length 0
    emitter.instruction("bl __rt_basename");                                    // x1/x2 now point at the basename slice
    // Find the last '.' in the basename slice.
    emitter.instruction("mov x5, x2");                                          // scan index = length
    emitter.label("__rt_pathinfo_ext_scan");
    emitter.instruction("cbz x5, __rt_pathinfo_ext_none");                      // no '.' encountered → empty extension
    emitter.instruction("sub x9, x5, #1");                                      // candidate index
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load candidate byte
    emitter.instruction("cmp w10, #0x2E");                                      // is it '.'?
    emitter.instruction("b.eq __rt_pathinfo_ext_found");                        // located the dot
    emitter.instruction("sub x5, x5, #1");                                      // step left
    emitter.instruction("b __rt_pathinfo_ext_scan");                            // continue scanning

    emitter.label("__rt_pathinfo_ext_found");
    // Dot at index x5-1. PHP treats leading-dot names as having an extension,
    // but trailing-dot names have an empty extension key in the array form.
    emitter.instruction("add x1, x1, x5");                                      // skip past the dot
    emitter.instruction("sub x2, x2, x5");                                      // remaining bytes form the extension
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_ext_none");
    emitter.instruction("mov x1, #0");                                          // empty extension
    emitter.instruction("mov x2, #0");                                          // empty length
    emitter.instruction("b __rt_pathinfo_done");                                // unwind frame and return

    emitter.label("__rt_pathinfo_filename");
    emitter.instruction("mov x3, #0");                                          // basename with empty suffix
    emitter.instruction("mov x4, #0");                                          // suffix length 0
    emitter.instruction("bl __rt_basename");                                    // x1/x2 = basename slice
    // Find the last '.'; PHP trims leading-dot names to an empty filename.
    emitter.instruction("mov x5, x2");                                          // scan index = length
    emitter.label("__rt_pathinfo_filename_scan");
    emitter.instruction("cbz x5, __rt_pathinfo_done");                          // no dot found → keep the full basename
    emitter.instruction("sub x9, x5, #1");                                      // candidate index
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load candidate byte
    emitter.instruction("cmp w10, #0x2E");                                      // is it '.'?
    emitter.instruction("b.eq __rt_pathinfo_filename_trim");                    // located the trimming dot
    emitter.instruction("sub x5, x5, #1");                                      // step left
    emitter.instruction("b __rt_pathinfo_filename_scan");                       // continue scanning

    emitter.label("__rt_pathinfo_filename_trim");
    emitter.instruction("sub x2, x5, #1");                                      // length becomes everything before the last dot
    // x1 unchanged: filename starts at the same position as the basename.

    emitter.label("__rt_pathinfo_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate frame
    emitter.instruction("ret");                                                 // return component slice in x1/x2
}

/// x86_64 Linux pathinfo (single-flag form): returns one component of a path as a string.
///
/// ABI: `rax` = path pointer, `rdx` = path length, `rdi` = flag → `rax/rdx` = result.
/// Flag values match the ARM64 convention: 1=DIRNAME, 2=BASENAME, 4=EXTENSION, 8=FILENAME.
///
/// Shares the same component-selection order, fail-closed PATHINFO_ALL guard, and
/// PHP-compatible basename/extension/filename semantics as the ARM64 emitter.
/// dirname is called with the path pointer/length in the same registers per the
/// x86_64 System V ABI.
fn emit_pathinfo_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pathinfo (single-flag form) ---");
    emitter.label_global("__rt_pathinfo_str");

    // ABI: rax=path_ptr, rdx=path_len, rdi=flag → rax/rdx=result

    emitter.instruction("push rbp");                                            // preserve caller frame pointer while pathinfo dispatches
    emitter.instruction("mov rbp, rsp");                                        // establish stable frame base

    emitter.instruction("cmp rdi, 15");                                         // dynamic PATHINFO_ALL cannot be returned by the string helper
    emitter.instruction("je __rt_pathinfo_empty_x86");                          // fail closed instead of returning a misleading component string
    emitter.instruction("test rdi, 1");                                         // does the bitmask request PATHINFO_DIRNAME first?
    emitter.instruction("jnz __rt_pathinfo_dirname_x86");                       // delegate to dirname when dirname is present
    emitter.instruction("test rdi, 2");                                         // does the bitmask request PATHINFO_BASENAME next?
    emitter.instruction("jnz __rt_pathinfo_basename_x86");                      // delegate to basename when basename is present
    emitter.instruction("test rdi, 4");                                         // does the bitmask request PATHINFO_EXTENSION next?
    emitter.instruction("jnz __rt_pathinfo_extension_x86");                     // compute extension when requested
    emitter.instruction("test rdi, 8");                                         // does the bitmask request PATHINFO_FILENAME last?
    emitter.instruction("jnz __rt_pathinfo_filename_x86");                      // compute filename when requested
    emitter.label("__rt_pathinfo_empty_x86");
    emitter.instruction("xor eax, eax");                                        // unsupported flag → empty pointer
    emitter.instruction("xor edx, edx");                                        // unsupported flag → empty length
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return empty string

    emitter.label("__rt_pathinfo_dirname_x86");
    emitter.instruction("test rdx, rdx");                                       // pathinfo("", PATHINFO_DIRNAME) returns "" rather than dirname("") = "."
    emitter.instruction("jz __rt_pathinfo_empty_x86");                          // preserve PHP's pathinfo-specific empty-path rule
    emitter.instruction("call __rt_dirname");                                   // dirname; result in rax/rdx
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return result

    emitter.label("__rt_pathinfo_basename_x86");
    emitter.instruction("xor edi, edi");                                        // basename suffix pointer = 0
    emitter.instruction("xor esi, esi");                                        // basename suffix length = 0
    emitter.instruction("call __rt_basename");                                  // basename; result in rax/rdx
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return result

    emitter.label("__rt_pathinfo_extension_x86");
    emitter.instruction("xor edi, edi");                                        // basename suffix pointer = 0
    emitter.instruction("xor esi, esi");                                        // basename suffix length = 0
    emitter.instruction("call __rt_basename");                                  // basename; result in rax/rdx
    emitter.instruction("mov r8, rdx");                                         // r8 = scan index = basename length
    emitter.label("__rt_pathinfo_ext_scan_x86");
    emitter.instruction("test r8, r8");                                         // exhausted basename without finding '.'?
    emitter.instruction("jz __rt_pathinfo_ext_none_x86");                       // → empty extension
    emitter.instruction("mov r9, r8");                                          // candidate index = r8 - 1
    emitter.instruction("sub r9, 1");                                           // step left
    emitter.instruction("movzx ecx, BYTE PTR [rax + r9]");                      // load candidate byte
    emitter.instruction("cmp cl, 0x2E");                                        // is it '.'?
    emitter.instruction("je __rt_pathinfo_ext_found_x86");                      // located the dot
    emitter.instruction("sub r8, 1");                                           // step left
    emitter.instruction("jmp __rt_pathinfo_ext_scan_x86");                      // continue scanning
    emitter.label("__rt_pathinfo_ext_found_x86");
    emitter.instruction("add rax, r8");                                         // skip past the dot
    emitter.instruction("sub rdx, r8");                                         // remaining bytes form the extension
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return extension
    emitter.label("__rt_pathinfo_ext_none_x86");
    emitter.instruction("xor eax, eax");                                        // empty extension pointer
    emitter.instruction("xor edx, edx");                                        // empty extension length
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return empty string

    emitter.label("__rt_pathinfo_filename_x86");
    emitter.instruction("xor edi, edi");                                        // basename suffix pointer = 0
    emitter.instruction("xor esi, esi");                                        // basename suffix length = 0
    emitter.instruction("call __rt_basename");                                  // basename; result in rax/rdx
    emitter.instruction("mov r8, rdx");                                         // r8 = scan index = basename length
    emitter.label("__rt_pathinfo_filename_scan_x86");
    emitter.instruction("test r8, r8");                                         // exhausted the basename without finding a dot?
    emitter.instruction("jz __rt_pathinfo_filename_done_x86");                  // no dot found → keep full basename
    emitter.instruction("mov r9, r8");                                          // candidate index = r8 - 1
    emitter.instruction("sub r9, 1");                                           // step left
    emitter.instruction("movzx ecx, BYTE PTR [rax + r9]");                      // load candidate byte
    emitter.instruction("cmp cl, 0x2E");                                        // is it '.'?
    emitter.instruction("je __rt_pathinfo_filename_trim_x86");                  // trim at this position
    emitter.instruction("sub r8, 1");                                           // step left
    emitter.instruction("jmp __rt_pathinfo_filename_scan_x86");                 // continue scanning
    emitter.label("__rt_pathinfo_filename_trim_x86");
    emitter.instruction("mov rdx, r8");                                         // length becomes everything before the dot
    emitter.instruction("sub rdx, 1");                                          // drop the dot itself
    emitter.label("__rt_pathinfo_filename_done_x86");
    emitter.instruction("pop rbp");                                             // restore frame pointer
    emitter.instruction("ret");                                                 // return filename slice
}
