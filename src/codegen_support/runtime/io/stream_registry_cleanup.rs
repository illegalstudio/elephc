//! Purpose:
//! Retires owned stream wrapper and filter registration strings at request shutdown.
//!
//! Called from:
//! - Resource inventory teardown, after callbacks can no longer need registrations.
//!
//! Key details:
//! - Clears each registration before freeing its two independently persisted strings.
//! - Uses only stack spills across allocator calls on every supported target.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits callback-free retirement of the two request-owned stream registration tables.
pub(crate) fn emit_stream_registry_cleanup(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_stream_registry_cleanup");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // reserve cursor, count, second string, and saved caller state
            emitter.instruction("stp x29, x30, [sp, #32]");                     // preserve the caller across allocator calls
            emitter.instruction("add x29, sp, #32");                            // establish the cleanup frame
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the cleanup frame
            emitter.instruction("sub rsp, 32");                                 // reserve aligned cursor, count, and second-string storage
        }
    }
    for (table, capacity) in [("_user_wrappers", 64), ("_user_filter_registry", 128)] {
        emit_table_cleanup(emitter, table, capacity);
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore the caller frame and return address
            emitter.instruction("add sp, sp, #48");                             // retire cleanup spills
        }
        Arch::X86_64 => {
            emitter.instruction("add rsp, 32");                                 // retire cleanup spills
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
        }
    }
    emitter.instruction("ret");                                                 // finish after both registration tables are empty
}

/// Frees every occupied four-word registration while preserving the cursor across calls.
fn emit_table_cleanup(emitter: &mut Emitter, table: &str, capacity: usize) {
    let scan = format!("__rt_stream_registry_cleanup{table}_scan");
    let next = format!("__rt_stream_registry_cleanup{table}_next");
    let done = format!("__rt_stream_registry_cleanup{table}_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", table);
            emitter.instruction(&format!("mov x10, #{capacity}"));              // bound the scan by the actual registration table capacity
            emitter.instruction("stp x9, x10, [sp]");                           // save the first slot and remaining count
            emitter.label(&scan);
            emitter.instruction("ldr x9, [sp]");                                // reload the registration cursor
            emitter.instruction("ldr x0, [x9]");                                // load the independently owned name pointer
            emitter.instruction(&format!("cbz x0, {next}"));                    // empty registrations own no strings
            emitter.instruction("ldr x10, [x9, #16]");                          // load the independently owned class name
            emitter.instruction("str x10, [sp, #16]");                          // protect the second owner across the first free
            emitter.instruction("stp xzr, xzr, [x9]");                          // retire the name tuple before freeing storage
            emitter.instruction("stp xzr, xzr, [x9, #16]");                     // retire the class tuple before freeing storage
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the class name owner
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
            emitter.label(&next);
            emitter.instruction("ldp x9, x10, [sp]");                           // reload the cursor and remaining count
            emitter.instruction("add x9, x9, #32");                             // advance by one four-word registration
            emitter.instruction("subs x10, x10, #1");                           // consume one bounded table slot
            emitter.instruction("stp x9, x10, [sp]");                           // preserve cursor state for the next allocator call
            emitter.instruction(&format!("b.ne {scan}"));                       // inspect the next registration until all slots are retired
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r9", table);
            emitter.instruction("mov QWORD PTR [rsp], r9");                     // save the first registration cursor
            emitter.instruction(&format!("mov QWORD PTR [rsp + 8], {capacity}"));// bound the scan by the registration table capacity
            emitter.label(&scan);
            emitter.instruction("mov r9, QWORD PTR [rsp]");                     // reload the registration cursor
            emitter.instruction("mov rax, QWORD PTR [r9]");                     // load the independently owned name pointer
            emitter.instruction("test rax, rax");                               // empty registrations own no strings
            emitter.instruction(&format!("jz {next}"));                         // skip empty slots
            emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                // load the independently owned class name
            emitter.instruction("mov QWORD PTR [rsp + 16], r10");               // protect the second owner across the first free
            for offset in [0, 8, 16, 24] {
                emitter.instruction(&format!("mov QWORD PTR [r9 + {offset}], 0"));// retire every registration word before freeing its storage
            }
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the class name owner
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
            emitter.label(&next);
            emitter.instruction("add QWORD PTR [rsp], 32");                     // advance by one four-word registration
            emitter.instruction("sub QWORD PTR [rsp + 8], 1");                  // consume one bounded table slot
            emitter.instruction(&format!("jnz {scan}"));                        // inspect the next registration until all slots are retired
        }
    }
    emitter.label(&done);
}

/// Every target clears both registration tuples before releasing either owned string.
#[test]
fn stream_registration_shutdown_retires_owned_names_on_all_targets() {
    use crate::codegen_support::platform::Target;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        emit_stream_registry_cleanup(&mut emitter);
        let assembly = emitter.output();
        assert!(assembly.contains("_user_wrappers"), "{name}: wrapper names");
        assert!(assembly.contains("_user_filter_registry"), "{name}: filter names");
        assert_eq!(assembly.matches("__rt_heap_free_safe").count(), 4, "{name}: both owners in both tables");
        let clear = if name == "linux-x86_64" { "mov QWORD PTR [r9 + 24], 0" } else { "stp xzr, xzr, [x9, #16]" };
        assert!(assembly.find(clear).unwrap() < assembly.find("__rt_heap_free_safe").unwrap(), "{name}: retire before free");
    }
}
