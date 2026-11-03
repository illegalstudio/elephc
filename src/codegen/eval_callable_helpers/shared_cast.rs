//! Purpose:
//! Shares eval callback conversion across native method and constructor argument slots.
//!
//! Called from:
//! - Eval callable support emission and the argument casters in native eval bridges.
//!
//! Key details:
//! - The helper borrows an explicit eval context and boxed callback through the C argument ABI.
//! - Existing descriptor selection and dynamic fallback retain their ownership conventions.
//! - A zero descriptor reports the caller's existing unsupported path without unwinding.

use super::*;
use crate::codegen_support::platform::Arch;

const ENTRY: &str = "__elephc_eval_resolve_callable_arg";
pub(super) const CONTEXT_SLOT: usize = 8;
const FRAME: usize = 64;

/// Emits one context-bearing resolver with the frame homes expected by the shared selectors.
pub(super) fn emit(
    module: &Module, emitter: &mut Emitter, data: &mut DataSection,
    support: &EvalCallableDescriptorSupport,
) {
    let result = abi::int_result_reg(emitter);
    let failed = "__elephc_eval_resolve_callable_arg_failed";
    let done = "__elephc_eval_resolve_callable_arg_return";
    emitter.label_global(ENTRY);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT_SLOT);
    let value_slot = if emitter.target.arch == Arch::AArch64 { 16 } else { 40 };
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), value_slot);
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64_cast_eval_callable_arg_inline(
            module, emitter, data, support, ENTRY, failed,
        ),
        Arch::X86_64 => emit_x86_64_cast_eval_callable_arg_inline(
            module, emitter, data, support, ENTRY, failed, CONTEXT_SLOT,
        ),
    }
    abi::emit_jump(emitter, done);
    emitter.label(failed);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Passes the ARM64 bridge's existing context and argument homes to the module resolver.
pub(in crate::codegen) fn emit_aarch64_cast_eval_callable_arg(
    _module: &Module, emitter: &mut Emitter, _data: &mut DataSection,
    support: &EvalCallableDescriptorSupport, _label_prefix: &str, fail_label: &str,
) {
    if support.is_empty() && !support.has_dynamic_descriptor() {
        abi::emit_jump(emitter, fail_label);
        return;
    }
    abi::emit_load_from_address(emitter, "x0", "x29", AARCH64_EVAL_CONTEXT_FROM_FP_OFFSET as usize);
    abi::load_at_offset(emitter, "x1", 16);
    abi::emit_call_label(emitter, ENTRY);
    abi::emit_branch_if_int_result_zero(emitter, fail_label);
}

/// Passes the System V bridge's context and argument homes to the same module resolver.
pub(in crate::codegen) fn emit_x86_64_cast_eval_callable_arg(
    _module: &Module, emitter: &mut Emitter, _data: &mut DataSection,
    support: &EvalCallableDescriptorSupport, _label_prefix: &str, fail_label: &str,
    context_frame_offset: usize,
) {
    if support.is_empty() && !support.has_dynamic_descriptor() {
        abi::emit_jump(emitter, fail_label);
        return;
    }
    abi::load_at_offset(emitter, "rdi", context_frame_offset);
    abi::load_at_offset(emitter, "rsi", 40);
    abi::emit_call_label(emitter, ENTRY);
    abi::emit_branch_if_int_result_zero(emitter, fail_label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Constructor and method sites pass their own contexts without copying descriptor tables.
    #[test]
    fn eval_callable_bridge_casters_share_one_context_bearing_resolver_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let support = EvalCallableDescriptorSupport {
                argument_normalizer_needed: false, string_cases: Vec::new(),
                instance_array_cases: Vec::new(), static_array_cases: Vec::new(),
                object_cases: vec![EvalInstanceMethodCallableCase {
                    class_id: 1, method_name: "__invoke".into(),
                    case: RuntimeCallableCase {
                        label: "test_native_invoke_entry".into(),
                        descriptor_label: "test_native_invokable".into(), php_name: Some("Native::__invoke".into()),
                    },
                }], dynamic_descriptor_label: Some("test_eval_callback_template".into()),
            };
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            for offset in [64, 72] {
                match target.arch {
                    Arch::AArch64 => emit_aarch64_cast_eval_callable_arg(
                        &module, &mut emitter, &mut data, &support, "site", "site_failed",
                    ),
                    Arch::X86_64 => emit_x86_64_cast_eval_callable_arg(
                        &module, &mut emitter, &mut data, &support, "site", "site_failed", offset,
                    ),
                }
            }
            let sites = emitter.output();
            assert_eq!(sites.matches(ENTRY).count(), 2, "{name}: share both argument conversions");
            assert!(!sites.contains("__rt_mixed_unbox") && !sites.contains("__rt_strcasecmp"),
                "{name}: call sites contain no candidate tables");
            let mut emitter = Emitter::new(target);
            emit(&module, &mut emitter, &mut data, &support);
            let body = emitter.output();
            assert_eq!(body.matches(&format!("{ENTRY}:\n")).count(), 1, "{name}: one global helper");
            let object_done = body.find(&format!("{ENTRY}_callable_object_done:")).unwrap();
            let dynamic = body.find(&format!("{ENTRY}_callable_dynamic:")).unwrap();
            assert!(body[object_done..dynamic].contains(&format!("{ENTRY}_callable_cast_done")),
                "{name}: a selected native object must not fall through into dynamic allocation");
            assert!(body.contains(&target.extern_symbol("__elephc_eval_is_callable")),
                "{name}: preserve dynamic eval callback validation");
        }
    }
}
