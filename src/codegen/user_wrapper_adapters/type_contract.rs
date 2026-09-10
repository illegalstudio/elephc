//! Purpose:
//! Enforces declared union, nullable, intersection, and dynamically boxed callback types.
//! Selects PHP weak-scalar union conversions without weakening exact container/object matches.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::coercion`.
//!
//! Key details:
//! - Dynamic `Mixed` values are validated by their normalized runtime tag, not their checker type.
//! - Composite parameters receive fresh Mixed boxes so coercion ownership is deterministic.

mod numeric;
mod semantics;

use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_box_runtime_payload_as_mixed,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::ir::Module;
use crate::parser::ast::TypeExpr;
use crate::types::PhpType;

use super::{adapter_slot_offset, coercion};
use semantics::{
    ScalarAtom, classify_named_target, is_builtin_named_type, php_type_for_runtime_tag,
    scalar_fallback, type_expr_accepts_static_exact, type_expr_accepts_tag_without_value,
    type_expr_has_atom, type_expr_has_value_checked_candidate, type_expr_is_mixed,
};

/// One compile-time conversion selected for a concrete source and composite declaration.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum StaticConversion {
    Preserve,
    Convert(PhpType),
    NumericString {
        allow_int: bool,
        allow_float: bool,
        fallback_bool: bool,
    },
}

/// Returns which dynamic scalar kinds may prefer a lossy weak conversion to `int`.
pub(super) fn weak_int_deprecation_sources(type_expr: &TypeExpr) -> (bool, bool) {
    let accepts_int = type_expr_has_atom(type_expr, ScalarAtom::Int);
    if !accepts_int {
        return (false, false);
    }
    let accepts_float = type_expr_has_atom(type_expr, ScalarAtom::Float);
    let accepts_string = type_expr_has_atom(type_expr, ScalarAtom::String);
    (
        !accepts_float,
        !accepts_float && !accepts_string,
    )
}

/// Returns whether a declared type uses the boxed ABI while retaining narrower PHP semantics.
pub(super) fn is_composite(type_expr: Option<&TypeExpr>, target_ty: &PhpType) -> bool {
    target_ty.codegen_repr() == PhpType::Mixed
        && type_expr.is_some_and(|type_expr| {
            matches!(
                type_expr,
                TypeExpr::Union(_) | TypeExpr::Nullable(_) | TypeExpr::Intersection(_)
            )
        })
}

/// Resolves one non-composite declared callback type to its runtime storage representation.
pub(super) fn simple_declared_storage_type(type_expr: &TypeExpr) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::False => PhpType::False,
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Iterable => PhpType::Iterable,
        TypeExpr::Array(inner) => {
            PhpType::Array(Box::new(simple_declared_storage_type(inner)))
        }
        TypeExpr::Ptr(name) => {
            PhpType::Pointer(name.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(simple_declared_storage_type(inner)))
        }
        TypeExpr::Named(name) => {
            let raw = name.as_str();
            match raw.trim_start_matches('\\').to_ascii_lowercase().as_str() {
                "array" => PhpType::Array(Box::new(PhpType::Mixed)),
                "callable" | "closure" => PhpType::Callable,
                "mixed" => PhpType::Mixed,
                "object" => PhpType::Object(String::new()),
                "string" => PhpType::Str,
                "null" | "void" => PhpType::Void,
                _ => PhpType::Object(raw.to_string()),
            }
        }
        TypeExpr::Nullable(_) | TypeExpr::Union(_) | TypeExpr::Intersection(_) => PhpType::Mixed,
    }
}

/// Selects PHP's exact-first weak conversion for one statically known composite argument.
pub(super) fn select_static_conversion(
    module: &Module,
    source_ty: &PhpType,
    type_expr: &TypeExpr,
) -> Option<StaticConversion> {
    if type_expr_accepts_static_exact(module, type_expr, source_ty) {
        return Some(StaticConversion::Preserve);
    }
    match source_ty.codegen_repr() {
        PhpType::Int => scalar_fallback(type_expr, 0).map(StaticConversion::Convert),
        PhpType::Float => scalar_fallback(type_expr, 2).map(StaticConversion::Convert),
        PhpType::Str => {
            let allow_int = type_expr_has_atom(type_expr, ScalarAtom::Int);
            let allow_float = type_expr_has_atom(type_expr, ScalarAtom::Float);
            if allow_int || allow_float {
                Some(StaticConversion::NumericString {
                    allow_int,
                    allow_float,
                    fallback_bool: type_expr_has_atom(type_expr, ScalarAtom::Bool),
                })
            } else {
                scalar_fallback(type_expr, 1).map(StaticConversion::Convert)
            }
        }
        PhpType::Bool | PhpType::False => {
            scalar_fallback(type_expr, 3).map(StaticConversion::Convert)
        }
        _ => None,
    }
}

/// Emits runtime validation for a declared callback parameter whose source is boxed Mixed.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_dynamic_preflight(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut crate::codegen::data_section::DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    parameter_index: usize,
    parameter_name: Option<&str>,
    target_ty: &PhpType,
    type_expr: &TypeExpr,
) {
    if type_expr_is_mixed(type_expr) {
        return;
    }
    let done = format!("{label_prefix}_dynamic_type_ok");
    let unknown = format!("{label_prefix}_dynamic_type_unknown");
    let case_labels = (0..=10)
        .map(|tag| format!("{label_prefix}_dynamic_type_tag_{tag}"))
        .collect::<Vec<_>>();
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for (tag, label) in case_labels.iter().enumerate() {
        emit_branch_if_tag_equal(emitter, tag as i64, label);
    }
    abi::emit_jump(emitter, &unknown);

    for (tag, label) in case_labels.iter().enumerate() {
        emitter.label(label);
        emit_dynamic_tag_preflight(
            module,
            emitter,
            data,
            label_prefix,
            class_name,
            method_name,
            parameter_index,
            parameter_name,
            target_ty,
            type_expr,
            tag as u8,
            &done,
        );
    }

    emitter.label(&unknown);
    coercion::emit_wrapper_parameter_type_error(
        emitter,
        data,
        &unknown,
        class_name,
        method_name,
        parameter_index,
        parameter_name,
        target_ty,
        Some(type_expr),
        &PhpType::Mixed,
    );
    emitter.label(&done);
}

/// Converts one validated boxed Mixed source to a declared non-composite ABI value.
pub(super) fn emit_dynamic_non_composite_conversion(
    emitter: &mut Emitter,
    target_ty: &PhpType,
) {
    match target_ty.codegen_repr() {
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_)
        | PhpType::Iterable
        | PhpType::Callable => {
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            move_unboxed_low_payload_to_result(emitter);
        }
        PhpType::Void => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
        _ => {}
    }
}

/// Converts one validated boxed Mixed source to a fresh composite-parameter Mixed owner.
pub(super) fn emit_dynamic_composite_conversion(
    module: &Module,
    emitter: &mut Emitter,
    label_prefix: &str,
    source_offset: usize,
    type_expr: &TypeExpr,
) {
    let done = format!("{label_prefix}_dynamic_union_done");
    let clone = format!("{label_prefix}_dynamic_union_clone");
    let case_labels = (0..=10)
        .map(|tag| format!("{label_prefix}_dynamic_union_tag_{tag}"))
        .collect::<Vec<_>>();
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for (tag, label) in case_labels.iter().enumerate() {
        emit_branch_if_tag_equal(emitter, tag as i64, label);
    }
    abi::emit_jump(emitter, &clone);

    for (tag, label) in case_labels.iter().enumerate() {
        emitter.label(label);
        if type_expr_accepts_tag_without_value(type_expr, tag as u8) {
            abi::emit_jump(emitter, &clone);
            continue;
        }
        if type_expr_has_value_checked_candidate(type_expr, tag as u8) {
            let checked = format!("{label}_checked");
            let fallback = format!("{label}_fallback");
            emit_value_checked_acceptance(
                module,
                emitter,
                source_offset,
                type_expr,
                tag as u8,
                &checked,
                &fallback,
            );
            emitter.label(&checked);
            abi::emit_jump(emitter, &clone);
            emitter.label(&fallback);
            emit_dynamic_scalar_conversion(
                emitter,
                label_prefix,
                type_expr,
                tag as u8,
                &clone,
                &done,
            );
            continue;
        }
        emit_dynamic_scalar_conversion(
            emitter,
            label_prefix,
            type_expr,
            tag as u8,
            &clone,
            &done,
        );
    }

    emitter.label(&clone);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    emit_box_unboxed_payload(emitter);
    abi::emit_jump(emitter, &done);

    emitter.label(&done);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Emits one concrete-source composite conversion and leaves a fresh Mixed owner.
pub(super) fn emit_static_composite_conversion(
    emitter: &mut Emitter,
    label_prefix: &str,
    source_ty: &PhpType,
    conversion: &StaticConversion,
) {
    match conversion {
        StaticConversion::Preserve => emit_box_current_value_as_mixed(emitter, source_ty),
        StaticConversion::Convert(target_ty) => {
            coercion::emit_scalar_conversion(emitter, label_prefix, source_ty, target_ty);
            emit_box_converted_scalar(emitter, target_ty);
        }
        StaticConversion::NumericString {
            allow_int,
            allow_float,
            fallback_bool,
        } => numeric::emit_string_numeric_union_conversion(
            emitter,
            label_prefix,
            None,
            *allow_int,
            *allow_float,
            *fallback_bool,
        ),
    }
}

/// Emits the per-tag validation path and branches to the shared success label when accepted.
#[allow(clippy::too_many_arguments)]
fn emit_dynamic_tag_preflight(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut crate::codegen::data_section::DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    parameter_index: usize,
    parameter_name: Option<&str>,
    target_ty: &PhpType,
    type_expr: &TypeExpr,
    tag: u8,
    done: &str,
) {
    if type_expr_accepts_tag_without_value(type_expr, tag) {
        abi::emit_jump(emitter, done);
        return;
    }
    let fallback = format!("{label_prefix}_dynamic_type_tag_{tag}_fallback");
    if type_expr_has_value_checked_candidate(type_expr, tag) {
        emit_value_checked_acceptance(
            module,
            emitter,
            adapter_slot_offset(parameter_index),
            type_expr,
            tag,
            done,
            &fallback,
        );
        emitter.label(&fallback);
    }
    if tag == 1 {
        let allow_int = type_expr_has_atom(type_expr, ScalarAtom::Int);
        let allow_float = type_expr_has_atom(type_expr, ScalarAtom::Float);
        if allow_int || allow_float {
            if type_expr_has_atom(type_expr, ScalarAtom::Bool) {
                abi::emit_jump(emitter, done);
                return;
            }
            numeric::emit_string_numeric_preflight(emitter, allow_float, done);
            emit_dynamic_type_error(
                emitter,
                data,
                label_prefix,
                class_name,
                method_name,
                parameter_index,
                parameter_name,
                target_ty,
                type_expr,
                tag,
            );
            return;
        }
    }
    if tag == 3 && type_expr_has_atom(type_expr, ScalarAtom::False) {
        let false_value = format!("{label_prefix}_dynamic_type_false_value");
        emit_branch_if_low_payload_zero(emitter, &false_value);
        if scalar_fallback(type_expr, tag).is_some() {
            abi::emit_jump(emitter, done);
        } else {
            emit_dynamic_type_error(
                emitter,
                data,
                label_prefix,
                class_name,
                method_name,
                parameter_index,
                parameter_name,
                target_ty,
                type_expr,
                tag,
            );
        }
        emitter.label(&false_value);
        abi::emit_jump(emitter, done);
        return;
    }
    if scalar_fallback(type_expr, tag).is_some() {
        abi::emit_jump(emitter, done);
        return;
    }
    emit_dynamic_type_error(
        emitter,
        data,
        label_prefix,
        class_name,
        method_name,
        parameter_index,
        parameter_name,
        target_ty,
        type_expr,
        tag,
    );
}

/// Emits a dynamic TypeError block with the concrete runtime tag's PHP spelling.
#[allow(clippy::too_many_arguments)]
fn emit_dynamic_type_error(
    emitter: &mut Emitter,
    data: &mut crate::codegen::data_section::DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    parameter_index: usize,
    parameter_name: Option<&str>,
    target_ty: &PhpType,
    type_expr: &TypeExpr,
    tag: u8,
) {
    coercion::emit_wrapper_parameter_type_error(
        emitter,
        data,
        &format!("{label_prefix}_dynamic_type_error_{tag}"),
        class_name,
        method_name,
        parameter_index,
        parameter_name,
        target_ty,
        Some(type_expr),
        &php_type_for_runtime_tag(tag),
    );
}

/// Emits value-sensitive acceptance for class/interface, callable, iterable, and false atoms.
#[allow(clippy::too_many_arguments)]
fn emit_value_checked_acceptance(
    module: &Module,
    emitter: &mut Emitter,
    source_offset: usize,
    type_expr: &TypeExpr,
    tag: u8,
    success: &str,
    failure: &str,
) {
    match type_expr {
        TypeExpr::Nullable(inner) => emit_value_checked_acceptance(
            module,
            emitter,
            source_offset,
            inner,
            tag,
            success,
            failure,
        ),
        TypeExpr::Union(members) => {
            for (index, member) in members.iter().enumerate() {
                let next = if index + 1 == members.len() {
                    failure.to_string()
                } else {
                    format!("{success}_member_{index}_next")
                };
                emit_value_checked_acceptance(
                    module,
                    emitter,
                    source_offset,
                    member,
                    tag,
                    success,
                    &next,
                );
                if index + 1 != members.len() {
                    emitter.label(&next);
                }
            }
        }
        TypeExpr::Intersection(members) if tag == 6 => {
            for (index, member) in members.iter().enumerate() {
                let next = if index + 1 == members.len() {
                    success.to_string()
                } else {
                    format!("{success}_intersection_{index}_next")
                };
                emit_value_checked_acceptance(
                    module,
                    emitter,
                    source_offset,
                    member,
                    tag,
                    &next,
                    failure,
                );
                if index + 1 != members.len() {
                    emitter.label(&next);
                }
            }
        }
        TypeExpr::False if tag == 3 => {
            emit_branch_if_low_payload_zero(emitter, success);
            abi::emit_jump(emitter, failure);
        }
        TypeExpr::Iterable if tag == 6 => {
            emit_named_mixed_match(
                module,
                emitter,
                source_offset,
                "Traversable",
                success,
                failure,
            );
        }
        TypeExpr::Named(name)
            if name.as_str().eq_ignore_ascii_case("callable")
                && matches!(tag, 1 | 4 | 5 | 6) =>
        {
            abi::emit_load(emitter, &PhpType::Mixed, source_offset);
            abi::emit_call_label(emitter, "__rt_is_callable_mixed");
            emit_branch_if_int_nonzero(emitter, success);
            abi::emit_jump(emitter, failure);
        }
        TypeExpr::Named(name)
            if tag == 6 && !is_builtin_named_type(name.as_str()) =>
        {
            emit_named_mixed_match(
                module,
                emitter,
                source_offset,
                name.as_str(),
                success,
                failure,
            );
        }
        _ => abi::emit_jump(emitter, failure),
    }
}

/// Emits one named class/interface match against the boxed source slot.
fn emit_named_mixed_match(
    module: &Module,
    emitter: &mut Emitter,
    source_offset: usize,
    name: &str,
    success: &str,
    failure: &str,
) {
    let Some((target_id, target_kind)) = classify_named_target(module, name) else {
        abi::emit_jump(emitter, failure);
        return;
    };
    abi::emit_load(
        emitter,
        &PhpType::Mixed,
        source_offset,
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x1", target_id as i64);
            abi::emit_load_int_immediate(emitter, "x2", target_kind);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the boxed callback value to mixed instanceof
            abi::emit_load_int_immediate(emitter, "rsi", target_id as i64);
            abi::emit_load_int_immediate(emitter, "rdx", target_kind);
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_instanceof");
    emit_branch_if_int_nonzero(emitter, success);
    abi::emit_jump(emitter, failure);
}

/// Emits the scalar fallback conversion for one dynamic union tag.
fn emit_dynamic_scalar_conversion(
    emitter: &mut Emitter,
    label_prefix: &str,
    type_expr: &TypeExpr,
    tag: u8,
    clone: &str,
    done: &str,
) {
    if tag == 3 && type_expr_has_atom(type_expr, ScalarAtom::False) {
        let true_fallback = format!("{label_prefix}_dynamic_union_true_fallback");
        emit_branch_if_low_payload_zero(emitter, clone);
        emitter.label(&true_fallback);
    }
    if tag == 1 {
        let allow_int = type_expr_has_atom(type_expr, ScalarAtom::Int);
        let allow_float = type_expr_has_atom(type_expr, ScalarAtom::Float);
        if allow_int || allow_float {
            numeric::emit_string_numeric_union_conversion(
                emitter,
                label_prefix,
                Some(0),
                allow_int,
                allow_float,
                type_expr_has_atom(type_expr, ScalarAtom::Bool),
            );
            abi::emit_jump(emitter, done);
            return;
        }
    }
    let Some(target_ty) = scalar_fallback(type_expr, tag) else {
        abi::emit_jump(emitter, clone);
        return;
    };
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    coercion::emit_scalar_conversion(emitter, label_prefix, &PhpType::Mixed, &target_ty);
    emit_box_converted_scalar(emitter, &target_ty);
    abi::emit_jump(emitter, done);
}

/// Boxes the tag and payload returned by mixed-unbox into a fresh owner.
fn emit_box_unboxed_payload(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_box_runtime_payload_as_mixed(emitter, "x0", "x1", "x2"),
        Arch::X86_64 => emit_box_runtime_payload_as_mixed(emitter, "rax", "rdi", "rdx"),
    }
}

/// Boxes a converted scalar while transferring an owned persisted string when necessary.
fn emit_box_converted_scalar(emitter: &mut Emitter, target_ty: &PhpType) {
    if target_ty.codegen_repr() == PhpType::Str {
        emit_box_current_owned_value_as_mixed(emitter, target_ty);
    } else {
        emit_box_current_value_as_mixed(emitter, target_ty);
    }
}

/// Moves mixed-unbox's low payload word into the canonical integer result register.
fn move_unboxed_low_payload_to_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, x1"),                     // return the unboxed callback payload pointer/value
        Arch::X86_64 => emitter.instruction("mov rax, rdi"),                    // return the unboxed callback payload pointer/value
    }
}

/// Branches when mixed-unbox's tag result equals one literal tag.
fn emit_branch_if_tag_equal(emitter: &mut Emitter, tag: i64, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{}", tag));                  // compare the dynamic callback value with one Mixed tag
            emitter.instruction(&format!("b.eq {}", label));                    // dispatch the matching dynamic callback tag
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {}", tag));                  // compare the dynamic callback value with one Mixed tag
            emitter.instruction(&format!("je {}", label));                      // dispatch the matching dynamic callback tag
        }
    }
}

/// Branches when the current integer result is nonzero.
fn emit_branch_if_int_nonzero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz x0, {}", label));                // accept the dynamic callback value after a runtime predicate
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect the dynamic callback runtime predicate
            emitter.instruction(&format!("jnz {}", label));                     // accept the value when the predicate succeeded
        }
    }
}

/// Branches when mixed-unbox's low payload word is zero.
fn emit_branch_if_low_payload_zero(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", label));                 // literal false has a zero boolean payload
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // inspect the unboxed boolean payload
            emitter.instruction(&format!("jz {}", label));                      // literal false has a zero boolean payload
        }
    }
}
