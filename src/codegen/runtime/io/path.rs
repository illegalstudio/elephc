use crate::codegen::{emit::Emitter, platform::Arch};

use super::super::data::DIRNAME_LEVELS_MSG;

/// basename: extract trailing name component of a path.
/// Input:  x1/x2 = path string, x3/x4 = optional suffix string (empty = no suffix)
/// Output: x1/x2 = trailing name component (a slice of the input path)
///
/// Behaviour mirrors PHP's `basename()`:
/// - trailing `/` characters are stripped before scanning
/// - the substring after the last remaining `/` is returned
/// - if the suffix matches the tail of the result and is shorter than the result,
///   it is trimmed off
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

/// dirname: return the parent directory portion of a path.
/// Input:  x1/x2 = path string
/// Output: x1/x2 = parent directory (a slice of the input path)
///
/// Behaviour mirrors PHP's `dirname()`:
/// - if the path has no separator: returns "."
/// - if the path is "/" (or only slashes): returns "/"
/// - trailing slashes are ignored before locating the final separator
/// - the result drops the trailing slash unless the parent is the filesystem root
pub fn emit_dirname(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dirname_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: dirname ---");
    emitter.label_global("__rt_dirname");

    // -- empty path: return "." --
    emitter.instruction("cbz x2, __rt_dirname_dot");                            // empty input → "."

    // -- strip trailing slashes (but remember whether we saw any) --
    emitter.label("__rt_dirname_strip");
    emitter.instruction("cbz x2, __rt_dirname_only_slashes");                   // we consumed every byte and they were all slashes
    emitter.instruction("sub x9, x2, #1");                                      // index of the last byte
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the last byte
    emitter.instruction("cmp w10, #0x2F");                                      // is it a slash?
    emitter.instruction("b.ne __rt_dirname_scan_init");                         // no more trailing slashes, start scanning
    emitter.instruction("sub x2, x2, #1");                                      // drop the trailing slash
    emitter.instruction("b __rt_dirname_strip");                                // keep stripping

    // -- scan right-to-left for the final separator inside the trimmed slice --
    emitter.label("__rt_dirname_scan_init");
    emitter.instruction("mov x5, x2");                                          // x5 walks left from the end
    emitter.label("__rt_dirname_scan");
    emitter.instruction("cbz x5, __rt_dirname_dot");                            // reached the start with no slash → "."
    emitter.instruction("sub x9, x5, #1");                                      // candidate index
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load candidate byte
    emitter.instruction("cmp w10, #0x2F");                                      // is it a slash?
    emitter.instruction("b.eq __rt_dirname_slash");                             // found the parent boundary
    emitter.instruction("sub x5, x5, #1");                                      // step left
    emitter.instruction("b __rt_dirname_scan");                                 // continue scanning

    emitter.label("__rt_dirname_slash");
    // x5 == position immediately after the slash; the slash itself sits at x5-1.
    emitter.instruction("sub x2, x5, #1");                                      // length becomes everything before the slash
    emitter.label("__rt_dirname_strip_parent_slashes");
    emitter.instruction("cbz x2, __rt_dirname_root");                           // every preceding byte was a slash → root "/"
    emitter.instruction("sub x9, x2, #1");                                      // index of the last byte of the parent
    emitter.instruction("ldrb w10, [x1, x9]");                                  // peek at the last byte of the parent
    emitter.instruction("cmp w10, #0x2F");                                      // is it another redundant slash?
    emitter.instruction("b.ne __rt_dirname_done");                              // parent ends on a non-slash, keep it
    emitter.instruction("sub x2, x2, #1");                                      // collapse repeated slashes
    emitter.instruction("b __rt_dirname_strip_parent_slashes");                 // keep collapsing

    // -- result is the root: emit a single "/" --
    emitter.label("__rt_dirname_root");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_dirname_slash");  // load address of the literal "/"
    emitter.instruction("mov x2, #1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    // -- result is "." (no separator at all, or the path was strictly trailing slashes that resolved to nothing actionable) --
    emitter.label("__rt_dirname_dot");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_dirname_dot");    // load address of the literal "."
    emitter.instruction("mov x2, #1");                                          // length = 1
    emitter.instruction("ret");                                                 // return "."

    // -- the path was made of only slashes ("/", "//", ...) → "/" --
    emitter.label("__rt_dirname_only_slashes");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_dirname_slash");  // load address of the literal "/"
    emitter.instruction("mov x2, #1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    emitter.label("__rt_dirname_done");
    emitter.instruction("ret");                                                 // return parent-dir slice in x1/x2
}

fn emit_dirname_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dirname ---");
    emitter.label_global("__rt_dirname");

    // ABI: rax=path_ptr, rdx=path_len. Returns rax/rdx.

    emitter.instruction("test rdx, rdx");                                       // empty input?
    emitter.instruction("jz __rt_dirname_dot_x86");                             // empty path → "."

    // -- strip trailing slashes --
    emitter.label("__rt_dirname_strip_x86");
    emitter.instruction("test rdx, rdx");                                       // consumed every byte?
    emitter.instruction("jz __rt_dirname_only_slashes_x86");                    // every byte was a slash → root
    emitter.instruction("mov r8, rdx");                                         // r8 = last-byte index
    emitter.instruction("sub r8, 1");                                           // step into the slice
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // load the last byte
    emitter.instruction("cmp r9b, 0x2F");                                       // is it a slash?
    emitter.instruction("jne __rt_dirname_scan_init_x86");                      // no more trailing slashes
    emitter.instruction("sub rdx, 1");                                          // drop the trailing slash
    emitter.instruction("jmp __rt_dirname_strip_x86");                          // keep stripping

    // -- scan right-to-left for the final separator --
    emitter.label("__rt_dirname_scan_init_x86");
    emitter.instruction("mov r8, rdx");                                         // r8 walks left from the end
    emitter.label("__rt_dirname_scan_x86");
    emitter.instruction("test r8, r8");                                         // reached start with no slash?
    emitter.instruction("jz __rt_dirname_dot_x86");                             // → "."
    emitter.instruction("mov r9, r8");                                          // candidate index = r8 - 1
    emitter.instruction("sub r9, 1");                                           // step the candidate index left
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load candidate byte
    emitter.instruction("cmp r10b, 0x2F");                                      // is it a slash?
    emitter.instruction("je __rt_dirname_slash_x86");                           // found the parent boundary
    emitter.instruction("sub r8, 1");                                           // step left
    emitter.instruction("jmp __rt_dirname_scan_x86");                           // continue scanning

    emitter.label("__rt_dirname_slash_x86");
    emitter.instruction("mov rdx, r8");                                         // rdx = position right after the slash
    emitter.instruction("sub rdx, 1");                                          // drop the slash itself, keeping the parent prefix

    emitter.label("__rt_dirname_strip_parent_slashes_x86");
    emitter.instruction("test rdx, rdx");                                       // every preceding byte was a slash?
    emitter.instruction("jz __rt_dirname_root_x86");                            // → "/"
    emitter.instruction("mov r8, rdx");                                         // index of last byte of the parent
    emitter.instruction("sub r8, 1");                                           // step into the slice
    emitter.instruction("movzx r9d, BYTE PTR [rax + r8]");                      // peek at the last parent byte
    emitter.instruction("cmp r9b, 0x2F");                                       // is it another redundant slash?
    emitter.instruction("jne __rt_dirname_done_x86");                           // parent ends on a non-slash, keep it
    emitter.instruction("sub rdx, 1");                                          // collapse repeated slashes
    emitter.instruction("jmp __rt_dirname_strip_parent_slashes_x86");           // keep collapsing

    emitter.label("__rt_dirname_root_x86");
    crate::codegen::abi::emit_symbol_address(emitter, "rax", "_dirname_slash"); // result = "/"
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    emitter.label("__rt_dirname_dot_x86");
    crate::codegen::abi::emit_symbol_address(emitter, "rax", "_dirname_dot");   // result = "."
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // return "."

    emitter.label("__rt_dirname_only_slashes_x86");
    crate::codegen::abi::emit_symbol_address(emitter, "rax", "_dirname_slash"); // result = "/"
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    emitter.label("__rt_dirname_done_x86");
    emitter.instruction("ret");                                                 // return parent-dir slice in rax/rdx
}

/// dirname_levels: apply dirname() repeatedly for PHP's second argument.
/// Input:  x1/x2 = path string, x3 = levels
/// Output: x1/x2 = parent directory after `levels` applications
pub fn emit_dirname_levels(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_dirname_levels_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: dirname levels ---");
    emitter.label_global("__rt_dirname_levels");

    emitter.instruction("sub sp, sp, #32");                                     // reserve a small frame for the loop counter and return address
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across dirname calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame pointer for the loop frame
    emitter.instruction("cmp x3, #1");                                          // PHP requires dirname() levels to be at least 1
    emitter.instruction("b.lt __rt_dirname_levels_fail");                       // reject invalid dynamic levels with a fatal runtime diagnostic
    emitter.instruction("str x3, [sp, #0]");                                    // store the requested parent depth as the remaining loop count

    emitter.label("__rt_dirname_levels_loop");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the remaining dirname applications
    emitter.instruction("cmp x9, #0");                                          // have all requested levels been consumed?
    emitter.instruction("b.le __rt_dirname_levels_done");                       // zero or negative levels leave the current path unchanged
    emitter.instruction("bl __rt_dirname");                                     // replace the current path with its parent directory
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the remaining level count after dirname clobbered scratch regs
    emitter.instruction("sub x9, x9, #1");                                      // account for the dirname application just performed
    emitter.instruction("str x9, [sp, #0]");                                    // persist the decremented remaining level count
    emitter.instruction("b __rt_dirname_levels_loop");                          // continue until the requested number of levels has been applied

    emitter.label("__rt_dirname_levels_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the dirname-levels frame
    emitter.instruction("ret");                                                 // return the repeated dirname result in x1/x2

    emitter.label("__rt_dirname_levels_fail");
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_dirname_levels_msg"); // load the dirname levels fatal diagnostic text
    emitter.instruction(&format!("mov x2, #{}", DIRNAME_LEVELS_MSG.len()));     // pass the exact dirname levels diagnostic length to write()
    emitter.instruction("mov x0, #2");                                          // write dirname diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // use a failing process exit status for invalid dirname levels
    emitter.syscall(1);
}

fn emit_dirname_levels_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dirname levels ---");
    emitter.label_global("__rt_dirname_levels");

    // ABI: rax=path_ptr, rdx=path_len, rdi=levels. Returns rax/rdx.
    emitter.instruction("push rbp");                                            // preserve caller frame pointer before the loop helper uses a spill slot
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the remaining level count
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill space for the remaining level count
    emitter.instruction("cmp rdi, 1");                                          // PHP requires dirname() levels to be at least 1
    emitter.instruction("jl __rt_dirname_levels_fail_x86");                     // reject invalid dynamic levels with a fatal runtime diagnostic
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested parent depth as the remaining loop count

    emitter.label("__rt_dirname_levels_loop_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the remaining dirname applications
    emitter.instruction("test r8, r8");                                         // have all requested levels been consumed?
    emitter.instruction("jle __rt_dirname_levels_done_x86");                    // zero or negative levels leave the current path unchanged
    emitter.instruction("call __rt_dirname");                                   // replace the current path with its parent directory
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the remaining level count after dirname clobbered scratch regs
    emitter.instruction("sub r8, 1");                                           // account for the dirname application just performed
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // persist the decremented remaining level count
    emitter.instruction("jmp __rt_dirname_levels_loop_x86");                    // continue until the requested number of levels has been applied

    emitter.label("__rt_dirname_levels_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the dirname-levels spill space
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the repeated dirname result in rax/rdx

    emitter.label("__rt_dirname_levels_fail_x86");
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_dirname_levels_msg"); // load the dirname levels fatal diagnostic text
    emitter.instruction(&format!("mov edx, {}", DIRNAME_LEVELS_MSG.len()));     // pass the exact dirname levels diagnostic length to write()
    emitter.instruction("mov edi, 2");                                          // write dirname diagnostics to stderr
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 writes the diagnostic bytes
    emitter.instruction("syscall");                                             // emit the invalid dirname levels diagnostic
    emitter.instruction("mov edi, 1");                                          // use a failing process exit status for invalid dirname levels
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 exits the process
    emitter.instruction("syscall");                                             // terminate after the fatal dirname diagnostic
}
