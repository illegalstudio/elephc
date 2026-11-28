//! Purpose:
//! Ends source binding metadata with the active native or interpreter scope layout.
//!
//! Called from:
//! - Shared eval-scope runtime emission for typed EIR binding retirement.
//!
//! Key details:
//! - Native scopes retire registry origins without interpreting their header as Rust storage.
//! - Contained destructors return to native code before their owned Throwable propagates.

use crate::codegen::{abi, emit::Emitter};

/// Selects the existing protected scope implementation without changing its ABI or ownership.
pub(super) fn emit(emitter: &mut Emitter, interpreter_scope: bool) {
    emitter.label_global("__rt_eval_scope_end_binding");
    if interpreter_scope {
        abi::emit_jump(emitter, &emitter.target.extern_symbol("__elephc_eval_scope_unset"));
        return;
    }
    let result = abi::int_result_reg(emitter);
    let done = "__rt_eval_scope_end_binding_done";
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("__elephc_eval_scope_origin_unset_v1"));
    abi::emit_branch_if_zero_wide(emitter, result, done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_jump(emitter, "__rt_throw_boxed_destructor_exception");
    emitter.label(done);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Every supported ABI selects the scope's actual layout and contains native-origin failures.
    #[test]
    fn source_binding_retirement_scope_layouts_emit_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for interpreter in [false, true] {
                let target = Target::parse(name).unwrap();
                let mut emitter = Emitter::new(target);
                emit(&mut emitter, interpreter);
                let assembly = emitter.output();
                assert!(assembly.contains("__rt_eval_scope_end_binding"), "{name}");
                let expected = if interpreter { "__elephc_eval_scope_unset" }
                    else { "__elephc_eval_scope_origin_unset_v1" };
                assert!(assembly.contains(&target.extern_symbol(expected)), "{name}: {assembly}");
                assert_eq!(assembly.contains("__rt_throw_boxed_destructor_exception"), !interpreter, "{name}");
            }
        }
    }
}
