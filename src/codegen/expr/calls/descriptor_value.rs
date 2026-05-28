//! Purpose:
//! Materializes callable descriptor values for closures and first-class callables.
//! Handles static descriptor records, generated runtime invokers, and runtime
//! capture slots for callable environments.
//!
//! Called from:
//! - `crate::codegen::expr::calls::closure`
//! - `crate::codegen::expr::calls::first_class`
//!
//! Key details:
//! - Runtime descriptors preserve the static descriptor header, then append
//!   16-byte capture value slots consumed by uniform descriptor invokers.

use crate::codegen::callable_descriptor::{
    self, CallableDescriptorInvocation,
};
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, callable_dispatch};
use crate::types::{FunctionSig, PhpType};

use super::args;

/// Emits a callable descriptor value, allocating runtime capture storage when needed.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_callable_descriptor_value(
    entry_label: &str,
    php_name: Option<&str>,
    kind: u64,
    sig: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    hidden_params: &[(String, PhpType, bool)],
    invocation: CallableDescriptorInvocation,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let invoker_label = callable_dispatch::ensure_runtime_descriptor_invoker(ctx, hidden_params, sig);
    let descriptor_label = callable_descriptor::static_descriptor_with_optional_invoker_meta(
        data,
        entry_label,
        php_name,
        kind,
        Some(sig),
        captures,
        hidden_params,
        invocation,
        invoker_label.as_deref(),
    );

    if captures.is_empty() {
        abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &descriptor_label);
        return;
    }

    emit_runtime_descriptor_with_captures(
        &descriptor_label,
        captures,
        emitter,
        ctx,
    );
}

/// Allocates a runtime descriptor, copies the static header, and stores capture values.
fn emit_runtime_descriptor_with_captures(
    descriptor_label: &str,
    captures: &[(String, PhpType, bool)],
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let descriptor_reg = abi::nested_call_reg(emitter);
    let total_bytes =
        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + captures.len() * 16;

    emitter.comment("callable descriptor: allocate runtime capture storage");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), total_bytes as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov {}, {}", descriptor_reg, abi::int_result_reg(emitter))); // keep the runtime descriptor pointer while storing captures
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        emitter,
        descriptor_reg,
        descriptor_label,
    );

    for (idx, (capture_name, capture_ty, by_ref)) in captures.iter().enumerate() {
        emitter.comment(&format!("callable descriptor: store capture ${}", capture_name));
        if matches!(capture_ty.codegen_repr(), PhpType::Callable) {
            ctx.mark_fcc_used(capture_name);
        }
        if *by_ref {
            if !args::emit_ref_arg_variable_address(
                capture_name,
                "callable descriptor capture ref",
                emitter,
                ctx,
            ) {
                emitter.comment(&format!(
                    "WARNING: captured callable variable ${} not found",
                    capture_name
                ));
                continue;
            }
            callable_descriptor::emit_store_current_result_to_runtime_capture(
                emitter,
                descriptor_reg,
                idx,
                &PhpType::Int,
            );
            continue;
        }

        let Some(capture_info) = ctx.variables.get(capture_name) else {
            emitter.comment(&format!(
                "WARNING: captured callable variable ${} not found",
                capture_name
            ));
            continue;
        };
        abi::emit_load(emitter, capture_ty, capture_info.stack_offset);
        if matches!(capture_ty.codegen_repr(), PhpType::Str) {
            abi::emit_call_label(emitter, "__rt_str_persist");
            callable_descriptor::emit_store_current_result_to_runtime_capture(
                emitter,
                descriptor_reg,
                idx,
                capture_ty,
            );
            continue;
        }
        callable_descriptor::emit_store_current_result_to_runtime_capture(
            emitter,
            descriptor_reg,
            idx,
            capture_ty,
        );
        retain_runtime_capture_result(emitter, capture_ty);
    }

    if descriptor_reg != abi::int_result_reg(emitter) {
        emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), descriptor_reg)); // return the runtime callable descriptor pointer
    }
}

/// Retains a by-value capture stored in a runtime descriptor.
fn retain_runtime_capture_result(emitter: &mut Emitter, capture_ty: &PhpType) {
    match capture_ty.codegen_repr() {
        PhpType::Str => {}
        PhpType::Callable => {
            callable_descriptor::emit_retain_current_descriptor(emitter);
        }
        other if other.is_refcounted() => {
            abi::emit_incref_if_refcounted(emitter, &other);
        }
        _ => {}
    }
}
