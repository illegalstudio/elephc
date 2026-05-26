//! Purpose:
//! Emits the `__rt_iterable_write_stdout`, `__rt_heap_kind` runtime helper assembly for iterable write stdout.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Iterable helpers dispatch on runtime kind tags and must report unsupported shapes without corrupting iteration state.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_iterable_write_stdout` runtime helper.
///
/// Dispatches to the x86_64 Linux variant on that target; on ARM64 emits directly.
/// Writes the literal `"Array"` to stdout for indexed-array (kind 2) and hash-table
/// (kind 3) iterable payloads. Object payloads (kind 4) and all other heap kinds are
/// silently skipped, matching PHP's `echo $array;` behavior.
///
/// Input: x0/rax = iterable heap pointer.
/// Output: writes `"Array"` to stdout for array-like iterables; no output otherwise.
/// Preserves: x29, x30 on ARM64; rbp on x86_64 are saved/restored across the helper call.
pub fn emit_iterable_write_stdout(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_iterable_write_stdout_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: iterable_write_stdout ---");
    emitter.label_global("__rt_iterable_write_stdout");

    // -- save the link register so we can call __rt_heap_kind without losing the return target --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve frame and link registers across the helper call
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the helper

    emitter.instruction("bl __rt_heap_kind");                                   // x0 = heap kind tag for the iterable payload
    emitter.instruction("cmp x0, #2");                                          // is the iterable backed by an indexed array?
    emitter.instruction("b.eq __rt_iterable_write_stdout_array");               // indexed arrays print as the literal \"Array\"
    emitter.instruction("cmp x0, #3");                                          // is the iterable backed by a hash table?
    emitter.instruction("b.eq __rt_iterable_write_stdout_array");               // hash tables also print as the literal \"Array\"
    emitter.instruction("b __rt_iterable_write_stdout_done");                   // every other heap kind is a silent no-op

    emitter.label("__rt_iterable_write_stdout_array");
    emitter.adrp("x1", "_iterable_array_str");                                  // load the page that contains the literal "Array" bytes
    emitter.add_lo12("x1", "x1", "_iterable_array_str");                        // resolve the literal "Array" address within that page
    emitter.instruction("mov x2, #5");                                          // pass the 5-byte length of the literal "Array" to write()
    emitter.instruction("mov x0, #1");                                          // fd = stdout
    emitter.syscall(4);

    emitter.label("__rt_iterable_write_stdout_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the saved frame and link registers
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 variant of `__rt_iterable_write_stdout`.
///
/// Saves and restores rbp as the frame pointer. Identical dispatch logic to the
/// ARM64 variant: writes `"Array"` for heap kinds 2 and 3, silent no-op otherwise.
///
/// Input: rax = iterable heap pointer.
/// Output: writes `"Array"` to stdout for array-like iterables; no output otherwise.
fn emit_iterable_write_stdout_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: iterable_write_stdout ---");
    emitter.label_global("__rt_iterable_write_stdout");

    // -- preserve the return address so we can call __rt_heap_kind without losing it --
    emitter.instruction("push rbp");                                            // align the SysV stack frame on entry
    emitter.instruction("mov rbp, rsp");                                        // establish a frame pointer for the helper

    emitter.instruction("call __rt_heap_kind");                                 // rax = heap kind tag for the iterable payload
    emitter.instruction("cmp rax, 2");                                          // is the iterable backed by an indexed array?
    emitter.instruction("je __rt_iterable_write_stdout_array");                 // indexed arrays print as the literal \"Array\"
    emitter.instruction("cmp rax, 3");                                          // is the iterable backed by a hash table?
    emitter.instruction("je __rt_iterable_write_stdout_array");                 // hash tables also print as the literal \"Array\"
    emitter.instruction("jmp __rt_iterable_write_stdout_done");                 // every other heap kind is a silent no-op

    emitter.label("__rt_iterable_write_stdout_array");
    abi::emit_symbol_address(emitter, "rsi", "_iterable_array_str");            // point the Linux write() buffer register at the literal "Array" bytes
    emitter.instruction("mov edx, 5");                                          // pass the 5-byte length of the literal "Array" to write()
    emitter.instruction("mov edi, 1");                                          // fd = stdout
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the literal "Array" to stdout

    emitter.label("__rt_iterable_write_stdout_done");
    emitter.instruction("pop rbp");                                             // restore the prior frame pointer before returning
    emitter.instruction("ret");                                                 // return to the caller
}
