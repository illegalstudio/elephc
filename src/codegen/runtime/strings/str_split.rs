//! Purpose:
//! Emits the `__rt_str_split`, `__rt_array_new` runtime helper assembly for str split.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - String helpers scan or transform byte ranges and return target ABI pointer/length pairs for generated call sites.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::data::STR_SPLIT_LENGTH_MSG;

/// Emits the `__rt_str_split` runtime helper that splits a byte string into an array of fixed-length chunks.
///
/// ## ABI
/// - **AArch64**: x1=ptr, x2=len, x3=chunk_len → x0=array_ptr
/// - **x86_64 Linux**: rax=ptr, rdx=len, rdi=chunk_len → rax=array_ptr
///
/// ## Behavior
/// - A chunk length below 1 is a PHP `ValueError`; this emits the fatal diagnostic and exits (the
///   loop would otherwise never advance and hang), matching the uncatchable-fatal policy.
/// - An empty source string yields a single empty-string element (PHP `str_split("") === [""]`).
/// - Otherwise iterates through the string in chunk_len increments, copying each chunk as a new
///   string entry; the final chunk may be shorter if fewer bytes remain than chunk_len.
/// - Delegates to `__rt_array_push_str` for array growth and element appending.
/// - Allocates initial array with capacity 16, elem_size 16 (ptr+len slots).
/// - Caller-saved registers are preserved across the internal loop.
pub fn emit_str_split(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_split_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_split ---");
    emitter.label_global("__rt_str_split");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer
    emitter.instruction("cmp x3, #1");                                          // PHP requires the chunk length to be at least 1
    emitter.instruction("b.lt __rt_str_split_bad_length");                      // length < 1 is a PHP ValueError → uncatchable fatal
    emitter.instruction("stp x1, x2, [sp]");                                    // save string ptr/len
    emitter.instruction("str x3, [sp, #16]");                                   // save chunk length

    // -- create array --
    emitter.instruction("mov x0, #16");                                         // initial capacity
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (str ptr+len)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #24]");                                   // save array pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // current position = 0
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length
    emitter.instruction("cbz x2, __rt_str_split_empty_input");                  // an empty source yields PHP's single "" element

    emitter.label("__rt_str_split_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                   // load current position
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload string ptr/len
    emitter.instruction("cmp x4, x2");                                          // past end of string?
    emitter.instruction("b.ge __rt_str_split_done");                            // yes → done

    // -- compute this chunk's actual length --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("sub x5, x2, x4");                                      // remaining = len - pos
    emitter.instruction("cmp x5, x3");                                          // remaining vs chunk_length
    emitter.instruction("csel x5, x3, x5, gt");                                 // chunk = min(remaining, chunk_length)

    // -- push chunk as string element --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload array pointer
    emitter.instruction("add x1, x1, x4");                                      // x1 = base + current position
    emitter.instruction("mov x2, x5");                                          // x2 = chunk length
    emitter.instruction("bl __rt_array_push_str");                              // push chunk onto array
    emitter.instruction("str x0, [sp, #24]");                                   // update array pointer after possible realloc

    // -- advance position by chunk length --
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload position
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("add x4, x4, x3");                                      // position += chunk_length
    emitter.instruction("str x4, [sp, #32]");                                   // save updated position
    emitter.instruction("b __rt_str_split_loop");                               // continue

    emitter.label("__rt_str_split_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return

    // -- empty source string: push a single "" element (PHP str_split("") === [""]) --
    emitter.label("__rt_str_split_empty_input");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the result array pointer
    emitter.instruction("ldr x1, [sp]");                                        // source pointer (zero-length, never dereferenced)
    emitter.instruction("mov x2, #0");                                          // zero-length chunk for the lone empty element
    emitter.instruction("bl __rt_array_push_str");                              // append one empty string element
    emitter.instruction("str x0, [sp, #24]");                                   // save the possibly-grown array pointer
    emitter.instruction("b __rt_str_split_done");                               // return the single-element array

    // -- fatal error: chunk length below 1 --
    emitter.label("__rt_str_split_bad_length");
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the invalid-length diagnostic
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_str_split_length_msg");
    emitter.instruction(&format!("mov x2, #{}", STR_SPLIT_LENGTH_MSG.len()));   // pass the exact invalid-length diagnostic byte count
    emitter.syscall(4);                                                         // write the diagnostic to stderr
    emitter.instruction("mov x0, #1");                                          // exit code 1 for the invalid-length abort path
    emitter.syscall(1);                                                         // terminate the process
}

/// x86_64 Linux-specific implementation of the `__rt_str_split` runtime helper.
///
/// ## ABI
/// - Input: rax=ptr, rdx=len, rdi=chunk_len
/// - Output: rax=array_ptr
///
/// ## Behavior
/// - Mirrors the AArch64 variant: a chunk length below 1 fatals (PHP `ValueError`), and an empty
///   source string yields a single empty-string element (`str_split("") === [""]`).
///
/// ## Stack frame
/// - Saves rax/rdx/rdi across calls to `__rt_array_new` and `__rt_array_push_str`.
/// - Uses rbp-8/16/24 for ptr/len/chunk_len; rbp-32/40 for array ptr and cursor.
/// - Preserves all caller-saved registers except rax (used for return values).
fn emit_str_split_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_split ---");
    emitter.label_global("__rt_str_split");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving str_split() spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source string, chunk length, array pointer, and cursor
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the source string, chunk length, array pointer, and current position
    emitter.instruction("cmp rdi, 1");                                          // PHP requires the chunk length to be at least 1
    emitter.instruction("jl __rt_str_split_bad_length_linux_x86_64");           // length < 1 is a PHP ValueError → uncatchable fatal
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the source string pointer across array allocation and push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the source string length across array allocation and push helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the requested chunk length across array allocation and push helper calls
    emitter.instruction("mov edi, 16");                                         // seed the result array with the same initial capacity used by the AArch64 str_split() helper
    emitter.instruction("mov esi, 16");                                         // use 16-byte string slots (ptr + len) for the str_split() result array payload
    emitter.instruction("call __rt_array_new");                                 // allocate the result array that will hold the fixed-size string chunks
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the result array pointer across later push helper calls and possible growth
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start scanning the source string from byte offset zero
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the source length to detect an empty source string
    emitter.instruction("test rcx, rcx");                                       // is the source string empty?
    emitter.instruction("jz __rt_str_split_empty_input_linux_x86_64");          // an empty source yields PHP's single "" element

    emitter.label("__rt_str_split_loop_linux_x86_64");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the current source-string byte offset before testing loop completion
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the source-string length before testing loop completion
    emitter.instruction("cmp rcx, r8");                                         // have we already consumed every byte of the source string?
    emitter.instruction("jge __rt_str_split_done_linux_x86_64");                // stop once the current offset reaches the source-string length
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the requested chunk length before clamping the final chunk
    emitter.instruction("mov r10, r8");                                         // copy the remaining-length base so the final chunk length can be clamped to the source tail
    emitter.instruction("sub r10, rcx");                                        // compute how many bytes remain from the current source offset to the end of the string
    emitter.instruction("cmp r10, r9");                                         // is the remaining tail shorter than the requested chunk length?
    emitter.instruction("cmovl r9, r10");                                       // clamp the chunk length down to the remaining tail for the final partial chunk
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source string pointer before computing the chunk start address
    emitter.instruction("add rax, rcx");                                        // advance the source string pointer to the start of the current chunk slice
    emitter.instruction("mov rdx, r9");                                         // move the clamped chunk length into the x86_64 string-helper length register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the current result array pointer before appending the next chunk slice
    emitter.instruction("mov rsi, rax");                                        // pass the current chunk slice pointer to the string-array append helper
    emitter.instruction("call __rt_array_push_str");                            // append the current chunk slice as an owned string entry, growing the result array if needed
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the possibly grown result array pointer returned by the append helper
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the current source-string byte offset before advancing to the next chunk
    emitter.instruction("add rcx, QWORD PTR [rbp - 24]");                       // advance by the requested chunk length so the next iteration starts at the correct source offset
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the updated source-string byte offset for the next loop iteration
    emitter.instruction("jmp __rt_str_split_loop_linux_x86_64");                // continue splitting the source string until every byte has been chunked

    emitter.label("__rt_str_split_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the result array pointer after the final chunk append has completed
    emitter.instruction("add rsp, 48");                                         // release the str_split() spill slots before returning the result array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the result array pointer in the standard x86_64 integer result register

    // -- empty source string: push a single "" element (PHP str_split("") === [""]) --
    emitter.label("__rt_str_split_empty_input_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the result array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // source pointer (zero-length, never dereferenced)
    emitter.instruction("xor rdx, rdx");                                        // zero-length chunk for the lone empty element
    emitter.instruction("call __rt_array_push_str");                            // append one empty string element
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the possibly-grown array pointer
    emitter.instruction("jmp __rt_str_split_done_linux_x86_64");                // return the single-element array

    // -- fatal error: chunk length below 1 --
    emitter.label("__rt_str_split_bad_length_linux_x86_64");
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the invalid-length diagnostic
    crate::codegen::abi::emit_symbol_address(emitter, "rsi", "_str_split_length_msg");
    emitter.instruction(&format!("mov edx, {}", STR_SPLIT_LENGTH_MSG.len()));   // pass the exact invalid-length diagnostic byte count
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal invalid-length message before terminating
    emitter.instruction("mov edi, 1");                                          // exit code 1 for the invalid-length abort path
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the invalid length
}
