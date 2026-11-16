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
    super::argument_owners::keep(emitter, 64);
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
    super::argument_owners::keep(emitter, context_frame_offset);
}

/// Casts an eval argument for a native parameter declaring `callable` on ARM64.
///
/// The parameter stores the boxed PHP value. A natively callable value is passed as it is, so
/// the callee keeps the string, array or object it was given. A value only eval can resolve
/// (a function declared inside `eval()`) is converted by the shared resolver into a descriptor
/// that is boxed into a fresh `Mixed` cell the bridge owns. Anything else is passed unchanged
/// too: the callee's entry check accepts it for another union member (`int|callable`, `null`)
/// or raises PHP's `must be of type callable, T given` `TypeError`.
pub(in crate::codegen) fn emit_aarch64_cast_eval_declared_callable_arg(
    _module: &Module, emitter: &mut Emitter, _data: &mut DataSection,
    support: &EvalCallableDescriptorSupport, label_prefix: &str, _fail_label: &str,
) {
    let keep = format!("{label_prefix}_declared_callable_keep");
    let done = format!("{label_prefix}_declared_callable_ready");
    // -- a natively callable value keeps its PHP value --
    abi::load_at_offset(emitter, "x0", 16);
    abi::emit_call_label(emitter, "__rt_is_callable_mixed");
    abi::emit_branch_if_int_result_nonzero(emitter, &keep);
    if !support.is_empty() || support.has_dynamic_descriptor() {
        // -- an eval-only callback becomes a boxed descriptor; an unresolved value stays --
        abi::emit_load_from_address(emitter, "x0", "x29", AARCH64_EVAL_CONTEXT_FROM_FP_OFFSET as usize);
        abi::load_at_offset(emitter, "x1", 16);
        abi::emit_call_label(emitter, ENTRY);
        abi::emit_branch_if_int_result_zero(emitter, &keep);
        super::argument_owners::keep(emitter, 64);
        crate::codegen::emit_box_current_value_as_mixed(emitter, &crate::types::PhpType::Callable);
        super::argument_owners::keep_mixed(emitter, 64);
        abi::emit_jump(emitter, &done);
    }
    emitter.label(&keep);
    abi::load_at_offset(emitter, "x0", 16);
    emitter.label(&done);
}

/// Casts an eval argument for a native parameter declaring `callable` on x86_64.
///
/// Mirrors the ARM64 caster: native callables and unresolved values pass unchanged, and only
/// an eval-only callback is converted into a boxed descriptor the bridge owns.
pub(in crate::codegen) fn emit_x86_64_cast_eval_declared_callable_arg(
    _module: &Module, emitter: &mut Emitter, _data: &mut DataSection,
    support: &EvalCallableDescriptorSupport, label_prefix: &str, _fail_label: &str,
    context_frame_offset: usize,
) {
    let keep = format!("{label_prefix}_declared_callable_keep");
    let done = format!("{label_prefix}_declared_callable_ready");
    // -- a natively callable value keeps its PHP value --
    abi::load_at_offset(emitter, "rdi", 40);
    abi::emit_call_label(emitter, "__rt_is_callable_mixed");
    abi::emit_branch_if_int_result_nonzero(emitter, &keep);
    if !support.is_empty() || support.has_dynamic_descriptor() {
        // -- an eval-only callback becomes a boxed descriptor; an unresolved value stays --
        abi::load_at_offset(emitter, "rdi", context_frame_offset);
        abi::load_at_offset(emitter, "rsi", 40);
        abi::emit_call_label(emitter, ENTRY);
        abi::emit_branch_if_int_result_zero(emitter, &keep);
        super::argument_owners::keep(emitter, context_frame_offset);
        crate::codegen::emit_box_current_value_as_mixed(emitter, &crate::types::PhpType::Callable);
        super::argument_owners::keep_mixed(emitter, context_frame_offset);
        abi::emit_jump(emitter, &done);
    }
    emitter.label(&keep);
    abi::load_at_offset(emitter, "rax", 40);
    emitter.label(&done);
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
    /// Shared callable arguments reach the array and invokable-object lookups through
    /// unconditional AArch64 branches.
    ///
    /// The string-name descriptor lookup emitted between the tag ladder and those lookups grows
    /// with the program's callable names, so a `b.eq` across it can leave the ±1 MiB
    /// conditional-branch range (#1444). x86_64 keeps its rel32 `je`.
    #[test]
    fn shared_callable_argument_ladder_widens_aarch64_branches() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = Module::new(target);
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let support = EvalCallableDescriptorSupport {
                argument_normalizer_needed: false, string_cases: Vec::new(),
                instance_array_cases: Vec::new(), static_array_cases: Vec::new(),
                object_cases: Vec::new(), dynamic_descriptor_label: Some("test_dynamic_template".into()),
            };
            emit(&module, &mut emitter, &mut data, &support);
            let asm = emitter.output();
            let lines = asm.lines().map(str::trim).collect::<Vec<_>>();
            for (suffix, sites) in [("_callable_array", 2), ("_callable_object", 1)] {
                let reaches = |line: &str, mnemonic: &str| {
                    line.strip_prefix(mnemonic).is_some_and(|label| label.ends_with(suffix))
                };
                match target.arch {
                    Arch::AArch64 => {
                        assert!(
                            !lines.iter().any(|line| reaches(line, "b.eq ")),
                            "{name}: near branch to {suffix}:\n{asm}"
                        );
                        let wide = lines
                            .windows(2)
                            .filter(|pair| pair[0] == "b.ne 1f" && reaches(pair[1], "b "))
                            .count();
                        assert_eq!(wide, sites, "{name}: wide branches to {suffix}:\n{asm}");
                    }
                    Arch::X86_64 => {
                        let near = lines.iter().filter(|line| reaches(line, "je ")).count();
                        assert_eq!(near, sites, "{name}: je branches to {suffix}:\n{asm}");
                    }
                }
            }
        }
    }


}
