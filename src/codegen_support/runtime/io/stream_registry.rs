//! Purpose:
//! Releases the owned strings stored in userspace stream wrapper and filter registrations.
//!
//! Called from:
//! - Wrapper unregistration, full legacy request resets, and executable shutdown.
//!
//! Key details:
//! - A retired row is cleared before releasing either string, so reuse cannot observe stale owners.
//! - Filter registrations remain available until shutdown, after user close callbacks finish.
//! - Application orphan pruning preserves registrations across requests.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits registration retirement and process-shutdown cleanup for both supported architectures.
pub(crate) fn emit_stream_registry(emitter: &mut Emitter) {
    emit_retire(emitter);
    emit_cleanup(emitter);
}

/// Saves caller linkage and reserves three pointer-sized state slots.
fn prologue(emitter: &mut Emitter) {
    // -- reserve aligned state while preserving the native caller linkage --
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #48");                                 // reserve aligned state and caller linkage
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve the caller frame and return address
    } else {
        emitter.instruction("push rbp");                                        // preserve the SysV caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the cleanup frame pointer
        emitter.instruction("sub rsp, 32");                                     // reserve aligned state across string release calls
    }
}

/// Restores the caller linkage while retaining the integer result register.
fn epilogue(emitter: &mut Emitter) {
    // -- restore the native caller after all registration owners have retired --
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore the original caller linkage
        emitter.instruction("add sp, sp, #48");                                 // release the aligned cleanup state
    } else {
        emitter.instruction("add rsp, 32");                                     // release the aligned state slots
        emitter.instruction("pop rbp");                                         // restore the SysV caller frame pointer
    }
    emitter.instruction("ret");                                                 // return to unregistration or normal shutdown
}

/// Consumes one registration row through the native integer ABI and returns PHP true.
fn emit_retire(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (result, frame, scratch) = if arm { ("x0", "sp", "x9") } else { ("rax", "rsp", "r9") };
    emitter.label_global("__rt_stream_registry_retire");
    // -- retain both string owners before clearing the complete registration row --
    prologue(emitter);
    for (source, destination) in [(0, 0), (16, 8)] {
        abi::emit_load_from_address(emitter, scratch, result, source);
        abi::emit_store_to_address(emitter, scratch, frame, destination);
    }
    for offset in [0, 8, 16, 24] {
        abi::emit_store_zero_to_address(emitter, result, offset);
    }
    // -- release each transferred owner after the row no longer publishes it --
    for offset in [0, 8] {
        abi::emit_load_from_address(emitter, result, frame, offset);
        abi::emit_call_label(emitter, "__rt_heap_free_safe");
    }
    abi::emit_load_int_immediate(emitter, result, 1);
    epilogue(emitter);
}

/// Retires every remaining row once the executable can no longer enter PHP close callbacks.
fn emit_cleanup(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (result, frame, scratch) = if arm { ("x0", "sp", "x9") } else { ("rax", "rsp", "r9") };
    emitter.label_global("__rt_stream_registry_cleanup");
    // -- preserve table cursors across each registration's owned-string releases --
    prologue(emitter);
    for (symbol, count, suffix) in [("_user_wrappers", 64, "wrapper"), ("_user_filter_registry", 128, "filter")] {
        let scan = format!("__rt_stream_registry_{suffix}_scan");
        let next = format!("__rt_stream_registry_{suffix}_next");
        abi::emit_symbol_address(emitter, result, symbol);
        abi::emit_store_to_address(emitter, result, frame, 0);
        abi::emit_load_int_immediate(emitter, scratch, count);
        abi::emit_store_to_address(emitter, scratch, frame, 8);
        // -- visit occupied fixed-width rows without invoking PHP code --
        emitter.label(&scan);
        abi::emit_load_from_address(emitter, result, frame, 0);
        abi::emit_load_from_address(emitter, scratch, result, 0);
        emitter.instruction(if arm { "cmp x9, #0" } else { "test r9, r9" });    // skip vacant registration rows
        emitter.instruction(&format!("{} {next}", if arm { "b.eq" } else { "jz" })); // keep empty slots free of release work
        abi::emit_call_label(emitter, "__rt_stream_registry_retire");
        emitter.label(&next);
        abi::emit_load_from_address(emitter, scratch, frame, 0);
        emitter.instruction(if arm { "add x9, x9, #32" } else { "add r9, 32" }); // advance by one four-word registration row
        abi::emit_store_to_address(emitter, scratch, frame, 0);
        abi::emit_load_from_address(emitter, scratch, frame, 8);
        emitter.instruction(if arm { "sub x9, x9, #1" } else { "sub r9, 1" });  // consume one slot from the bounded table scan
        abi::emit_store_to_address(emitter, scratch, frame, 8);
        emitter.instruction(if arm { "cmp x9, #0" } else { "test r9, r9" });    // stop once every registration slot has been visited
        emitter.instruction(&format!("{} {scan}", if arm { "b.ne" } else { "jnz" })); // inspect the next slot while any remain
    }
    // -- restore the executable's shutdown frame after all registry owners retire --
    epilogue(emitter);
}

/// Every target clears both registration tuples before releasing either owned string.
#[test]
fn stream_registration_shutdown_retires_owned_names_on_all_targets() {
    use crate::codegen_support::platform::Target;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        emit_stream_registry(&mut emitter);
        let assembly = emitter.output();
        assert!(assembly.contains("_user_wrappers"), "{name}: wrapper names");
        assert!(assembly.contains("_user_filter_registry"), "{name}: filter names");
        assert_eq!(assembly.matches("__rt_heap_free_safe").count(), 2, "{name}: one release per owner in shared retirement");
        assert_eq!(assembly.lines().filter(|line| {
            let line = line.trim_start();
            (line.starts_with("bl ") || line.starts_with("call "))
                && line.contains("__rt_stream_registry_retire")
        }).count(), 2, "{name}: both tables call shared retirement");
        let clear = if name == "linux-x86_64" { "mov QWORD PTR [rax + 24], 0" } else { "str xzr, [x0, #24]" };
        assert!(assembly.find(clear).unwrap() < assembly.find("__rt_heap_free_safe").unwrap(), "{name}: retire before free");
    }
}
