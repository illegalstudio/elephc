//! Purpose:
//! Emits the `__rt_array_new`, `__rt_heap_alloc` runtime helper assembly for array new.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.
//! - `capacity * elem_size` is validated before the allocation request: an unchecked product
//!   wraps to a tiny allocation while the header keeps the pre-overflow capacity, so every
//!   caller's fill loop would then run outside the block. Overflow is a fatal error.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::ARRAY_ALLOC_SIZE_MSG;


/// Emits the `__rt_array_new` runtime helper for array allocation.
///
/// Dispatches to the target-specific implementation (x86_64 or ARM64).
///
/// # Inputs (ARM64 calling convention)
/// - `x0`: requested capacity (number of elements)
/// - `x1`: element size in bytes (8 for scalar arrays, 16 for string arrays)
///
/// # Output
/// - `x0`: pointer to the allocated array header on ARM64, `rax` on x86_64
///
/// # Memory layout
/// `[length:8][capacity:8][elem_size:8][elements...]`
///
/// The kind word at `header - 8` encodes the array variant (indexed array, copy-on-write flag, string-array layout hint).
///
/// # Size validation
/// Negative capacities are clamped to an empty payload region (callers' fill loops use signed
/// comparisons and write nothing), and any `capacity * elem_size + 24` that does not fit in a
/// non-negative machine word terminates the process through `__rt_array_cap_overflow`.
pub fn emit_array_new(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_new_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_new ---");
    emitter.label_global("__rt_array_new");

    // -- validate the requested allocation size before touching the heap --
    emitter.instruction("cmp x0, #0");                                          // is the requested capacity negative?
    emitter.instruction("csel x9, x0, xzr, ge");                                // clamp negative capacities to an empty payload region
    emitter.instruction("umulh x10, x9, x1");                                   // x10 = high 64 bits of capacity * elem_size
    emitter.instruction("cbnz x10, __rt_array_cap_overflow");                   // reject payload sizes that do not fit in one machine word
    emitter.instruction("mul x9, x9, x1");                                      // x9 = low 64 bits of capacity * elem_size
    emitter.instruction("adds x9, x9, #24");                                    // x9 = payload size plus the 24-byte array header
    emitter.instruction("b.hs __rt_array_cap_overflow");                        // reject totals that carried out of the machine word
    emitter.instruction("tbnz x9, #63, __rt_array_cap_overflow");               // reject totals the signed heap-size check would read as negative

    // -- set up stack frame, save arguments for use after heap_alloc call --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save capacity to stack (need it after bl)
    emitter.instruction("str x1, [sp, #8]");                                    // save elem_size to stack (need it after bl)
    emitter.instruction("str xzr, [sp, #16]");                                  // keep a reserved scratch slot for future array metadata helpers

    // -- allocate the validated total: 24-byte header + (capacity * elem_size) --
    emitter.instruction("mov x0, x9");                                          // x0 = validated data size + 24-byte header
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate memory, x0 = pointer to array
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload elem_size for the default packed metadata choice
    emitter.instruction("cmp x9, #16");                                         // does the array store 16-byte string payloads?
    emitter.instruction("cset x9, eq");                                         // 16-byte arrays default to string value_type tag 1, others to 0
    emitter.instruction("lsl x9, x9, #8");                                      // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("mov x10, #0x8000");                                    // bit 15 marks heap containers that participate in copy-on-write
    emitter.instruction("orr x9, x9, x10");                                     // preserve the persistent copy-on-write container flag in the kind word
    emitter.instruction("add x9, x9, #2");                                      // low byte 2 = indexed array heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // store the packed indexed-array kind word in the heap header

    // -- initialize the array header fields --
    emitter.instruction("str xzr, [x0]");                                       // header[0]: length = 0 (array starts empty)
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload capacity from stack
    emitter.instruction("str x9, [x0, #8]");                                    // header[8]: capacity = original x0 arg
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload elem_size from stack
    emitter.instruction("str x9, [x0, #16]");                                   // header[16]: elem_size = original x1 arg

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = array pointer

    // -- fatal error: requested array size cannot be represented --
    emitter.label("__rt_array_cap_overflow");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    abi::emit_symbol_address(emitter, "x1", "_arr_cap_err_msg");
    emitter.instruction(&format!("mov x2, #{}", ARRAY_ALLOC_SIZE_MSG.len()));   // pass the exact array-size diagnostic byte count
    emitter.syscall(4);
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.syscall(1);
}

/// Emits the x86_64 Linux implementation of `__rt_array_new`.
///
/// Uses the System V AMD64 ABI: arguments in `rdi` (capacity) and `rsi` (element size),
/// result returned in `rax`.
///
/// # Inputs
/// - `rdi`: requested capacity (number of elements)
/// - `rsi`: element size in bytes (8 for scalar arrays, 16 for string arrays)
///
/// # Output
/// - `rax`: pointer to the allocated array header
///
/// # Memory layout
/// `[length:8][capacity:8][elem_size:8][elements...]`
///
/// The kind word at `header - 8` includes the x86_64 heap magic marker (`0x454C5048_XXXX_XXXX`),
/// the copy-on-write flag (`0x8000`), and the indexed-array kind tag (`2`).
///
/// # Size validation
/// Mirrors the ARM64 guard: negative capacities are clamped to an empty payload region and any
/// `capacity * elem_size + 24` that does not fit in a non-negative machine word terminates the
/// process through `__rt_array_cap_overflow`.
fn emit_array_new_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_new ---");
    emitter.label_global("__rt_array_new");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving local scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for capacity and element-size bookkeeping
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for capacity and element size across the heap allocation helper call
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save capacity across the heap allocation helper call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save element size across the heap allocation helper call

    // -- validate the requested allocation size before touching the heap --
    emitter.instruction("xor rax, rax");                                        // default the sizing operand to an empty payload region
    emitter.instruction("test rdi, rdi");                                       // is the requested capacity strictly positive?
    emitter.instruction("cmovg rax, rdi");                                      // clamp negative capacities to an empty payload region
    emitter.instruction("imul rax, rsi");                                       // rax = capacity * elem_size, overflow flag set when the product does not fit
    emitter.instruction("jo __rt_array_cap_overflow");                          // reject payload sizes that do not fit in one machine word
    emitter.instruction("add rax, 24");                                         // include the fixed 24-byte array header in the owned heap allocation size
    emitter.instruction("jo __rt_array_cap_overflow");                          // reject totals the signed heap-size accounting would read as negative
    emitter.instruction("call __rt_heap_alloc");                                // allocate the array backing storage through the shared x86_64 heap wrapper
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the element size so the array kind word can encode the string-value layout hint
    emitter.instruction("cmp r10, 16");                                         // detect string arrays that use 16-byte payload slots
    emitter.instruction("sete r11b");                                           // materialize the runtime value-type tag for the string-array layout choice
    emitter.instruction("movzx r11, r11b");                                     // widen the one-byte string-array flag into a full integer register
    emitter.instruction("shl r11, 8");                                          // move the runtime value-type tag into the packed heap-kind byte lane
    emitter.instruction("or r11, 0x8000");                                      // mark the array allocation as a copy-on-write heap container
    emitter.instruction("add r11, 2");                                          // low byte 2 identifies the payload as an indexed array heap object
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(0))); // materialize the x86_64 heap marker in a scratch register before combining it with the packed array kind bits
    emitter.instruction("or r11, r10");                                         // add the x86_64 heap marker while preserving the packed array kind bits
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // stamp the allocated payload as an indexed array in the uniform heap header
    emitter.instruction("mov QWORD PTR [rax], 0");                              // header[0]: length = 0 (array starts empty)
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload capacity after heap_alloc clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // header[8]: capacity = original capacity argument
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload elem_size after heap_alloc clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // header[16]: elem_size = original element-size argument
    emitter.instruction("add rsp, 16");                                         // release the temporary capacity and element-size spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the array pointer in rax

    // -- fatal error: requested array size cannot be represented --
    emitter.label("__rt_array_cap_overflow");
    if emitter.phpext_output { abi::emit_cdylib_exit_escape(emitter); }
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the array-size fatal error message
    abi::emit_symbol_address(emitter, "rsi", "_arr_cap_err_msg");
    emitter.instruction(&format!("mov edx, {}", ARRAY_ALLOC_SIZE_MSG.len()));   // pass the exact array-size diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // print the fatal array-size message to stderr
    abi::emit_cdylib_exit_escape(emitter);
    emitter.instruction("mov edi, 1");                                          // exit code 1 for an unrepresentable array size
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the array-size failure
}
