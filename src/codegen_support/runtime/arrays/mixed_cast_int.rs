//! Purpose:
//! Emits the `__rt_mixed_cast_int`, `__rt_mixed_unbox` runtime helper assembly for mixed cast int.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};

/// Emits the `__rt_mixed_cast_int` runtime helper for casting a boxed Mixed cell to int.
///
/// Dispatches to the x86_64 variant when targeting Linux on x86_64; otherwise emits the
/// ARM64 variant. The ARM64 path uses `__rt_mixed_unbox` to extract the tag (x0) and
/// payload words (x1, x2), then switches on the tag to apply PHP's scalar cast rules:
/// int → direct forward, string → `__rt_str_to_int`, float → truncate-to-zero, bool → 0/1 payload,
/// array/resource → element count or display id, null/unsupported → 0.
///
/// # Input
/// - ARM64: x0 holds the boxed mixed pointer on entry
/// - x86_64: rax holds the boxed mixed pointer on entry
///
/// # Output
/// - ARM64: integer result returned in x0
/// - x86_64: integer result returned in rax
/// Emits `__rt_mixed_cast_int_nullable`, which answers `NULL_SENTINEL` for a boxed null and
/// otherwise defers to `__rt_mixed_cast_int`.
///
/// php's `?int` parameters treat null as "no bound", which is never the same answer as the `0`
/// that `__rt_mixed_cast_int` produces for a null payload: `fgets($h, null)` reads the whole
/// line where a `0` bound raised `Argument #2 ($length) must be greater than 0`, and
/// `fwrite($h, $data, null)` writes every byte where a `0` cap writes none. A caller that binds
/// its `?int` argument from a `mixed` value — an untyped function parameter forwarding to the
/// builtin is the usual way — needs the two apart, so this helper keeps null distinguishable
/// instead of flattening it into a legitimate zero.
///
/// # Input / Output
/// Same registers as `__rt_mixed_cast_int`: the boxed pointer in x0/rax, the integer out in x0/rax.
pub fn emit_mixed_cast_int_nullable(emitter: &mut Emitter) {
    let sentinel = crate::codegen_support::sentinels::NULL_SENTINEL;
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_int_nullable ---");
    emitter.label_global("__rt_mixed_cast_int_nullable");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame for the tag peek
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper stack frame
            emitter.instruction("str x0, [sp, #0]");                            // keep the boxed pointer across the peek
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // x0 = runtime tag
            emitter.instruction("cmp x0, #8");                                  // runtime tag 8 = null
            emitter.instruction("b.eq __rt_mcin_null");                         // null is php's "no bound"
            emitter.instruction("ldr x0, [sp, #0]");                            // restore the boxed pointer
            abi::emit_call_label(emitter, "__rt_mixed_cast_int");               // every other payload casts as usual
            emitter.instruction("b __rt_mcin_done");
            emitter.label("__rt_mcin_null");
            abi::emit_load_int_immediate(emitter, "x0", sentinel);              // the caller's "unbounded" marker
            emitter.label("__rt_mcin_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the helper frame
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish a stable frame pointer
            emitter.instruction("sub rsp, 16");                                 // one aligned slot, keeping SysV alignment
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // keep the boxed pointer across the peek
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // rax = runtime tag
            emitter.instruction("cmp rax, 8");                                  // runtime tag 8 = null
            emitter.instruction("je __rt_mcin_null_x86");                       // null is php's "no bound"
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // restore the boxed pointer
            abi::emit_call_label(emitter, "__rt_mixed_cast_int");               // every other payload casts as usual
            emitter.instruction("jmp __rt_mcin_done_x86");
            emitter.label("__rt_mcin_null_x86");
            abi::emit_load_int_immediate(emitter, "rax", sentinel);             // the caller's "unbounded" marker
            emitter.label("__rt_mcin_done_x86");
            emitter.instruction("leave");                                       // restore rbp + rsp
            emitter.instruction("ret");
        }
    }
}

pub fn emit_mixed_cast_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_int ---");
    emitter.label_global("__rt_mixed_cast_int");

    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for nested helper calls
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper stack frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=value_lo, x2=value_hi for the boxed payload
    emitter.instruction("cmp x0, #0");                                          // does the mixed payload already hold an int?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_int");                   // ints reuse their stored payload directly
    emitter.instruction("cmp x0, #1");                                          // does the mixed payload hold a string?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_string");                // strings cast through the PHP string-to-int helper
    emitter.instruction("cmp x0, #2");                                          // does the mixed payload hold a float?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_float");                 // floats cast by truncating toward zero
    emitter.instruction("cmp x0, #3");                                          // does the mixed payload hold a bool?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_bool");                  // bools reuse their 0/1 payload directly
    emitter.instruction("cmp x0, #4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_array");                 // arrays cast to their current element count
    emitter.instruction("cmp x0, #5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_array");                 // hashes cast to their current element count
    emitter.instruction("cmp x0, #9");                                          // does the mixed payload hold a resource?
    emitter.instruction("b.eq __rt_mixed_cast_int_from_resource");              // resources cast to their display id
    emitter.instruction("mov x0, #0");                                          // null and unsupported payloads cast to zero for now
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the normalized integer result

    emitter.label("__rt_mixed_cast_int_from_int");
    emitter.instruction("mov x0, x1");                                          // forward the stored integer payload directly
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the unboxed integer payload

    emitter.label("__rt_mixed_cast_int_from_string");
    emitter.instruction("bl __rt_str_to_int");                                  // parse the unboxed string payload through PHP string-to-int cast rules
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the parsed integer result

    emitter.label("__rt_mixed_cast_int_from_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the FP register file
    abi::emit_php_float_to_int(emitter, "x0");                                  // apply PHP float->int rules to the unboxed float payload
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the converted integer result

    emitter.label("__rt_mixed_cast_int_from_bool");
    emitter.instruction("mov x0, x1");                                          // bool payloads are already normalized to 0 or 1
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the bool-as-int result

    emitter.label("__rt_mixed_cast_int_from_array");
    emitter.instruction("cbz x1, __rt_mixed_cast_int_zero");                    // null container pointers cast like empty containers
    emitter.instruction("ldr x0, [x1]");                                        // load the current container element count from the header
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the container size as the cast result

    emitter.label("__rt_mixed_cast_int_zero");
    emitter.instruction("mov x0, #0");                                          // null containers cast to zero
    emitter.instruction("b __rt_mixed_cast_int_done");                          // return the null-container cast result

    emitter.label("__rt_mixed_cast_int_from_resource");
    emitter.instruction("mov x0, x1");                                          // move the native resource payload into the registry argument
    emitter.instruction("bl __rt_resource_id_of");                              // PHP casts a resource to its RESOURCE ID, not to its native payload

    emitter.label("__rt_mixed_cast_int_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the integer cast result in x0
}

/// Emits the x86_64 Linux variant of `__rt_mixed_cast_int`.
///
/// Uses the System V AMD64 ABI: unbox via `__rt_mixed_unbox` (returns tag in rax,
/// payload in rdi/rdx), then dispatches on tag using je jumps to type-specific handlers.
/// Results are returned in rax.
///
/// # ABI
/// - Input: rax = boxed mixed pointer
/// - Output: rax = integer result
/// - Clobbers: rax, rdi, rdx, xmm0, rsp; preserves rbp
fn emit_mixed_cast_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_int ---");
    emitter.label_global("__rt_mixed_cast_int");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before this helper allocates its own frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame pointer for the helper body
    emitter.instruction("sub rsp, 16");                                         // reserve one aligned temporary slot so nested helper calls keep the SysV stack aligned
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // return the mixed runtime tag in rax and payload words in rdi/rdx for the boxed value
    emitter.instruction("cmp rax, 0");                                          // does the mixed payload already hold an int?
    emitter.instruction("je __rt_mixed_cast_int_from_int_linux_x86_64");        // ints reuse their stored payload directly
    emitter.instruction("cmp rax, 1");                                          // does the mixed payload hold a string?
    emitter.instruction("je __rt_mixed_cast_int_from_string_linux_x86_64");     // strings cast through the PHP string-to-int helper
    emitter.instruction("cmp rax, 2");                                          // does the mixed payload hold a float?
    emitter.instruction("je __rt_mixed_cast_int_from_float_linux_x86_64");      // floats cast by truncating toward zero
    emitter.instruction("cmp rax, 3");                                          // does the mixed payload hold a bool?
    emitter.instruction("je __rt_mixed_cast_int_from_bool_linux_x86_64");       // bools reuse their 0/1 payload directly
    emitter.instruction("cmp rax, 4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("je __rt_mixed_cast_int_from_array_linux_x86_64");      // arrays cast to their current element count
    emitter.instruction("cmp rax, 5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("je __rt_mixed_cast_int_from_array_linux_x86_64");      // hashes cast to their current element count
    emitter.instruction("cmp rax, 9");                                          // does the mixed payload hold a resource?
    emitter.instruction("je __rt_mixed_cast_int_from_resource_linux_x86_64");   // resources cast to their display id
    emitter.instruction("mov rax, 0");                                          // null and unsupported payloads cast to zero for now
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the normalized integer result

    emitter.label("__rt_mixed_cast_int_from_int_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // forward the stored integer payload directly
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the unboxed integer payload

    emitter.label("__rt_mixed_cast_int_from_string_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the standard x86_64 string result register
    abi::emit_call_label(emitter, "__rt_str_to_int");                           // parse the unboxed string payload through PHP string-to-int cast rules
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the parsed integer result

    emitter.label("__rt_mixed_cast_int_from_float_linux_x86_64");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float bits into the floating-point result register
    abi::emit_php_float_to_int(emitter, "rax");                                 // apply PHP float->int rules to the unboxed float payload
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the converted integer result

    emitter.label("__rt_mixed_cast_int_from_bool_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // bool payloads are already normalized to 0 or 1
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the bool-as-int result

    emitter.label("__rt_mixed_cast_int_from_array_linux_x86_64");
    emitter.instruction("test rdi, rdi");                                       // null container pointers cast like empty containers
    emitter.instruction("je __rt_mixed_cast_int_zero_linux_x86_64");            // skip the header load when the container pointer is null
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the current container element count from the header
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the container size as the cast result

    emitter.label("__rt_mixed_cast_int_zero_linux_x86_64");
    emitter.instruction("mov rax, 0");                                          // null containers cast to zero
    emitter.instruction("jmp __rt_mixed_cast_int_done_linux_x86_64");           // return the null-container cast result

    emitter.label("__rt_mixed_cast_int_from_resource_linux_x86_64");
    emitter.instruction("mov rax, rdi");                                        // move the native resource payload into the registry argument
    abi::emit_call_label(emitter, "__rt_resource_id_of");                       // PHP casts a resource to its RESOURCE ID, not to its native payload

    emitter.label("__rt_mixed_cast_int_done_linux_x86_64");
    emitter.instruction("add rsp, 16");                                         // release the aligned temporary slot reserved for nested helper calls
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the integer cast result in rax
}
