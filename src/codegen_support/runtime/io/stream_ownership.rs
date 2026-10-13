//! Purpose:
//! Emits per-context ownership bookkeeping for ordinary OS stream descriptors.
//!
//! Called from:
//! - Stream open/close helpers and the Parallel fatal resource drain.
//!
//! Key details:
//! - Only descriptors in the runtime's reviewed 0..256 table range are tracked.
//! - Mark/clear preserve the descriptor result register for call-site composition.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Marks the descriptor in the result register as owned by the active runtime context.
pub fn emit_stream_owner_mark(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_owner_mark ---");
    emitter.label_global("__rt_stream_owner_mark");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // reject negative descriptors before indexing the ownership table
            emitter.instruction("b.lt __rt_stream_owner_mark_done");            // failed opens own no descriptor
            emitter.instruction("cmp x0, #256");                                // stay inside the fixed descriptor registry
            emitter.instruction("b.hs __rt_stream_owner_mark_done");            // unsupported high descriptors are not indexed out of bounds
            abi::emit_symbol_address(emitter, "x9", "_stream_owned_fds");
            emitter.instruction("mov w10, #1");                                 // one means the active context owns this descriptor
            emitter.instruction("strb w10, [x9, x0]");                          // publish ownership after the OS open succeeded
            emitter.label("__rt_stream_owner_mark_done");
            emitter.instruction("ret");                                         // return the original descriptor unchanged
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // reject negative descriptors before indexing the ownership table
            emitter.instruction("js __rt_stream_owner_mark_done_x86");          // failed opens own no descriptor
            emitter.instruction("cmp rax, 256");                                // stay inside the fixed descriptor registry
            emitter.instruction("jae __rt_stream_owner_mark_done_x86");         // unsupported high descriptors are not indexed out of bounds
            abi::emit_symbol_address(emitter, "r9", "_stream_owned_fds");
            emitter.instruction("mov BYTE PTR [r9 + rax], 1");                  // publish ownership after the OS open succeeded
            emitter.label("__rt_stream_owner_mark_done_x86");
            emitter.instruction("ret");                                         // return the original descriptor unchanged
        }
    }
}

/// Clears active-context ownership for the descriptor in the result register.
pub fn emit_stream_owner_clear(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_owner_clear ---");
    emitter.label_global("__rt_stream_owner_clear");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // reject negative descriptors before indexing the ownership table
            emitter.instruction("b.lt __rt_stream_owner_clear_done");           // failed or synthetic descriptors own no OS slot here
            emitter.instruction("cmp x0, #256");                                // stay inside the fixed descriptor registry
            emitter.instruction("b.hs __rt_stream_owner_clear_done");           // synthetic and unsupported high descriptors bypass this table
            abi::emit_symbol_address(emitter, "x9", "_stream_owned_fds");
            emitter.instruction("strb wzr, [x9, x0]");                          // clear ownership before the descriptor can be reused
            emitter.label("__rt_stream_owner_clear_done");
            emitter.instruction("ret");                                         // return the original descriptor unchanged
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // reject negative descriptors before indexing the ownership table
            emitter.instruction("js __rt_stream_owner_clear_done_x86");         // failed or synthetic descriptors own no OS slot here
            emitter.instruction("cmp rax, 256");                                // stay inside the fixed descriptor registry
            emitter.instruction("jae __rt_stream_owner_clear_done_x86");        // synthetic and unsupported high descriptors bypass this table
            abi::emit_symbol_address(emitter, "r9", "_stream_owned_fds");
            emitter.instruction("mov BYTE PTR [r9 + rax], 0");                  // clear ownership before the descriptor can be reused
            emitter.label("__rt_stream_owner_clear_done_x86");
            emitter.instruction("ret");                                         // return the original descriptor unchanged
        }
    }
}

/// Drains every OS descriptor still owned by the active context after normal frame cleanup.
pub fn emit_stream_owner_drain(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_owner_drain_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: stream_owner_drain ---");
    emitter.label_global("__rt_stream_owner_drain");
    emitter.instruction("sub sp, sp, #48");                                     // reserve an aligned frame for the descriptor scan
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve frame pointer and return address
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve the descriptor index and handle scratch
    emitter.instruction("add x29, sp, #32");                                    // establish the drain frame pointer
    emitter.instruction("mov x19, #0");                                         // begin at descriptor zero
    emitter.label("__rt_stream_owner_drain_loop");
    emitter.instruction("cmp x19, #256");                                       // scanned the complete bounded descriptor range?
    emitter.instruction("b.hs __rt_stream_owner_drain_done");                   // finish after descriptor 255
    abi::emit_symbol_address(emitter, "x9", "_stream_owned_fds");
    emitter.instruction("ldrb w10, [x9, x19]");                                 // test whether this context owns the descriptor
    emitter.instruction("cbz w10, __rt_stream_owner_drain_next");               // leave descriptors owned by other contexts untouched
    emitter.instruction("strb wzr, [x9, x19]");                                 // detach ownership before any close permits fd reuse
    emitter.instruction("mov x0, x19");                                         // identify the descriptor being abandoned by this worker
    abi::emit_call_label(emitter, "__rt_resource_inventory_abandon_fd");

    for (name, table, function, pass_handle) in [
        ("zlib", "_zstream_handles", "_zlib_close_fn", false),
        ("bz2", "_bzstream_handles", "_bz2_close_fn", false),
        ("iconv", "_iconv_handles", "_iconv_close_fn", false),
        ("tls", "_tls_sessions", "_elephc_tls_close_fn", true),
    ] {
        let skip = format!("__rt_stream_owner_drain_{name}_skip");
        abi::emit_symbol_address(emitter, "x9", table);
        emitter.instruction("ldr x20, [x9, x19, lsl #3]");                      // load this descriptor's attached native handle
        emitter.instruction(&format!("cbz x20, {skip}"));                       // skip absent filter or TLS state
        abi::emit_load_symbol_to_reg(emitter, "x10", function, 0);
        emitter.instruction(&format!("cbz x10, {skip}"));                       // a missing optional bridge cannot be called
        emitter.instruction(if pass_handle { "mov x0, x20" } else { "mov x0, x19" }); // pass the bridge's reviewed close argument
        emitter.instruction("blr x10");                                         // release the attached native filter or TLS state
        if pass_handle {
            abi::emit_symbol_address(emitter, "x9", table);
            emitter.instruction("str xzr, [x9, x19, lsl #3]");                  // clear TLS state after the bridge released its handle
        }
        emitter.label(&skip);
    }

    for table in ["_stream_read_filters", "_stream_write_filters"] {
        abi::emit_symbol_address(emitter, "x9", table);
        emitter.instruction("strb wzr, [x9, x19]");                             // clear the descriptor's filter dispatch id
    }
    emitter.instruction("mov x0, x19");                                         // pass the fd to fatal-only user-filter ownership cleanup
    emitter.instruction("bl __rt_user_filter_abandon_fd");                      // detach and release unique filter instances without onClose
    abi::emit_symbol_address(emitter, "x9", "_stream_chunk_size");
    emitter.instruction("str xzr, [x9, x19, lsl #3]");                          // clear per-descriptor chunk metadata
    abi::emit_symbol_address(emitter, "x9", "_stream_connect_host");
    emitter.instruction("add x10, x9, x19, lsl #4");                            // address this descriptor's host pointer/length pair
    emitter.instruction("stp xzr, xzr, [x10]");                                 // clear connection-host metadata before fd reuse
    abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("strb wzr, [x9, x19]");                                 // clear stale EOF state before fd reuse
    abi::emit_symbol_address(emitter, "x9", "_popen_files");
    emitter.instruction("ldr x10, [x9, x19, lsl #3]");                          // does this descriptor own a process pipe?
    emitter.instruction("cbz x10, __rt_stream_owner_drain_directory");          // ordinary streams and directories use later branches
    emitter.instruction("mov x0, x19");                                         // pass the pipe descriptor to pclose
    emitter.instruction("bl __rt_pclose");                                      // close the stream and reap its child
    emitter.instruction("b __rt_stream_owner_drain_next");                      // the descriptor is completely drained
    emitter.label("__rt_stream_owner_drain_directory");
    abi::emit_symbol_address(emitter, "x9", "_glob_handles");
    emitter.instruction("ldr x10, [x9, x19, lsl #3]");                          // inspect a glob directory owner first
    abi::emit_symbol_address(emitter, "x9", "_dir_handles");
    emitter.instruction("ldr x11, [x9, x19, lsl #3]");                          // inspect an ordinary directory owner
    emitter.instruction("orr x10, x10, x11");                                   // either table selects the directory cleanup path
    emitter.instruction("cbz x10, __rt_stream_owner_drain_plain");              // no special owner means an ordinary descriptor
    emitter.instruction("mov x0, x19");                                         // pass the directory descriptor to the shared closer
    emitter.instruction("bl __rt_closedir");                                    // release DIR/glob state and close its descriptor
    emitter.instruction("b __rt_stream_owner_drain_next");                      // the descriptor is completely drained
    emitter.label("__rt_stream_owner_drain_plain");
    emitter.instruction("mov x0, x19");                                         // pass the remaining ordinary descriptor to close
    emitter.syscall(6);
    emitter.label("__rt_stream_owner_drain_next");
    emitter.instruction("add x19, x19, #1");                                    // advance to the next bounded descriptor slot
    emitter.instruction("b __rt_stream_owner_drain_loop");                      // continue until every owned descriptor is closed
    emitter.label("__rt_stream_owner_drain_done");
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved scan registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore caller frame state
    emitter.instruction("add sp, sp, #48");                                     // release the drain frame
    emitter.instruction("ret");                                                 // return only after no owned descriptor remains
}

/// Drains SysV descriptor/filter/TLS owners after retiring their resource inventory incarnations.
fn emit_stream_owner_drain_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_owner_drain ---");
    emitter.label_global("__rt_stream_owner_drain");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the drain frame pointer
    emitter.instruction("push r12");                                            // preserve the descriptor index
    emitter.instruction("push r13");                                            // preserve the attached-handle scratch
    emitter.instruction("xor r12d, r12d");                                      // begin at descriptor zero
    emitter.label("__rt_stream_owner_drain_loop_x86");
    emitter.instruction("cmp r12, 256");                                        // scanned the complete bounded descriptor range?
    emitter.instruction("jae __rt_stream_owner_drain_done_x86");                // finish after descriptor 255
    abi::emit_symbol_address(emitter, "r9", "_stream_owned_fds");
    emitter.instruction("cmp BYTE PTR [r9 + r12], 0");                          // does this context own the descriptor?
    emitter.instruction("je __rt_stream_owner_drain_next_x86");                 // leave descriptors owned by other contexts untouched
    emitter.instruction("mov BYTE PTR [r9 + r12], 0");                          // detach ownership before any close permits fd reuse
    emitter.instruction("mov rdi, r12");                                        // identify the descriptor being abandoned by this worker
    abi::emit_call_label(emitter, "__rt_resource_inventory_abandon_fd");

    for (name, table, function, pass_handle) in [
        ("zlib", "_zstream_handles", "_zlib_close_fn", false),
        ("bz2", "_bzstream_handles", "_bz2_close_fn", false),
        ("iconv", "_iconv_handles", "_iconv_close_fn", false),
        ("tls", "_tls_sessions", "_elephc_tls_close_fn", true),
    ] {
        let skip = format!("__rt_stream_owner_drain_{name}_skip_x86");
        abi::emit_symbol_address(emitter, "r9", table);
        emitter.instruction("mov r13, QWORD PTR [r9 + r12 * 8]");               // load this descriptor's attached native handle
        emitter.instruction("test r13, r13");                                   // is filter or TLS state attached?
        emitter.instruction(&format!("jz {skip}"));                             // skip absent state
        abi::emit_load_symbol_to_reg(emitter, "r10", function, 0);
        emitter.instruction("test r10, r10");                                   // is the optional bridge close hook installed?
        emitter.instruction(&format!("jz {skip}"));                             // a missing optional bridge cannot be called
        emitter.instruction(if pass_handle { "mov rdi, r13" } else { "mov rdi, r12" }); // pass the bridge's reviewed close argument
        emitter.instruction("call r10");                                        // release the attached native filter or TLS state
        if pass_handle {
            abi::emit_symbol_address(emitter, "r9", table);
            emitter.instruction("mov QWORD PTR [r9 + r12 * 8], 0");             // clear TLS state after the bridge released its handle
        }
        emitter.label(&skip);
    }

    for table in ["_stream_read_filters", "_stream_write_filters"] {
        abi::emit_symbol_address(emitter, "r9", table);
        emitter.instruction("mov BYTE PTR [r9 + r12], 0");                      // clear the descriptor's filter dispatch id
    }
    emitter.instruction("mov rdi, r12");                                        // pass the fd to fatal-only user-filter ownership cleanup
    emitter.instruction("call __rt_user_filter_abandon_fd");                    // detach and release unique filter instances without onClose
    abi::emit_symbol_address(emitter, "r9", "_stream_chunk_size");
    emitter.instruction("mov QWORD PTR [r9 + r12 * 8], 0");                     // clear per-descriptor chunk metadata
    abi::emit_symbol_address(emitter, "r9", "_stream_connect_host");
    emitter.instruction("mov r10, r12");                                        // copy descriptor index for 16-byte host metadata scaling
    emitter.instruction("shl r10, 4");                                          // scale descriptor index by pointer/length pair size
    emitter.instruction("mov QWORD PTR [r9 + r10], 0");                         // clear connection-host pointer
    emitter.instruction("mov QWORD PTR [r9 + r10 + 8], 0");                     // clear connection-host length
    abi::emit_symbol_address(emitter, "r9", "_eof_flags");
    emitter.instruction("mov BYTE PTR [r9 + r12], 0");                          // clear stale EOF state before fd reuse
    abi::emit_symbol_address(emitter, "r9", "_popen_files");
    emitter.instruction("cmp QWORD PTR [r9 + r12 * 8], 0");                     // does this descriptor own a process pipe?
    emitter.instruction("je __rt_stream_owner_drain_directory_x86");            // ordinary streams and directories use later branches
    emitter.instruction("mov rdi, r12");                                        // pass the pipe descriptor to pclose
    emitter.instruction("call __rt_pclose");                                    // close the stream and reap its child
    emitter.instruction("jmp __rt_stream_owner_drain_next_x86");                // the descriptor is completely drained
    emitter.label("__rt_stream_owner_drain_directory_x86");
    abi::emit_symbol_address(emitter, "r9", "_glob_handles");
    emitter.instruction("mov r10, QWORD PTR [r9 + r12 * 8]");                   // inspect a glob directory owner first
    abi::emit_symbol_address(emitter, "r9", "_dir_handles");
    emitter.instruction("or r10, QWORD PTR [r9 + r12 * 8]");                    // combine the ordinary directory owner
    emitter.instruction("test r10, r10");                                       // did either directory table own this fd?
    emitter.instruction("jz __rt_stream_owner_drain_plain_x86");                // no special owner means an ordinary descriptor
    emitter.instruction("mov rdi, r12");                                        // pass the directory descriptor to the shared closer
    emitter.instruction("call __rt_closedir");                                  // release DIR/glob state and close its descriptor
    emitter.instruction("jmp __rt_stream_owner_drain_next_x86");                // the descriptor is completely drained
    emitter.label("__rt_stream_owner_drain_plain_x86");
    emitter.instruction("mov rdi, r12");                                        // pass the remaining ordinary descriptor to libc close
    emitter.instruction("call close");                                          // close only a descriptor claimed by this context
    emitter.label("__rt_stream_owner_drain_next_x86");
    emitter.instruction("inc r12");                                             // advance to the next bounded descriptor slot
    emitter.instruction("jmp __rt_stream_owner_drain_loop_x86");                // continue until every owned descriptor is closed
    emitter.label("__rt_stream_owner_drain_done_x86");
    emitter.instruction("pop r13");                                             // restore attached-handle scratch
    emitter.instruction("pop r12");                                             // restore descriptor index
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return only after no owned descriptor remains
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Both ABI emitters mark and clear only the calling context's bounded ownership table.
    #[test]
    fn ownership_helpers_are_bounded_context_writes_on_both_abis() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emitter.ctx_register = true;
            emit_stream_owner_mark(&mut emitter);
            emit_stream_owner_clear(&mut emitter);
            let ctx_register = crate::codegen_support::runtime::ctx::ctx_reg(&emitter);
            let asm = emitter.output();
            assert!(asm.contains("256"), "{target:?}:\n{asm}");
            assert!(asm.contains(ctx_register));
            assert!(!asm.contains("_stream_owned_fds"), "{target:?}:\n{asm}");
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("strb w10, [x9, x0]"), "{asm}");
                    assert!(asm.contains("strb wzr, [x9, x0]"), "{asm}");
                }
                Arch::X86_64 => {
                    assert!(asm.contains("mov BYTE PTR [r9 + rax], 1"), "{asm}");
                    assert!(asm.contains("mov BYTE PTR [r9 + rax], 0"), "{asm}");
                }
            }
        }
    }

    /// A descriptor's inventory incarnation retires before native cleanup permits fd reuse.
    #[test]
    fn drain_detaches_ownership_then_closes_special_or_plain_descriptors() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emitter.ctx_register = true;
            emit_stream_owner_drain(&mut emitter);
            let asm = emitter.output();
            let detach = if target.arch == Arch::AArch64 {
                asm.find("strb wzr, [x9, x19]")
            } else {
                asm.find("mov BYTE PTR [r9 + r12], 0")
            }
            .expect("ownership detach");
            let special = asm.find("__rt_pclose").expect("popen cleanup");
            let directory = asm.find("__rt_closedir").expect("directory cleanup");
            let retire = asm.find("__rt_resource_inventory_abandon_fd").expect("inventory incarnation retire");
            assert!(detach < retire && retire < special && retire < directory, "{target:?}:\n{asm}");
            for symbol in [
                "_zlib_close_fn",
                "_bz2_close_fn",
                "_iconv_close_fn",
                "_elephc_tls_close_fn",
            ] {
                assert!(asm.contains(symbol), "{target:?} missing {symbol}:\n{asm}");
            }
        }
    }
}
