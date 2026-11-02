//! Purpose:
//! Emits the `__rt_array_from_mixed` runtime dispatcher for the PHP `(array)` cast.
//! Inspects the boxed source tag and builds the matching boxed-Mixed indexed array.
//!
//! Called from:
//! - `crate::codegen_support::runtime::arrays` via the top-level runtime emitter.
//! - The EIR `Cast` lowering to `Heap(Array)`, which boxes any non-array source first.
//!
//! Key details:
//! - The cast result is typed `array<mixed>`, so every produced array uses boxed-Mixed
//!   (value_type 7) slots: null/empty -> empty array; an indexed-array payload is rebuilt
//!   element-by-element into a fresh owned boxed-Mixed array (so reads unbox correctly);
//!   a scalar (tags 0-3) wraps in a single-element `[value]` array.
//! - Associative array (tag 5) and object (tag 6) sources are string-keyed in PHP and
//!   cannot be represented in the int-indexed result type, so they fatal instead of
//!   silently dropping keys. The boxed Mixed argument is only borrowed; the result is an
//!   independently owned array.

use crate::codegen::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen::platform::Arch;

/// Length in bytes of the `_array_cast_unsupported_msg` runtime fatal string.
const ARRAY_CAST_UNSUPPORTED_MSG_LEN: usize = 72;

/// Emits `__rt_array_from_mixed(mixed_ptr) -> array_ptr` for the active target.
///
/// Dispatches on the boxed runtime tag to produce an owned boxed-Mixed indexed array:
/// PHP null becomes an empty array, an indexed-array payload is rebuilt into a fresh
/// boxed-Mixed array, and scalars wrap in a single-element `[value]` array. Associative
/// array and object payloads fatal because their string keys cannot fit the int-indexed
/// result type. The boxed Mixed argument is only borrowed.
pub fn emit_array_from_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_from_mixed_x86_64(emitter);
        return;
    }
    emit_array_from_mixed_aarch64(emitter);
}

/// ARM64 implementation of `__rt_array_from_mixed` (input `x0`, result `x0`).
fn emit_array_from_mixed_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_from_mixed ---");
    emitter.label_global("__rt_array_from_mixed");

    // Frame: [sp,#0]=mixed [sp,#8]=dest [sp,#16]=index [sp,#24]=len
    //        [sp,#32]=cell [sp,#48]=saved fp/lr
    emitter.instruction("sub sp, sp, #64");                                     // reserve the dispatch/build frame and saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed Mixed pointer
    emitter.instruction("cbz x0, __rt_array_from_mixed_null");                  // a null pointer yields an empty array
    emitter.instruction("ldr x9, [x0]");                                        // load the runtime tag from mixed[0]
    emitter.instruction("cmp x9, #8");                                          // is the payload PHP null?
    emitter.instruction("b.eq __rt_array_from_mixed_null");                     // null casts to an empty array
    emitter.instruction("cmp x9, #4");                                          // is the payload an indexed array?
    emitter.instruction("b.eq __rt_array_from_mixed_array");                    // indexed arrays rebuild into a boxed-Mixed array
    emitter.instruction("cmp x9, #0");                                          // is the payload an int scalar?
    emitter.instruction("b.eq __rt_array_from_mixed_scalar");                   // scalars wrap in a single-element array
    emitter.instruction("cmp x9, #1");                                          // is the payload a string scalar?
    emitter.instruction("b.eq __rt_array_from_mixed_scalar");                   // scalars wrap in a single-element array
    emitter.instruction("cmp x9, #2");                                          // is the payload a float scalar?
    emitter.instruction("b.eq __rt_array_from_mixed_scalar");                   // scalars wrap in a single-element array
    emitter.instruction("cmp x9, #3");                                          // is the payload a bool scalar?
    emitter.instruction("b.eq __rt_array_from_mixed_scalar");                   // scalars wrap in a single-element array
    emitter.instruction("b __rt_array_from_mixed_unsupported");                 // assoc array / object / other -> fatal

    emitter.label("__rt_array_from_mixed_null");
    emitter.instruction("mov x0, #0");                                          // capacity 0 for the empty result array
    emitter.instruction("mov x1, #8");                                          // 8-byte slots for boxed Mixed elements
    emitter.instruction("bl __rt_array_new");                                   // allocate an empty indexed array
    emitter.instruction("b __rt_array_from_mixed_ret");                         // return the empty array

    emitter.label("__rt_array_from_mixed_scalar");
    emitter.instruction("mov x0, #1");                                          // capacity 1 for the single scalar element
    emitter.instruction("mov x1, #8");                                          // 8-byte slots for boxed Mixed elements
    emitter.instruction("bl __rt_array_new");                                   // allocate the one-element indexed array
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = boxed scalar Mixed cell (push child)
    emitter.instruction("bl __rt_array_push_refcounted");                       // retain the cell and append it with value_type 7
    emitter.instruction("b __rt_array_from_mixed_ret");                         // return the single-element array

    // -- indexed array: rebuild a fresh boxed-Mixed array element-by-element --
    emitter.label("__rt_array_from_mixed_array");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the boxed Mixed pointer
    emitter.instruction("ldr x9, [x9, #8]");                                    // load the indexed-array payload pointer
    emitter.instruction("ldr x10, [x9]");                                       // load the source array length
    emitter.instruction("str x10, [sp, #24]");                                  // save the source length for the build loop
    emitter.instruction("mov x0, x10");                                         // destination capacity = source length
    emitter.instruction("cmp x10, #0");                                         // is the source array empty?
    emitter.instruction("b.ne __rt_array_from_mixed_array_alloc");              // non-empty arrays size the destination to the length
    emitter.instruction("mov x0, #1");                                          // empty arrays still need a non-zero capacity
    emitter.label("__rt_array_from_mixed_array_alloc");
    emitter.instruction("mov x1, #8");                                          // 8-byte slots for boxed Mixed elements
    emitter.instruction("bl __rt_array_new");                                   // allocate the destination boxed-Mixed array
    emitter.instruction("str x0, [sp, #8]");                                    // save the destination array pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the element index to 0
    emitter.label("__rt_array_from_mixed_array_loop");
    emitter.instruction("ldr x11, [sp, #16]");                                  // load the current element index
    emitter.instruction("ldr x12, [sp, #24]");                                  // load the source length
    emitter.instruction("cmp x11, x12");                                        // have all elements been rebuilt?
    emitter.instruction("b.ge __rt_array_from_mixed_array_done");               // finish once every element is appended
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed Mixed receiver
    emitter.instruction("mov x1, x11");                                         // element index as the lookup key
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("bl __rt_mixed_array_get");                             // read and box element i as an owned Mixed cell
    emitter.instruction("str x0, [sp, #32]");                                   // save the owned boxed element cell
    emitter.instruction("mov x1, x0");                                          // boxed cell becomes the push child
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // retain the cell and append it with value_type 7
    emitter.instruction("str x0, [sp, #8]");                                    // save the possibly-grown destination pointer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed element cell
    emitter.instruction("bl __rt_decref_mixed");                                // drop the get's owned ref (the array now owns it)
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the element index
    emitter.instruction("add x11, x11, #1");                                    // advance to the next element
    emitter.instruction("str x11, [sp, #16]");                                  // save the advanced element index
    emitter.instruction("b __rt_array_from_mixed_array_loop");                  // continue rebuilding elements
    emitter.label("__rt_array_from_mixed_array_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the rebuilt destination array
    emitter.instruction("b __rt_array_from_mixed_ret");                         // return the rebuilt boxed-Mixed array

    emitter.label("__rt_array_from_mixed_unsupported");
    abi::emit_symbol_address(emitter, "x1", "_array_cast_unsupported_msg");
    emitter.instruction(&format!("mov x2, #{}", ARRAY_CAST_UNSUPPORTED_MSG_LEN)); // pass the (array)-cast fatal message length
    emitter.instruction("mov x0, #2");                                          // write the diagnostic to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);

    emitter.label("__rt_array_from_mixed_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the local frame
    emitter.instruction("ret");                                                 // return the indexed-array pointer in x0
}

/// x86_64 implementation of `__rt_array_from_mixed` (input `rdi`, result `rax`).
fn emit_array_from_mixed_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_from_mixed ---");
    emitter.label_global("__rt_array_from_mixed");

    // Frame: [rbp-8]=mixed [rbp-16]=dest [rbp-24]=index [rbp-32]=len [rbp-40]=cell
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // reserve the dispatch/build spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed Mixed pointer
    emitter.instruction("test rdi, rdi");                                       // is the boxed Mixed pointer null?
    emitter.instruction("je __rt_array_from_mixed_null");                       // a null pointer yields an empty array
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the runtime tag from mixed[0]
    emitter.instruction("cmp r10, 8");                                          // is the payload PHP null?
    emitter.instruction("je __rt_array_from_mixed_null");                       // null casts to an empty array
    emitter.instruction("cmp r10, 4");                                          // is the payload an indexed array?
    emitter.instruction("je __rt_array_from_mixed_array");                      // indexed arrays rebuild into a boxed-Mixed array
    emitter.instruction("cmp r10, 0");                                          // is the payload an int scalar?
    emitter.instruction("je __rt_array_from_mixed_scalar");                     // scalars wrap in a single-element array
    emitter.instruction("cmp r10, 1");                                          // is the payload a string scalar?
    emitter.instruction("je __rt_array_from_mixed_scalar");                     // scalars wrap in a single-element array
    emitter.instruction("cmp r10, 2");                                          // is the payload a float scalar?
    emitter.instruction("je __rt_array_from_mixed_scalar");                     // scalars wrap in a single-element array
    emitter.instruction("cmp r10, 3");                                          // is the payload a bool scalar?
    emitter.instruction("je __rt_array_from_mixed_scalar");                     // scalars wrap in a single-element array
    emitter.instruction("jmp __rt_array_from_mixed_unsupported");               // assoc array / object / other -> fatal

    emitter.label("__rt_array_from_mixed_null");
    emitter.instruction("mov rdi, 0");                                          // capacity 0 for the empty result array
    emitter.instruction("mov rsi, 8");                                          // 8-byte slots for boxed Mixed elements
    emitter.instruction("call __rt_array_new");                                 // allocate an empty indexed array
    emitter.instruction("jmp __rt_array_from_mixed_ret");                       // return the empty array

    emitter.label("__rt_array_from_mixed_scalar");
    emitter.instruction("mov rdi, 1");                                          // capacity 1 for the single scalar element
    emitter.instruction("mov rsi, 8");                                          // 8-byte slots for boxed Mixed elements
    emitter.instruction("call __rt_array_new");                                 // allocate the one-element indexed array
    emitter.instruction("mov rdi, rax");                                        // destination array for the refcounted push
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // borrowed boxed scalar Mixed cell (push child)
    emitter.instruction("call __rt_array_push_refcounted");                     // retain the cell and append it with value_type 7
    emitter.instruction("jmp __rt_array_from_mixed_ret");                       // return the single-element array

    // -- indexed array: rebuild a fresh boxed-Mixed array element-by-element --
    emitter.label("__rt_array_from_mixed_array");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the boxed Mixed pointer
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // load the indexed-array payload pointer
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the source array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the source length for the build loop
    emitter.instruction("mov rdi, r11");                                        // destination capacity = source length
    emitter.instruction("test r11, r11");                                       // is the source array empty?
    emitter.instruction("jne __rt_array_from_mixed_array_alloc");               // non-empty arrays size the destination to the length
    emitter.instruction("mov rdi, 1");                                          // empty arrays still need a non-zero capacity
    emitter.label("__rt_array_from_mixed_array_alloc");
    emitter.instruction("mov rsi, 8");                                          // 8-byte slots for boxed Mixed elements
    emitter.instruction("call __rt_array_new");                                 // allocate the destination boxed-Mixed array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the destination array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the element index to 0
    emitter.label("__rt_array_from_mixed_array_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // load the current element index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all elements been rebuilt?
    emitter.instruction("jge __rt_array_from_mixed_array_done");                // finish once every element is appended
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the boxed Mixed receiver
    emitter.instruction("mov rsi, r10");                                        // element index as the lookup key
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("call __rt_mixed_array_get");                           // read and box element i as an owned Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the owned boxed element cell
    emitter.instruction("mov rsi, rax");                                        // boxed cell becomes the push child
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination array pointer
    emitter.instruction("call __rt_array_push_refcounted");                     // retain the cell and append it with value_type 7
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the possibly-grown destination pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed element cell
    emitter.instruction("call __rt_decref_mixed");                              // drop the get's owned ref (the array now owns it)
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the element index
    emitter.instruction("add r10, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the advanced element index
    emitter.instruction("jmp __rt_array_from_mixed_array_loop");                // continue rebuilding elements
    emitter.label("__rt_array_from_mixed_array_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the rebuilt destination array
    emitter.instruction("jmp __rt_array_from_mixed_ret");                       // return the rebuilt boxed-Mixed array

    emitter.label("__rt_array_from_mixed_unsupported");
    emitter.instruction("mov edi, 2");                                          // write the diagnostic to Linux stderr
    abi::emit_symbol_address(emitter, "rsi", "_array_cast_unsupported_msg");    // point the write() buffer at the fatal message
    emitter.instruction(&format!("mov edx, {}", ARRAY_CAST_UNSUPPORTED_MSG_LEN)); // pass the (array)-cast fatal message length
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the (array)-cast fatal before exiting
    emitter.instruction("mov edi, 70");                                         // use EX_SOFTWARE as the process exit status
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after the fatal diagnostic

    emitter.label("__rt_array_from_mixed_ret");
    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the indexed-array pointer in rax
}
