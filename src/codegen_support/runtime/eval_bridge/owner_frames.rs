//! Purpose:
//! Links pure Rust temporary-owner visitors into native coroutine activation chains.
//!
//! Called from:
//! - Magician runtime hooks while a Rust-owned cell can survive native suspension.
//!
//! Key details:
//! - Rust pins each complete 48-byte record on the current coroutine stack.
//! - Rust retains cleanup responsibility; native visitors only borrow its real pointer fields.
//! - Memoir initializes and publishes records; versioned C ABI adaptation stays here.
//! - Detachment uses the shared exact-record splice and never releases PHP values.

use super::*;

/// Emits the versioned C ABI for pinned Rust activations and their pure slot visitor.
pub(super) fn emit(emitter: &mut Emitter) {
    let record = abi::int_arg_reg_name(emitter.target, 0);
    let visitor = abi::int_arg_reg_name(emitter.target, 1);
    let data = abi::int_arg_reg_name(emitter.target, 2);
    label_c_global(emitter, "__elephc_eval_owner_frame_enter_v1");
    elephc_memoir::owner_records::publish_external(emitter,
        elephc_memoir::owner_records::ExternalRecord {
            record_address: record, visitor, data,
            head_symbol: "_exc_call_frame_top", cleanup_symbol: "__rt_eval_owner_frame_noop",
        });
    abi::emit_return(emitter);

    label_c_global(emitter, "__elephc_eval_owner_frame_leave_v1");
    abi::emit_unlink_call_operand_owner_at_address(emitter, record);
    abi::emit_return(emitter);

    label_c_global(emitter, "__elephc_eval_owner_frame_slot_v1");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 2), visitor);
    abi::emit_load_int_immediate(emitter, visitor, 0);
    abi::emit_jump(emitter, "__rt_gc_frame_owner_slot");

    // Protected native calls return through Rust, which clears and releases its own cell.
    // This callback provides a complete activation prefix without duplicating that owner.
    emitter.label_global("__rt_eval_owner_frame_noop");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target publishes the full extended activation and forwards actual slot identity.
    #[test]
    fn rust_owner_frames_publish_complete_pure_activations_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            for entry in ["enter", "leave", "slot"] {
                assert!(assembly.contains(&target.extern_symbol(&format!("__elephc_eval_owner_frame_{entry}_v1"))), "{name}: {entry}");
            }
            let published = assembly.find(if target.arch == Arch::X86_64 {
                "mov QWORD PTR [rdi + 40], rsi"
            } else { "str x1, [x0, #40]" }).unwrap();
            let cleanup = assembly[published..].find("__rt_eval_owner_frame_noop").unwrap() + published;
            assert!(published < cleanup, "{name}: initialize the visitor before exposing the record");
            assert!(assembly.contains("965:"), "{name}: preserve newer activation records during detachment");
            assert!(assembly.contains("__rt_gc_frame_owner_slot"), "{name}: use the collector's real slot identity");
            // A call would run PHP, which can re-enter the collector while this record is live.
            // The x86_64 SysV shim's `call` to its own local body is the one exception: it is the
            // wrapper that makes the Rust-called export safe, not PHP work.
            let php_calls: Vec<&str> = assembly
                .lines()
                .map(str::trim_start)
                .filter(|line| line.starts_with("bl ") || line.starts_with("call "))
                .filter(|line| !line.ends_with("_sysv_body"))
                .collect();
            assert!(php_calls.is_empty(), "{name}: linking, unlinking and visiting cannot run PHP: {php_calls:?}");
        }
    }
}
