//! Purpose:
//! Describes and validates the boundary between fixed stream-wrapper runtime callbacks and PHP methods.
//! Keeps callback arity, reference-cell, and opened-path rules out of adapter orchestration.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters`.
//!
//! Key details:
//! - Callback source shapes include the receiver and are independent of user declarations.
//! - A by-value opened-path argument observes null while a by-reference declaration receives the cell.

use crate::codegen::{DataSection, abi};
use crate::codegen_support::emit::Emitter;
use crate::ir::Module;
use crate::types::{FunctionSig, PhpType};

use super::{adapter_slot_offset, coercion, throwable};

/// Returns the runtime callback argument types, including the receiver.
pub(super) fn wrapper_runtime_arg_types(slot: usize, class_name: &str) -> Vec<PhpType> {
    let receiver = PhpType::Object(class_name.to_string());
    let visible = match slot {
        0 => vec![PhpType::Str, PhpType::Str, PhpType::Int, PhpType::Int],
        1 | 4 | 5 | 7 | 8 | 20 | 21 | 22 => Vec::new(),
        2 | 10 | 11 | 12 => vec![PhpType::Int],
        3 => vec![PhpType::Str],
        6 => vec![PhpType::Int, PhpType::Int],
        9 => vec![PhpType::Str, PhpType::Int],
        13 => vec![PhpType::Int, PhpType::Int, PhpType::Int],
        14 => vec![PhpType::Str, PhpType::Int, PhpType::Mixed],
        15 | 17 | 18 => vec![PhpType::Str, PhpType::Int, PhpType::Int],
        19 => vec![PhpType::Str, PhpType::Int],
        16 => vec![PhpType::Str, PhpType::Str],
        _ => unreachable!("unknown user-wrapper vtable slot"),
    };
    let mut args = Vec::with_capacity(visible.len() + 1);
    args.push(receiver);
    args.extend(visible);
    args
}

/// Returns the compiled method's ABI types, including receiver and by-reference cells.
pub(super) fn wrapper_method_arg_types(
    signature: &FunctionSig,
    impl_class: &str,
) -> Vec<PhpType> {
    let mut args = Vec::with_capacity(signature.params.len() + 1);
    args.push(PhpType::Object(impl_class.to_string()));
    args.extend(
        signature
            .params
            .iter()
            .enumerate()
            .map(|(index, (_, php_type))| {
                if signature.ref_params.get(index).copied().unwrap_or(false) {
                    PhpType::Int
                } else {
                    php_type.codegen_repr()
                }
            }),
    );
    args
}

/// Returns the semantic PHP type for one adapter argument, including its receiver.
pub(super) fn wrapper_semantic_arg_type<'a>(
    signature: &'a FunctionSig,
    actual_ty: &'a PhpType,
    index: usize,
) -> &'a PhpType {
    if index == 0 {
        return actual_ty;
    }
    &signature.params[index - 1].1
}

/// Returns whether one adapter parameter is passed through a PHP reference cell.
pub(super) fn wrapper_arg_is_by_ref(signature: &FunctionSig, index: usize) -> bool {
    index > 0
        && signature
            .ref_params
            .get(index - 1)
            .copied()
            .unwrap_or(false)
}

/// Returns whether one compiled adapter argument is the generated variadic array slot.
pub(super) fn wrapper_arg_is_variadic(signature: &FunctionSig, index: usize) -> bool {
    index > 0
        && signature.variadic.as_ref().is_some_and(|variadic| {
            signature
                .params
                .get(index - 1)
                .is_some_and(|(name, _)| name == variadic)
        })
}

/// Returns the number of fixed visible parameters before the variadic array slot.
pub(super) fn wrapper_regular_param_count(signature: &FunctionSig) -> usize {
    signature
        .params
        .len()
        .saturating_sub(usize::from(signature.variadic.is_some()))
}

/// Returns the PHP value type supplied by the fixed runtime callback contract.
pub(super) fn wrapper_source_type(
    slot: usize,
    index: usize,
    by_ref: bool,
    incoming_types: &[PhpType],
) -> Option<PhpType> {
    if slot == 0 && index == 4 && !by_ref {
        return Some(PhpType::Void);
    }
    incoming_types.get(index).cloned()
}

/// Returns whether the runtime supplied an actual PHP reference cell for this callback argument.
pub(super) fn wrapper_source_is_reference_cell(
    slot: usize,
    index: usize,
    by_ref: bool,
) -> bool {
    by_ref && slot == 0 && index == 4
}

/// Loads one fixed runtime callback value, materializing by-value opened-path null.
pub(super) fn load_wrapper_source(
    emitter: &mut Emitter,
    index: usize,
    source_ty: &PhpType,
) {
    if *source_ty == PhpType::Void {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        return;
    }
    abi::emit_load(emitter, source_ty, adapter_slot_offset(index));
}

/// Validates callback arity and all declared fixed parameters before allocating conversion owners.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_wrapper_arg_preflights(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    adapter: &str,
    impl_class: &str,
    method_name: &str,
    slot: usize,
    signature: &FunctionSig,
    incoming_types: &[PhpType],
) {
    emit_wrapper_arity_preflight(
        emitter,
        data,
        adapter,
        impl_class,
        method_name,
        signature,
        incoming_types.len().saturating_sub(1),
    );
    let fixed_limit = wrapper_regular_param_count(signature) + 1;
    for index in 1..incoming_types.len().min(fixed_limit) {
        let source_ty = wrapper_source_type(
            slot,
            index,
            wrapper_arg_is_by_ref(signature, index),
            incoming_types,
        )
        .expect("runtime wrapper preflight source exists");
        let source_is_ref_cell = wrapper_source_is_reference_cell(
            slot,
            index,
            wrapper_arg_is_by_ref(signature, index),
        );
        let (parameter_name, target_ty) = &signature.params[index - 1];
        let type_expr = signature
            .param_type_exprs
            .get(index - 1)
            .and_then(Option::as_ref);
        let declared = signature
            .declared_params
            .get(index - 1)
            .copied()
            .unwrap_or(false);
        if source_is_ref_cell {
            if declared && !type_expr.is_some_and(wrapper_type_expr_accepts_null) {
                coercion::emit_wrapper_arg_preflight(
                    module,
                    emitter,
                    data,
                    &format!("{adapter}_arg_{index}"),
                    impl_class,
                    method_name,
                    index,
                    Some(parameter_name),
                    &PhpType::Void,
                    target_ty,
                    type_expr,
                    declared,
                    false,
                );
            }
            continue;
        }
        load_wrapper_source(emitter, index, &source_ty);
        coercion::emit_wrapper_arg_preflight(
            module,
            emitter,
            data,
            &format!("{adapter}_arg_{index}"),
            impl_class,
            method_name,
            index,
            Some(parameter_name),
            &source_ty,
            target_ty,
            type_expr,
            declared,
            false,
        );
    }
}

/// Returns whether a declared callback parameter type accepts the opened-path cell's initial null.
fn wrapper_type_expr_accepts_null(type_expr: &crate::parser::ast::TypeExpr) -> bool {
    use crate::parser::ast::TypeExpr;

    match type_expr {
        TypeExpr::Nullable(_) | TypeExpr::Void => true,
        TypeExpr::Named(name) => name.as_str().eq_ignore_ascii_case("mixed"),
        TypeExpr::Union(members) => members.iter().any(wrapper_type_expr_accepts_null),
        _ => false,
    }
}

/// Throws PHP's exact catchable ArgumentCountError before any adapter owner is allocated.
#[allow(clippy::too_many_arguments)]
fn emit_wrapper_arity_preflight(
    emitter: &mut Emitter,
    data: &mut DataSection,
    adapter: &str,
    impl_class: &str,
    method_name: &str,
    signature: &FunctionSig,
    passed_count: usize,
) {
    let regular_count = wrapper_regular_param_count(signature);
    let required_count = (0..regular_count)
        .rev()
        .find(|index| {
            signature
                .defaults
                .get(*index)
                .and_then(Option::as_ref)
                .is_none()
        })
        .map_or(0, |index| index + 1);
    if passed_count >= required_count {
        return;
    }
    let message = format!(
        "Too few arguments to function {}::{}(), {} passed and exactly {} expected",
        impl_class.trim_start_matches('\\'),
        method_name,
        passed_count,
        required_count
    );
    throwable::emit_static_throwable(
        emitter,
        data,
        &format!("{adapter}_argument_count"),
        "ArgumentCountError",
        "_spl_argument_count_error_class_id",
        &message,
    );
}
