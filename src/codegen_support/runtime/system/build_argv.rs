//! Purpose:
//! Emits the `__rt_build_argv`, `__rt_array_new` runtime helper assembly for build argv.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - The helper constructs PHP $argv arrays from OS argc/argv without taking ownership of OS-provided storage.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_build_argv` runtime helper for the current target.
/// Reads `_global_argc` and `_global_argv` populated by the entry point, iterates over
/// each OS argument string, computes its length via null-terminator scan, and stores a
/// `ptr+len` slot in a runtime array. Returns the array pointer in the function result
/// register (`x0` on ARM64, `rax` on x86_64).
///
/// Every target uses managed array allocation and copies the borrowed OS strings.
/// The returned array owns those copies and supports refcounted cleanup and copy-on-write.
/// Callee-saved registers are preserved across the helper call sequence.
pub fn emit_build_argv(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_build_argv_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: build_argv ---");
    emitter.label_global("__rt_build_argv");

    // -- preserve incoming callee-saved registers before loading process arguments --
    emitter.instruction("sub sp, sp, #64");                                     // reserve saved registers, a loop index, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve caller values before loading argc and argv
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve the caller's array and loop registers

    // -- load argc from the global variable --
    abi::emit_load_symbol_to_reg(emitter, "x19", "_global_argc", 0);

    // -- load argv pointer from the global variable --
    abi::emit_load_symbol_to_reg(emitter, "x20", "_global_argv", 0);

    // -- create a new string array with capacity = argc --
    emitter.instruction("mov x0, x19");                                         // arg0: capacity = argc
    emitter.instruction("mov x1, #16");                                         // arg1: elem_size = 16 (ptr + len per string)
    emitter.instruction("bl __rt_array_new");                                   // allocate the array, x0 = array pointer
    emitter.instruction("mov x21, x0");                                         // x21 = array pointer (save in callee-saved reg)

    // -- initialize loop counter i = 0 --
    emitter.instruction("mov x22, #0");                                         // x22 = 0 (loop counter)
    emitter.instruction("str x22, [sp, #32]");                                  // store i on stack independently of the saved caller registers

    // -- loop: for i = 0..argc, convert each C string and push to array --
    emitter.label("__rt_build_argv_loop");
    emitter.instruction("ldr x22, [sp, #32]");                                  // reload i from stack
    emitter.instruction("cmp x22, x19");                                        // compare i with argc
    emitter.instruction("b.ge __rt_build_argv_done");                           // if i >= argc, exit loop

    // -- get pointer to argv[i] (C string) --
    emitter.instruction("ldr x1, [x20, x22, lsl #3]");                          // x1 = argv[i] (load pointer at argv + i*8)

    // -- compute string length by scanning for null terminator --
    emitter.instruction("mov x2, #0");                                          // x2 = 0 (length counter)
    emitter.label("__rt_build_argv_strlen");
    emitter.instruction("ldrb w3, [x1, x2]");                                   // w3 = byte at str[length] (load single byte)
    emitter.instruction("cbz w3, __rt_build_argv_push");                        // if byte == 0 (null terminator), done counting
    emitter.instruction("add x2, x2, #1");                                      // length += 1
    emitter.instruction("b __rt_build_argv_strlen");                            // continue scanning

    // -- push the string (ptr in x1, len in x2) to the array --
    emitter.label("__rt_build_argv_push");
    emitter.instruction("mov x0, x21");                                         // arg0: array pointer
    emitter.instruction("bl __rt_array_push_str");                              // push string element to array
    emitter.instruction("mov x21, x0");                                         // update array pointer after possible realloc

    // -- increment loop counter and continue --
    emitter.instruction("ldr x22, [sp, #32]");                                  // reload i from stack after string persistence
    emitter.instruction("add x22, x22, #1");                                    // i += 1
    emitter.instruction("str x22, [sp, #32]");                                  // save updated i back to stack
    emitter.instruction("b __rt_build_argv_loop");                              // continue loop

    // -- loop complete, return the array pointer --
    emitter.label("__rt_build_argv_done");
    emitter.instruction("mov x0, x21");                                         // return value: array pointer in x0

    // -- restore callee-saved registers and tear down stack frame --
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore x19 (argc) and x20 (argv)
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore both caller-owned callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_build_argv` using the System V AMD64 ABI.
/// Caller-saved registers are scratch only; frame slots preserve argc, argv, the managed array,
/// and the loop index across allocation and string-copy calls. Returns one owned array in rax.
fn emit_build_argv_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: build_argv ---");
    emitter.label_global("__rt_build_argv");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving local scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for argc/argv bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for argc, argv, result pointer, and loop index

    abi::emit_load_symbol_to_reg(emitter, "r8", "_global_argc", 0);
    abi::emit_load_symbol_to_reg(emitter, "r9", "_global_argv", 0);
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save argc across managed allocation and later loop iterations
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the borrowed OS pointer array across runtime calls

    emitter.instruction("mov rdi, r8");                                         // reserve capacity for every process argument
    emitter.instruction("mov esi, 16");                                         // each string entry stores a pointer and byte length
    emitter.instruction("call __rt_array_new");                                 // allocate a refcounted array with the managed heap header
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the allocated array pointer for the loop body and final return

    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the argv loop counter to zero

    emitter.label("__rt_build_argv_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the current argv element index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 8]");                        // compare the loop index against argc
    emitter.instruction("jae __rt_build_argv_done");                            // stop once every OS argv entry has been materialized

    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the OS argv pointer table base
    emitter.instruction("mov r11, QWORD PTR [r10 + rcx * 8]");                  // load argv[i] as a raw C string pointer
    emitter.instruction("xor rdx, rdx");                                        // reset the byte-count accumulator before scanning for the null terminator

    emitter.label("__rt_build_argv_strlen");
    emitter.instruction("mov al, BYTE PTR [r11 + rdx]");                        // read the next byte from argv[i] while measuring its PHP string length
    emitter.instruction("test al, al");                                         // check whether the current byte is the terminating NUL
    emitter.instruction("je __rt_build_argv_store");                            // stop scanning once the C string terminator is reached
    emitter.instruction("add rdx, 1");                                          // advance the measured argv[i] length by one byte
    emitter.instruction("jmp __rt_build_argv_strlen");                          // continue scanning the current argv[i] C string

    emitter.label("__rt_build_argv_store");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the managed destination array before appending
    emitter.instruction("mov rsi, r11");                                        // borrow the current OS string while its length remains in rdx
    emitter.instruction("call __rt_array_push_str");                            // append an independently owned copy of the process argument
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the updated array pointer after a possible growth

    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance the frame-owned loop index after the runtime call
    emitter.instruction("jmp __rt_build_argv_loop");                            // continue materializing the remaining argv entries

    emitter.label("__rt_build_argv_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the argv array pointer in the integer result register
    emitter.instruction("add rsp, 32");                                         // release the local argc/argv scratch slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the materialized argv array header pointer
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::Target;

    use super::*;

    /// Every target owns managed argv storage and ARM64 saves registers before overwriting them.
    #[test]
    fn argv_uses_managed_storage_and_preserves_callee_saved_registers() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_build_argv(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_array_new") && asm.contains("__rt_array_push_str"), "{name}");
            assert!(!asm.contains("malloc"), "{name}");
            if target.arch == Arch::AArch64 {
                assert!(asm.find("stp x19, x20").unwrap() < asm.find("_global_argc").unwrap(), "{name}");
                assert!(asm.find("stp x21, x22").unwrap() < asm.find("mov x22, #0").unwrap(), "{name}");
                assert!(asm.contains("ldp x21, x22"), "{name}");
            }
        }
    }
}
