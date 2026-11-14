//! Purpose:
//! Contains explicit checked numeric callbacks beneath the Magician Rust stack.
//!
//! Called from:
//! - The shared eval bridge emitter on every supported target.
//!
//! Key details:
//! - Version one borrows two cells and transfers one owner through the fourth C argument.
//! - Unsupported selectors and runtimes return status one without touching the output.

use super::{abi, Arch, Emitter};
use crate::codegen_support::runtime::exceptions::emit_protected_status;
use crate::codegen_support::try_handlers::TRY_HANDLER_SLOT_SIZE;
use elephc_semantics::{numeric::CheckedNumericOp, StrictFeature};

/// Emits an always-available symbol whose capabilities match the immutable linked runtime.
pub(super) fn emit(emitter: &mut Emitter) {
    let symbol = emitter.target.extern_symbol("__elephc_eval_checked_numeric_v1");
    emit_protected_status(emitter, &symbol, body);
}

/// Dispatches only reviewed checked entries and publishes a result after normal completion.
fn body(emitter: &mut Emitter) {
    let prefix = "__rt_eval_checked_numeric_v1";
    let result = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);
    let right = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    let done = format!("{prefix}_done");
    let success = format!("{prefix}_success");
    let supported = emitter.semantic_profile.contains(StrictFeature::Overflow);
    let operations = [
        (CheckedNumericOp::Add, "__rt_strict_mixed_numeric_add"),
        (CheckedNumericOp::Sub, "__rt_strict_mixed_numeric_sub"),
        (CheckedNumericOp::Mul, "__rt_strict_mixed_numeric_mul"),
        (CheckedNumericOp::Pow, "__rt_strict_mixed_numeric_pow"),
        (CheckedNumericOp::Div, "__rt_strict_mixed_numeric_div"),
        (CheckedNumericOp::Mod, "__rt_strict_mixed_numeric_mod"),
        (CheckedNumericOp::Abs, "__rt_strict_abs_mixed"),
        (CheckedNumericOp::ArraySum, "__rt_strict_array_sum_boxed"),
        (CheckedNumericOp::ArrayProduct, "__rt_strict_array_product_boxed"),
        (CheckedNumericOp::Increment, "__rt_strict_mixed_inc_dec"),
        (CheckedNumericOp::Decrement, "__rt_strict_mixed_inc_dec"),
    ];
    if supported {
        abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE);
        for (op, _) in operations {
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction(&format!("cmp {result}, #{}", op as u64)), // match the explicit versioned selector
                Arch::X86_64 => emitter.instruction(&format!("cmp {result}, {}", op as u64)), // match the explicit versioned selector
            }
            abi::emit_branch_if_equal_wide(emitter, &format!("{prefix}_{}", op as u64));
        }
    }
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, &done);
    if supported {
        for (op, target) in operations {
            emitter.label(&format!("{prefix}_{}", op as u64));
            if matches!(op, CheckedNumericOp::Increment | CheckedNumericOp::Decrement) {
                borrowed_update(emitter, op);
            } else {
                abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
                abi::emit_load_temporary_stack_slot(emitter, right, TRY_HANDLER_SLOT_SIZE + 16);
            }
            abi::emit_call_label(emitter, target);
            if matches!(op, CheckedNumericOp::ArraySum | CheckedNumericOp::ArrayProduct) {
                abi::emit_branch_if_int_result_nonzero(emitter, &success);
                abi::emit_call_label(emitter, crate::codegen_support::runtime::arrays::array_numeric_source::symbol(
                    op == CheckedNumericOp::ArrayProduct));
            }
            abi::emit_jump(emitter, &success);
        }
        emitter.label(&success);
        abi::emit_load_temporary_stack_slot(emitter, scratch, TRY_HANDLER_SLOT_SIZE + 24);
        abi::emit_store_to_address(emitter, result, scratch, 0);
        abi::emit_load_int_immediate(emitter, result, 0);
    }
    emitter.label(&done);
}

/// Copies a canonical borrowed value into headerless spills so unique-cell updates cannot mutate it.
fn borrowed_update(emitter: &mut Emitter, op: CheckedNumericOp) {
    let result = abi::int_result_reg(emitter);
    let (low, high) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rdx") };
    abi::emit_load_temporary_stack_slot(emitter, result, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for (register, offset) in [(result, 0), (low, 8), (high, 16)] {
        abi::emit_store_to_sp(emitter, register, TRY_HANDLER_SLOT_SIZE + offset);
    }
    if op == CheckedNumericOp::Decrement
        && crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php83 {
        let ready = "__rt_eval_checked_numeric_v1_decrement_ready";
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("cmp x0, #8"),                 // null decrement diagnoses a no-op without entering Rust
            Arch::X86_64 => emitter.instruction("cmp rax, 8"),                  // null decrement diagnoses a no-op without entering Rust
        }
        abi::emit_branch_if_not_equal_wide(emitter, ready);
        abi::emit_symbol_address(emitter, if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" },
            "_strict_eval_null_decrement_warning");
        abi::emit_load_int_immediate(emitter, if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" },
            elephc_semantics::numeric::NULL_DECREMENT_WARNING.len() as i64);
        abi::emit_call_label(emitter, "__rt_diag_warning");
        emitter.label(ready);
    }
    abi::emit_sp_address(emitter, result, TRY_HANDLER_SLOT_SIZE);
    abi::emit_load_int_immediate(emitter, low, if op == CheckedNumericOp::Increment { 1 } else { -1 });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use elephc_semantics::SemanticProfile;

    /// Every target returns native exceptions, retains PHP helpers and rejects unsupported runtimes.
    #[test]
    fn strict_eval_numeric_boundary_contains_all_checked_kernels() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                emit(&mut emitter);
                let symbol = emitter.target.extern_symbol("__elephc_eval_checked_numeric_v1");
                let asm = emitter.output();
                assert!(asm.contains(&symbol), "{name}");
                assert!(asm.contains("_exc_handler_top") && asm.contains("_exc_call_frame_top"), "{name}");
                assert!(!asm.contains("__rt_throw_current"), "{name}: native throws must return to Rust");
                for entry in ["add", "sub", "mul", "pow", "div", "mod"] {
                    assert_eq!(asm.contains(&format!("__rt_strict_mixed_numeric_{entry}")), checked, "{name}");
                }
                assert_eq!(asm.contains("__rt_strict_abs_mixed"), checked, "{name}");
                assert_eq!(asm.matches("__rt_strict_mixed_inc_dec").count(), if checked { 2 } else { 0 }, "{name}");
                if checked {
                    assert!(asm.contains("_strict_eval_null_decrement_warning"), "{name}");
                    let address = if name.ends_with("x86_64") { "lea rax, [rsp + 224]" } else { "add x0, x0, #224" };
                    assert_eq!(asm.matches(address).count(), 2, "{name}: updates must lend headerless value copies");
                }
                for op in ["sum", "product"] {
                    assert_eq!(asm.contains(&format!("__rt_strict_array_{op}_boxed")), checked, "{name}");
                }
            }
        }
    }

    /// The checked null update retains the selected PHP version's diagnostic behavior.
    #[test]
    fn strict_eval_diagnostics_null_decrement_respects_php_version() {
        struct Restore(crate::php_version::PhpVersion, bool);
        impl Drop for Restore {
            /// Restores this test thread's compilation policy after success or assertion failure.
            fn drop(&mut self) { crate::codegen_support::set_compile_profile(self.0, self.1); }
        }
        let _restore = Restore(crate::codegen_support::compile_php_version(), crate::codegen_support::compile_is_web_sapi());
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for (version, warns) in [(crate::php_version::PhpVersion::Php82, false), (crate::php_version::PhpVersion::Php83, true)] {
                crate::codegen_support::set_compile_profile(version, false);
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
                emit(&mut emitter);
                let asm = emitter.output();
                assert_eq!(asm.contains("_strict_eval_null_decrement_warning"), warns, "{name}, {version:?}");
            }
        }
    }

    /// AOT known-null updates and eval callbacks agree on the PHP 8.3 warning boundary.
    #[test]
    fn strict_eval_diagnostics_aot_null_decrement_respects_php_version() {
        struct Restore(crate::php_version::PhpVersion, bool);
        impl Drop for Restore {
            /// Restores compilation context even if a target/profile assertion fails.
            fn drop(&mut self) { crate::codegen_support::set_compile_profile(self.0, self.1); }
        }
        let _restore = Restore(crate::codegen_support::compile_php_version(), crate::codegen_support::compile_is_web_sapi());
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for (version, warns) in [(crate::php_version::PhpVersion::Php82, false), (crate::php_version::PhpVersion::Php83, true)] {
                crate::codegen_support::set_compile_profile(version, false);
                let target = Target::parse(name).unwrap();
                let program = crate::parser::parse(&crate::lexer::tokenize("<?php $value = null; $value--;").unwrap()).unwrap();
                let mut checked = crate::types::check_with_target(&program, target).unwrap();
                checked.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true);
                let module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
                let main = module.functions.iter().find(|function| function.name == "main").unwrap();
                let count = main.instructions.iter().filter(|inst| matches!(inst.immediate,
                    Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::WriteDiagnostic)))).count();
                assert_eq!(count, usize::from(warns), "{name}, {version:?}");
            }
        }
    }

}


#[cfg(test)]
mod aggregate_source_tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use elephc_semantics::SemanticProfile;

    /// Every target refuses a zero aggregate result before publishing any output pointer.
    #[test]
    fn strict_eval_aggregate_sources_guard_success_publication_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for checked in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, checked);
                emit(&mut emitter);
                let messages = crate::codegen_support::runtime::arrays::array_numeric_source::data(emitter.semantic_profile);
                let asm = emitter.output();
                for (selector, product) in [(CheckedNumericOp::ArraySum, false), (CheckedNumericOp::ArrayProduct, true)] {
                    let symbol = crate::codegen_support::runtime::arrays::array_numeric_source::symbol(product);
                    assert_eq!(asm.contains(symbol), checked, "{name}");
                    assert_eq!(messages.contains(crate::codegen_support::runtime::arrays::numeric_aggregate_source_message(product)), checked);
                    if checked {
                        let start = asm.find(&format!("__rt_eval_checked_numeric_v1_{}:", selector as u64)).unwrap();
                        let failure = asm[start..].find(symbol).unwrap();
                        let success = asm[start..].find("__rt_eval_checked_numeric_v1_success").unwrap();
                        assert!(success < failure, "{name}: a nonzero result bypasses the source failure");
                    }
                }
            }
        }
    }
}
