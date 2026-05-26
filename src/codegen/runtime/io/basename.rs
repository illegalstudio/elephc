//! Purpose:
//! Emits the `__rt_basename`, `__rt_basename_strip` runtime helper assembly for basename.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_basename` runtime helper for ARM64 targets, with an x86_64 Linux variant.
///
/// **ARM64 ABI** (caller-saved: x0-x7 for args, x1=path_ptr, x2=path_len, x3=suffix_ptr, x4=suffix_len):
/// Returns the basename slice in **x1=result_ptr, x2=result_len**. The returned slice is a direct
/// pointer/length into the input path; no allocation or copy is performed.
///
/// **x86_64 Linux dispatch**: Delegates to `emit_basename_linux_x86_64()` which uses the System V AMD64
/// ABI (rax=path_ptr, rdx=path_len, rdi=suffix_ptr, rsi=suffix_len; returns rax=result_ptr, rdx=result_len).
///
/// **Behaviour** mirrors PHP's `basename()`:
/// - trailing `/` characters are stripped before scanning
/// - the substring after the last remaining `/` is returned
/// - if the suffix matches the tail of the result and is strictly shorter than the result, it is trimmed off
pub fn emit_basename(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_basename_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: basename ---");
    emitter.label_global("__rt_basename");

    // -- strip trailing slashes from the path --
    emitter.label("__rt_basename_strip");
    emitter.instruction("cbz x2, __rt_basename_done");                          // empty path: return as-is (length already 0)
    emitter.instruction("sub x9, x2, #1");                                      // index of last byte
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load last byte
    emitter.instruction("cmp w10, #0x2F");                                      // is it a forward slash?
    emitter.instruction("b.ne __rt_basename_scan_init");                        // if not, scan for the last separator
    emitter.instruction("sub x2, x2, #1");                                      // drop the trailing slash from the slice length
    emitter.instruction("b __rt_basename_strip");                               // continue stripping further trailing slashes

    // -- scan from the end for the last remaining slash --
    emitter.label("__rt_basename_scan_init");
    emitter.instruction("mov x5, x2");                                          // start scanning from the slice end (exclusive)
    emitter.label("__rt_basename_scan");
    emitter.instruction("cbz x5, __rt_basename_no_slash");                      // reached start without finding any slash
    emitter.instruction("sub x9, x5, #1");                                      // candidate index = x5 - 1
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load candidate byte
    emitter.instruction("cmp w10, #0x2F");                                      // is the candidate a separator?
    emitter.instruction("b.eq __rt_basename_slash");                            // found the rightmost slash, slice past it
    emitter.instruction("sub x5, x5, #1");                                      // step left toward the start of the path
    emitter.instruction("b __rt_basename_scan");                                // continue the right-to-left scan

    emitter.label("__rt_basename_slash");
    emitter.instruction("add x1, x1, x5");                                      // advance pointer past the trailing separator
    emitter.instruction("sub x2, x2, x5");                                      // length becomes everything after the separator
    emitter.instruction("b __rt_basename_suffix");                              // proceed to the optional suffix-trim step

    emitter.label("__rt_basename_no_slash");
    // -- no slash found: keep the full (already-trimmed) slice --

    // -- optionally trim a trailing suffix --
    emitter.label("__rt_basename_suffix");
    emitter.instruction("cbz x4, __rt_basename_done");                          // no suffix supplied: nothing more to do
    emitter.instruction("cmp x2, x4");                                          // PHP keeps the result when suffix is not strictly shorter
    emitter.instruction("b.le __rt_basename_done");                             // suffix length >= result length: do not trim
    emitter.instruction("sub x9, x2, x4");                                      // start of the candidate tail in the result
    emitter.instruction("mov x10, #0");                                         // suffix-comparison index
    emitter.label("__rt_basename_suffix_loop");
    emitter.instruction("cmp x10, x4");                                         // walked the entire suffix?
    emitter.instruction("b.ge __rt_basename_suffix_match");                     // every byte matched, drop the suffix
    emitter.instruction("add x11, x9, x10");                                    // absolute index inside the result
    emitter.instruction("ldrb w12, [x1, x11]");                                 // load result byte
    emitter.instruction("ldrb w13, [x3, x10]");                                 // load suffix byte
    emitter.instruction("cmp w12, w13");                                        // do they match?
    emitter.instruction("b.ne __rt_basename_done");                             // mismatch: keep the full basename
    emitter.instruction("add x10, x10, #1");                                    // advance to the next suffix byte
    emitter.instruction("b __rt_basename_suffix_loop");                         // keep comparing

    emitter.label("__rt_basename_suffix_match");
    emitter.instruction("sub x2, x2, x4");                                      // shrink the result by the matched suffix length

    emitter.label("__rt_basename_done");
    emitter.instruction("ret");                                                 // return basename slice in x1/x2
}

/// Emits the `__rt_basename` runtime helper for the x86_64 Linux target (System V AMD64 ABI).
///
/// **ABI**: rax=path_ptr, rdx=path_len, rdi=suffix_ptr, rsi=suffix_len; returns rax=result_ptr, rdx=result_len.
///
/// The returned slice is a direct pointer/length into the input path; no allocation or copy is performed.
///
/// **Behaviour** mirrors PHP's `basename()`:
/// - trailing `/` characters are stripped before scanning
/// - the substring after the last remaining `/` is returned
/// - if the suffix matches the tail of the result and is strictly shorter than the result, it is trimmed off
fn emit_basename_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: basename ---");
    emitter.label_global("__rt_basename");

    // ABI: rax=path_ptr, rdx=path_len, rdi=suffix_ptr, rsi=suffix_len
    // Returns: rax=result_ptr, rdx=result_len

    // -- strip trailing slashes from the path --
    emitter.label("__rt_basename_strip_x86");
    emitter.instruction("test rdx, rdx");                                       // is the slice already empty?
    emitter.instruction("jz __rt_basename_done_x86");                           // empty path: return as-is
    emitter.instruction("mov r8, rdx");                                         // copy length so we can index the last byte
    emitter.instruction("sub r8, 1");                                           // r8 = index of the last byte
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the last byte
    emitter.instruction("cmp r9b, 0x2F");                                       // is it a forward slash?
    emitter.instruction("jne __rt_basename_scan_init_x86");                     // not a slash: scan for the last separator
    emitter.instruction("sub rdx, 1");                                          // drop the trailing slash from the slice length
    emitter.instruction("jmp __rt_basename_strip_x86");                         // continue stripping further trailing slashes

    // -- scan from the end for the last remaining slash --
    emitter.label("__rt_basename_scan_init_x86");
    emitter.instruction("mov r8, rdx");                                         // start scanning from the slice end (exclusive)
    emitter.label("__rt_basename_scan_x86");
    emitter.instruction("test r8, r8");                                         // reached the start without finding a slash?
    emitter.instruction("jz __rt_basename_no_slash_x86");                       // exit the scan with no separator located
    emitter.instruction("mov r9, r8");                                          // candidate index = r8 - 1
    emitter.instruction("sub r9, 1");                                           // step the candidate index left
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load the candidate byte
    emitter.instruction("cmp r10b, 0x2F");                                      // is the candidate a separator?
    emitter.instruction("je __rt_basename_slash_x86");                          // found the rightmost slash, slice past it
    emitter.instruction("sub r8, 1");                                           // step left toward the start of the path
    emitter.instruction("jmp __rt_basename_scan_x86");                          // continue the right-to-left scan

    emitter.label("__rt_basename_slash_x86");
    emitter.instruction("add rax, r8");                                         // advance pointer past the trailing separator
    emitter.instruction("sub rdx, r8");                                         // length becomes everything after the separator
    emitter.instruction("jmp __rt_basename_suffix_x86");                        // proceed to the optional suffix-trim step

    emitter.label("__rt_basename_no_slash_x86");
    // -- no slash found: keep the full (already-trimmed) slice --

    // -- optionally trim a trailing suffix --
    emitter.label("__rt_basename_suffix_x86");
    emitter.instruction("test rsi, rsi");                                       // suffix length zero (no suffix supplied)?
    emitter.instruction("jz __rt_basename_done_x86");                           // no suffix: return the basename as-is
    emitter.instruction("cmp rdx, rsi");                                        // PHP keeps the result when suffix is not strictly shorter
    emitter.instruction("jle __rt_basename_done_x86");                          // suffix length >= result length: do not trim
    emitter.instruction("mov r8, rdx");                                         // r8 = start of the candidate tail in the result
    emitter.instruction("sub r8, rsi");                                         // tail offset = result_len - suffix_len
    emitter.instruction("xor r9d, r9d");                                        // suffix-comparison index = 0
    emitter.label("__rt_basename_suffix_loop_x86");
    emitter.instruction("cmp r9, rsi");                                         // walked the entire suffix?
    emitter.instruction("jge __rt_basename_suffix_match_x86");                  // every byte matched, drop the suffix
    emitter.instruction("mov r10, r8");                                         // absolute index inside the result
    emitter.instruction("add r10, r9");                                         // tail_start + suffix_index
    emitter.instruction("movzx r11d, BYTE PTR [rax + r10]");                    // load result byte
    emitter.instruction("movzx ecx, BYTE PTR [rdi + r9]");                      // load suffix byte
    emitter.instruction("cmp r11b, cl");                                        // do they match?
    emitter.instruction("jne __rt_basename_done_x86");                          // mismatch: keep the full basename
    emitter.instruction("add r9, 1");                                           // advance to the next suffix byte
    emitter.instruction("jmp __rt_basename_suffix_loop_x86");                   // keep comparing

    emitter.label("__rt_basename_suffix_match_x86");
    emitter.instruction("sub rdx, rsi");                                        // shrink the result by the matched suffix length

    emitter.label("__rt_basename_done_x86");
    emitter.instruction("ret");                                                 // return basename slice in rax/rdx
}
