//! Purpose:
//! Adapts throwing eval scope mutation and teardown results after their Rust frames return.
//!
//! Called from:
//! - The shared eval bridge emitter for every supported target.
//!
//! Key details:
//! - Generated callers retain the scope API; Rust exports versioned exception-returning entries.
//! - Native throws happen only after Rust has committed scope changes and finished releasing owners.

use super::{abi, label_c_global, Emitter};

const FRAME: usize = 80;
const THROWN: usize = 48;
const STATUS: usize = 56;

/// Emits native adapters for the three scope operations that can invoke PHP destructors.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_operation(emitter, "__elephc_eval_scope_set", Some(5));
    emit_operation(emitter, "__elephc_eval_scope_unset", Some(3));
    emit_operation(emitter, "__elephc_eval_scope_free", None);
}

/// Preserves ordinary statuses and consumes an escaping boxed Throwable after a versioned Rust call.
fn emit_operation(emitter: &mut Emitter, entry: &str, output_index: Option<usize>) {
    label_c_global(emitter, entry);
    abi::emit_frame_prologue(emitter, FRAME);
    let result = abi::int_result_reg(emitter);
    if let Some(index) = output_index {
        let scratch = abi::secondary_scratch_reg(emitter);
        abi::emit_load_int_immediate(emitter, scratch, 0);
        abi::store_at_offset(emitter, scratch, THROWN);
        abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, index), THROWN);
    }
    abi::emit_call_label(emitter, &emitter.target.extern_symbol(&format!("{entry}_v2")));
    if output_index.is_some() {
        abi::store_at_offset(emitter, result, STATUS);
        abi::load_at_offset(emitter, result, THROWN);
    }
    let throw_label = format!("{entry}_propagate");
    abi::emit_branch_if_int_result_nonzero(emitter, &throw_label);
    if output_index.is_some() {
        abi::load_at_offset(emitter, result, STATUS);
    }
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emitter.label(&throw_label);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_boxed_destructor_exception");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Scope mutation and free return from Rust before any native throw on every supported ABI.
    #[test]
    fn scope_release_adapters_propagate_only_after_the_rust_call_returns() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for (entry, output) in [("__elephc_eval_scope_set", Some(5)), ("__elephc_eval_scope_unset", Some(3)), ("__elephc_eval_scope_free", None)] {
                let mut emitter = Emitter::new(target);
                emit_operation(&mut emitter, entry, output);
                let asm = emitter.output();
                let call = asm.find(&target.extern_symbol(&format!("{entry}_v2"))).unwrap();
                assert!(call < asm.find("__rt_throw_boxed_destructor_exception").unwrap(), "{name}: {entry}");
                assert!(asm.contains(&format!("{}:", target.extern_symbol(entry))), "{name}: {entry}");
                assert_eq!(asm.matches("__rt_throw_boxed_destructor_exception").count(), 1, "{name}: {entry}");
            }
        }
    }
}
