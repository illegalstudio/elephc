//! Purpose:
//! Emits cleanup callbacks and normal-path removal for owned-value exception guards.
//!
//! Called from:
//! - The common exception runtime emitter and typed EIR ownership calls.
//!
//! Key details:
//! - Guards use the existing three-word activation prefix followed by one heap owner.
//! - Removing any active guard preserves newer guards, including reordered named arguments.
//! - Cleanup clears the transferred owner before invoking potentially reentrant destruction.
//! - Failed construction marks the object so its destructor stays suppressed after escape.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::{EXCEPTION_GUARD_OWNER_OFFSET, TRY_HANDLER_SLOT_SIZE};
use elephc_memoir::{FrameOffset, GuardLayout, RecordOffset};

/// Pairs the guard owner with the protected callback's retained first argument.
const GUARD_LAYOUT: GuardLayout = GuardLayout::new(
    RecordOffset::new(EXCEPTION_GUARD_OWNER_OFFSET), FrameOffset::new(TRY_HANDLER_SLOT_SIZE));

/// Emits target-aware removal and cleanup entries for stack-local ownership guards.
pub(super) fn emit(emitter: &mut Emitter) {
    elephc_memoir::guarded_owners::emit_guard_chain(emitter, GUARD_LAYOUT, "_exc_call_frame_top");
    super::emit_protected(emitter, "__rt_exception_release_owned", release_body);
    super::emit_protected(emitter, "__rt_exception_release_callable", release_callable_body);
    super::emit_protected(
        emitter,
        "__rt_exception_release_unconstructed_object",
        release_unconstructed_object_body,
    );
    super::emit_protected(
        emitter,
        "__rt_exception_release_wrapper_fd",
        release_wrapper_fd_body,
    );
}

/// Closes a guarded synthetic user-wrapper descriptor during exception unwinding.
///
/// `file_get_contents()` opens a wrapper through `__rt_fopen` and normally closes it with
/// `__rt_user_wrapper_fclose`; a throwing `stream_read()`/`stream_eof()` would otherwise
/// abandon the handle slot. The guard stores the synthetic fd as its owner, and this
/// protected callback clears it before invoking `stream_close` so a nested throw cannot
/// close the same descriptor twice.
fn release_wrapper_fd_body(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("ldr x9, [sp, #224]");                              // recover the guard record passed as the C argument
        emitter.instruction("ldr x0, [x9, #24]");                               // load the guarded synthetic descriptor
        emitter.instruction("str xzr, [x9, #24]");                              // clear ownership before stream_close can throw
        emitter.instruction("cbz x0, __rt_exception_release_wrapper_fd_skip");  // an already-cleared guard has nothing to close
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 224]");                  // recover the guard record passed as the C argument
        emitter.instruction("mov rax, QWORD PTR [r10 + 24]");                   // load the guarded synthetic descriptor
        emitter.instruction("mov QWORD PTR [r10 + 24], 0");                     // clear ownership before stream_close can throw
        emitter.instruction("test rax, rax");                                   // has the guard already been consumed?
        emitter.instruction("jz __rt_exception_release_wrapper_fd_skip");       // an already-cleared guard has nothing to close
        emitter.instruction("mov rdi, rax");                                    // pass the synthetic descriptor to the close helper
    }
    abi::emit_call_label(emitter, "__rt_user_wrapper_fclose");
    emitter.label("__rt_exception_release_wrapper_fd_skip");
}

/// Consumes the guard's owner while the shared protected callback restores handler and GC state.
fn release_body(emitter: &mut Emitter) {
    emit_release_body(emitter, "__rt_decref_any");
}

/// Releases callable descriptors and their captures through the dedicated ownership protocol.
fn release_callable_body(emitter: &mut Emitter) {
    emit_release_body(emitter, "__rt_callable_descriptor_release");
}

/// Suppresses `__destruct` before releasing a raw object whose constructor did not complete.
fn release_unconstructed_object_body(emitter: &mut Emitter) {
    elephc_memoir::guarded_owners::emit_take_guard_owner(emitter, GUARD_LAYOUT);
    elephc_memoir::values::destructor_header::emit_suppress_unconstructed(
        emitter, "__rt_exception_release_unconstructed_object_drop",
    );
    abi::emit_call_label(emitter, "__rt_decref_any");
}

/// Adapts protected callback layout facts to the shared take-before-release primitive.
fn emit_release_body(emitter: &mut Emitter, symbol: &str) {
    elephc_memoir::guarded_owners::emit_release_body(emitter, symbol, GUARD_LAYOUT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};

    /// Every target marks failed construction before releasing the guard's raw object owner.
    #[test]
    fn unconstructed_object_cleanup_suppresses_destructor_on_all_targets() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let arch = emitter.target.arch;
            emit(&mut emitter);
            let assembly = emitter.output();
            let cleanup = assembly
                .split_once("__rt_exception_release_unconstructed_object:")
                .unwrap()
                .1;
            let mark = if arch == Arch::AArch64 {
                "orr x10, x10, #0x4000"
            } else {
                "or QWORD PTR [rax - 8], 0x4000"
            };

            assert!(cleanup.find(mark).unwrap() < cleanup.find("__rt_decref_any").unwrap(), "{name}");
            assert_eq!(cleanup.matches("__rt_decref_any").count(), 1, "{name}");
        }
    }

    /// The wrapper-descriptor cleanup callback closes exactly the guarded synthetic fd.
    #[test]
    fn wrapper_fd_cleanup_is_emitted_once_per_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            assert_eq!(
                assembly.matches("__rt_exception_release_wrapper_fd_skip:").count(),
                1,
                "{name}"
            );
            assert_eq!(assembly.matches("__rt_user_wrapper_fclose").count(), 1, "{name}");
        }
    }
}
