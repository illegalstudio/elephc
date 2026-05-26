//! Purpose:
//! Emits the `__rt_dirname`, `__rt_dirname_dot` runtime helper assembly for dirname.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};

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
