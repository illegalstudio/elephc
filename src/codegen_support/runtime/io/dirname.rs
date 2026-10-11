//! Purpose:
//! Emits the `__rt_dirname`, `__rt_dirname_dot` runtime helper assembly for dirname.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{emit::Emitter, platform::Arch, platform::Platform};

/// Emits the `__rt_dirname` runtime helper for the current target.
///
/// Dispatches to `emit_dirname_linux_x86_64` on x86_64; emits a shared ARM64
/// implementation on all other targets (including ARM64 macOS and Linux).
///
/// # ABI (ARM64)
/// - Input: `x1` = path string pointer, `x2` = path string length
/// - Output: `x1` = parent directory pointer (slice of the input, no allocation), `x2` = parent length
///
/// # ABI (x86_64)
/// - Input: `rax` = path string pointer, `rdx` = path string length
/// - Output: `rax` = parent directory pointer (slice of the input, no allocation), `rdx` = parent length
///
/// # Behaviour mirrors PHP's `dirname()`:
/// - empty path → "."
/// - path with no separator → "."
/// - path is "/" or only slashes → "/"
/// - trailing slashes are stripped before locating the final separator
/// - result drops the trailing slash unless the parent is the filesystem root
pub fn emit_dirname(emitter: &mut Emitter) {
    if emitter.platform == Platform::Windows {
        emit_dirname_windows_x86_64(emitter);
        return;
    }
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
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_dirname_slash");  // load address of the literal "/"
    emitter.instruction("mov x2, #1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    // -- result is "." (no separator at all, or the path was strictly trailing slashes that resolved to nothing actionable) --
    emitter.label("__rt_dirname_dot");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_dirname_dot");    // load address of the literal "."
    emitter.instruction("mov x2, #1");                                          // length = 1
    emitter.instruction("ret");                                                 // return "."

    // -- the path was made of only slashes ("/", "//", ...) → "/" --
    emitter.label("__rt_dirname_only_slashes");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_dirname_slash");  // load address of the literal "/"
    emitter.instruction("mov x2, #1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    emitter.label("__rt_dirname_done");
    emitter.instruction("ret");                                                 // return parent-dir slice in x1/x2
}

/// Emits the x86_64 Linux implementation of `__rt_dirname`.
///
/// # ABI
/// - Input: `rax` = path string pointer, `rdx` = path string length
/// - Output: `rax` = parent directory pointer (slice of the input, no allocation), `rdx` = parent length
///
/// # Behaviour (mirrors PHP's `dirname()`):
/// - empty path → "."
/// - no separator → "."
/// - "/" or only slashes → "/"
/// - trailing slashes stripped before scanning for the final separator
/// - parent drops trailing slash unless it is the filesystem root
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
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_dirname_slash"); // result = "/"
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    emitter.label("__rt_dirname_dot_x86");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_dirname_dot");   // result = "."
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // return "."

    emitter.label("__rt_dirname_only_slashes_x86");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_dirname_slash"); // result = "/"
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // return root slash

    emitter.label("__rt_dirname_done_x86");
    emitter.instruction("ret");                                                 // return parent-dir slice in rax/rdx
}

/// Emits the Windows x86_64 implementation of `__rt_dirname`.
///
/// Windows is an x86_64-only target, so this replaces the POSIX emitter outright
/// rather than threading platform tests through it. It follows php-src
/// `php_win32_ioutil_dirname` (win32/ioutil.c):
///
/// 1. an `<alpha>:` drive prefix is skipped and never scanned, so the result keeps
///    it; a bare `"C:"` is returned unchanged;
/// 2. both `/` and `\` are separators (`PHP_WIN32_IOUTIL_IS_SLASHW`, mirroring
///    `IS_SLASH` in Zend/zend_virtual_cwd.h);
/// 3. trailing separators are stripped, then the filename, then the separators
///    before it;
/// 4. running out of bytes yields the root, and finding no separator yields `"."`.
///
/// # ABI
/// - Input: `rax` = path pointer, `rdx` = path length
/// - Output: `rax` = parent pointer (a slice of the input), `rdx` = parent length
///
/// # Known divergence
/// php-src rewrites the root case to `PHP_WIN32_IOUTIL_DEFAULT_SLASH`, so it
/// normalises `dirname("C:/x")` to `"C:\"`. This returns the input's own bytes,
/// `"C:/"`, because the helper is contractually allocation-free and hands back a
/// slice. The separator is preserved rather than rewritten; every other result is
/// byte-identical to PHP.
fn emit_dirname_windows_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dirname (windows) ---");
    emitter.label_global("__rt_dirname");

    // ABI: rax=path_ptr, rdx=path_len. Returns rax/rdx.
    // r11 holds the drive-prefix length: the scan never steps below it, which is
    // what keeps "C:\x" from collapsing past the drive into "C".

    emitter.instruction("test rdx, rdx");                                       // empty input?
    emitter.instruction("jz __rt_dirname_dot_win");                             // empty path → "."
    emitter.instruction("xor r11d, r11d");                                      // assume no drive prefix
    emitter.instruction("cmp rdx, 2");                                          // room for an "<alpha>:" prefix?
    emitter.instruction("jb __rt_dirname_scan_init_win");                       // too short to carry a drive letter
    emitter.instruction("movzx r9d, BYTE PTR [rax + 1]");                       // second byte of the path
    emitter.instruction("cmp r9b, 0x3A");                                       // is it a colon?
    emitter.instruction("jne __rt_dirname_scan_init_win");                      // not a drive-qualified path
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // first byte of the path
    emitter.instruction("or r9b, 0x20");                                        // fold the drive letter to lower case
    emitter.instruction("cmp r9b, 0x61");                                       // below 'a'?
    emitter.instruction("jb __rt_dirname_scan_init_win");                       // not a letter, so not a drive
    emitter.instruction("cmp r9b, 0x7A");                                       // above 'z'?
    emitter.instruction("ja __rt_dirname_scan_init_win");                       // not a letter, so not a drive
    emitter.instruction("mov r11d, 2");                                         // the drive prefix is off-limits to the scan
    emitter.instruction("cmp rdx, 2");                                          // is the path exactly "C:"?
    emitter.instruction("je __rt_dirname_done_win");                            // php returns the drive unchanged

    // -- strip trailing separators --
    emitter.label("__rt_dirname_scan_init_win");
    emitter.instruction("mov r8, rdx");                                         // r8 = exclusive end index of the live slice
    emitter.label("__rt_dirname_strip_win");
    emitter.instruction("cmp r8, r11");                                         // consumed everything after the drive?
    emitter.instruction("jbe __rt_dirname_root_win");                           // only separators remained → root
    emitter.instruction("mov r9, r8");                                          // index of the last byte
    emitter.instruction("sub r9, 1");                                           // step into the slice
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load the last byte
    emitter.instruction("cmp r10b, 0x2F");                                      // forward-slash separator?
    emitter.instruction("je __rt_dirname_strip_step_win");                      // drop it
    emitter.instruction("cmp r10b, 0x5C");                                      // backslash separator?
    emitter.instruction("jne __rt_dirname_name_win");                           // neither: the filename starts here
    emitter.label("__rt_dirname_strip_step_win");
    emitter.instruction("mov r8, r9");                                          // shrink past the trailing separator
    emitter.instruction("jmp __rt_dirname_strip_win");                          // keep stripping

    // -- strip the filename --
    emitter.label("__rt_dirname_name_win");
    emitter.instruction("cmp r8, r11");                                         // reached the drive with no separator?
    emitter.instruction("jbe __rt_dirname_dot_win");                            // → "."
    emitter.instruction("mov r9, r8");                                          // candidate index
    emitter.instruction("sub r9, 1");                                           // step the candidate left
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load the candidate byte
    emitter.instruction("cmp r10b, 0x2F");                                      // forward-slash separator?
    emitter.instruction("je __rt_dirname_parent_win");                          // found the parent boundary
    emitter.instruction("cmp r10b, 0x5C");                                      // backslash separator?
    emitter.instruction("je __rt_dirname_parent_win");                          // found the parent boundary
    emitter.instruction("mov r8, r9");                                          // step left over the filename
    emitter.instruction("jmp __rt_dirname_name_win");                           // keep scanning

    // -- strip the separators that preceded the filename --
    emitter.label("__rt_dirname_parent_win");
    emitter.instruction("mov r8, r9");                                          // r8 = index of the separator itself
    emitter.label("__rt_dirname_parent_loop_win");
    emitter.instruction("cmp r8, r11");                                         // nothing but separators before it?
    emitter.instruction("jbe __rt_dirname_root_win");                           // → root
    emitter.instruction("mov r9, r8");                                          // index of the preceding byte
    emitter.instruction("sub r9, 1");                                           // step into the slice
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load it
    emitter.instruction("cmp r10b, 0x2F");                                      // another forward slash?
    emitter.instruction("je __rt_dirname_parent_step_win");                     // collapse it
    emitter.instruction("cmp r10b, 0x5C");                                      // another backslash?
    emitter.instruction("jne __rt_dirname_finish_win");                         // parent ends on a real byte
    emitter.label("__rt_dirname_parent_step_win");
    emitter.instruction("mov r8, r9");                                          // collapse the repeated separator
    emitter.instruction("jmp __rt_dirname_parent_loop_win");                    // keep collapsing

    emitter.label("__rt_dirname_finish_win");
    emitter.instruction("mov rdx, r8");                                         // the parent is the prefix up to r8
    emitter.instruction("ret");                                                 // return the parent slice

    // -- the parent is the root: "<drive>" plus one separator, or a bare "\" --
    emitter.label("__rt_dirname_root_win");
    emitter.instruction("test r11, r11");                                       // was there a drive prefix?
    emitter.instruction("jz __rt_dirname_root_literal_win");                    // no drive: return the separator literal
    emitter.instruction("mov rdx, 3");                                          // keep "<drive>:" plus the separator byte
    emitter.instruction("ret");                                                 // e.g. "C:\" for "C:\file"
    emitter.label("__rt_dirname_root_literal_win");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_dirname_backslash"); // result = "\"
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // php normalises a rooted path to DEFAULT_SLASH

    emitter.label("__rt_dirname_dot_win");
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_dirname_dot");       // result = "."
    emitter.instruction("mov rdx, 1");                                          // length = 1
    emitter.instruction("ret");                                                 // return "."

    emitter.label("__rt_dirname_done_win");
    emitter.instruction("ret");                                                 // return the path unchanged (bare drive)
}
