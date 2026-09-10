//! Purpose:
//! Validates and materializes PHP callback arguments for userspace stream-wrapper adapters.
//! Applies weak scalar parameter coercions before entering compiled user methods.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters`.
//!
//! Key details:
//! - Validation runs before owner-producing conversions so Throwable escape cannot leak temps.
//! - Converted strings are persisted because scalar formatters use transient runtime scratch.

use crate::codegen::{abi, data_section::DataSection, emit_box_current_value_as_mixed};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::ir::Module;
use crate::parser::ast::TypeExpr;
use crate::types::PhpType;

use super::{
    deprecations, throwable::emit_static_throwable, type_contract,
};

/// Validates one fixed runtime argument against a declared PHP callback parameter.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_wrapper_arg_preflight(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    parameter_index: usize,
    parameter_name: Option<&str>,
    source_ty: &PhpType,
    target_ty: &PhpType,
    type_expr: Option<&TypeExpr>,
    declared: bool,
    by_ref: bool,
) {
    if !declared || by_ref {
        return;
    }
    if *source_ty == PhpType::Mixed {
        if let Some(type_expr) = type_expr {
            let result = abi::int_result_reg(emitter);
            abi::emit_push_reg(emitter, result);
            type_contract::emit_dynamic_preflight(
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
            );
            abi::emit_pop_reg(emitter, result);
            let (float_can_select_int, string_can_select_int) =
                type_contract::weak_int_deprecation_sources(type_expr);
            deprecations::emit_dynamic_to_int_if_lossy(
                emitter,
                data,
                label_prefix,
                float_can_select_int,
                string_can_select_int,
            );
            return;
        }
    }
    if let Some(type_expr) = type_expr.filter(|type_expr| {
        matches!(
            type_expr,
            TypeExpr::Union(_) | TypeExpr::Nullable(_) | TypeExpr::Intersection(_)
        )
    }) {
        match type_contract::select_static_conversion(module, source_ty, type_expr) {
            Some(type_contract::StaticConversion::NumericString {
                allow_int,
                allow_float,
                fallback_bool: false,
            }) => {
                if allow_int && !allow_float {
                    deprecations::emit_string_to_int_if_lossy(
                        emitter,
                        data,
                        label_prefix,
                    );
                }
                emit_numeric_string_preflight(
                    emitter,
                    data,
                    label_prefix,
                    class_name,
                    method_name,
                    parameter_index,
                    parameter_name,
                    target_ty,
                    Some(type_expr),
                    allow_float,
                );
            }
            Some(type_contract::StaticConversion::NumericString {
                allow_int,
                allow_float,
                fallback_bool: true,
            }) => {
                if allow_int && !allow_float {
                    deprecations::emit_string_to_int_if_lossy(
                        emitter,
                        data,
                        label_prefix,
                    );
                }
            }
            Some(_) => {}
            None => emit_wrapper_parameter_type_error(
                emitter,
                data,
                label_prefix,
                class_name,
                method_name,
                parameter_index,
                parameter_name,
                target_ty,
                Some(type_expr),
                source_ty,
            ),
        }
        return;
    }
    if wrapper_types_are_directly_compatible(source_ty, target_ty) {
        return;
    }
    if *source_ty == PhpType::Str && matches!(target_ty.codegen_repr(), PhpType::Int | PhpType::Float)
    {
        if target_ty.codegen_repr() == PhpType::Int {
            deprecations::emit_string_to_int_if_lossy(
                emitter,
                data,
                label_prefix,
            );
        }
        emit_numeric_string_preflight(
            emitter,
            data,
            label_prefix,
            class_name,
            method_name,
            parameter_index,
            parameter_name,
            target_ty,
            type_expr,
            target_ty.codegen_repr() == PhpType::Float,
        );
        return;
    }
    if *source_ty == PhpType::Mixed && target_ty.codegen_repr() == PhpType::Int {
        deprecations::emit_dynamic_to_int_if_lossy(
            emitter,
            data,
            label_prefix,
            true,
            true,
        );
    }
    if wrapper_scalar_coercion_is_supported(source_ty, target_ty) {
        return;
    }
    emit_wrapper_parameter_type_error(
        emitter,
        data,
        label_prefix,
        class_name,
        method_name,
        parameter_index,
        parameter_name,
        target_ty,
        type_expr,
        source_ty,
    );
}

/// Converts the current canonical runtime value to the compiled callback parameter ABI.
pub(super) fn emit_wrapper_arg_conversion(
    module: &Module,
    emitter: &mut Emitter,
    label_prefix: &str,
    source_index: usize,
    source_ty: &PhpType,
    target_ty: &PhpType,
    type_expr: Option<&TypeExpr>,
    declared: bool,
    by_ref: bool,
) {
    if by_ref {
        return;
    }
    if declared && type_contract::is_composite(type_expr, target_ty) {
        let type_expr = type_expr.expect("declared composite callback type expression");
        if source_ty.codegen_repr() == PhpType::Mixed {
            type_contract::emit_dynamic_composite_conversion(
                module,
                emitter,
                label_prefix,
                super::adapter_slot_offset(source_index),
                type_expr,
            );
        } else {
            let conversion = type_contract::select_static_conversion(module, source_ty, type_expr)
                .expect("composite callback preflight accepted its concrete source");
            type_contract::emit_static_composite_conversion(
                emitter,
                label_prefix,
                source_ty,
                &conversion,
            );
        }
        return;
    }
    if wrapper_types_are_directly_compatible(source_ty, target_ty) {
        return;
    }
    let target_repr = target_ty.codegen_repr();
    if matches!(target_repr, PhpType::Mixed) {
        emit_box_current_value_as_mixed(emitter, source_ty);
        return;
    }
    emit_scalar_conversion(emitter, label_prefix, source_ty, &target_repr);
    if source_ty.codegen_repr() == PhpType::Mixed {
        type_contract::emit_dynamic_non_composite_conversion(emitter, target_ty);
    }
}

/// Applies one already-validated weak scalar conversion to the current callback value.
pub(super) fn emit_scalar_conversion(
    emitter: &mut Emitter,
    label_prefix: &str,
    source_ty: &PhpType,
    target_ty: &PhpType,
) {
    match (source_ty.codegen_repr(), target_ty.codegen_repr()) {
        (PhpType::Int, PhpType::Bool) => emit_int_to_bool(emitter),
        (PhpType::Str, PhpType::Bool) => emit_string_to_bool(emitter, label_prefix),
        (PhpType::Int, PhpType::Float) => abi::emit_int_result_to_float_result(emitter),
        (PhpType::Str, PhpType::Int) => {
            abi::emit_call_label(emitter, "__rt_str_to_int");
        }
        (PhpType::Str, PhpType::Float) => {
            abi::emit_call_label(emitter, "__rt_str_to_number");
        }
        (PhpType::Int, PhpType::Str) => {
            abi::emit_call_label(emitter, "__rt_itoa");
            abi::emit_call_label(emitter, "__rt_str_persist");
        }
        (PhpType::Mixed, PhpType::Bool) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
        }
        (PhpType::Mixed, PhpType::Int) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_int");
        }
        (PhpType::Mixed, PhpType::Float) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_float");
        }
        (PhpType::Mixed, PhpType::Str) => {
            abi::emit_call_label(emitter, "__rt_mixed_cast_string");
            abi::emit_call_label(emitter, "__rt_str_persist");
        }
        _ => {}
    }
}

/// Returns the owned temporary type created while adapting one runtime argument.
pub(super) fn wrapper_arg_temp_type(
    source_ty: Option<&PhpType>,
    target_ty: &PhpType,
    type_expr: Option<&TypeExpr>,
    declared: bool,
    by_ref: bool,
) -> Option<PhpType> {
    if by_ref {
        return None;
    }
    let target_repr = target_ty.codegen_repr();
    match source_ty {
        Some(source_ty)
            if declared
                && type_contract::is_composite(type_expr, target_ty)
                && source_ty.codegen_repr() == PhpType::Mixed =>
        {
            Some(PhpType::Mixed)
        }
        Some(source_ty)
            if target_repr == PhpType::Mixed
                && !matches!(source_ty.codegen_repr(), PhpType::Mixed) =>
        {
            Some(PhpType::Mixed)
        }
        Some(source_ty)
            if target_repr == PhpType::Str
                && matches!(source_ty.codegen_repr(), PhpType::Int | PhpType::Mixed) =>
        {
            Some(PhpType::Str)
        }
        None if matches!(
            target_repr,
            PhpType::Str
                | PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Iterable
                | PhpType::Callable
        ) =>
        {
            Some(target_repr)
        }
        _ => None,
    }
}

/// Calls the synthetic EIR thunk for one callback parameter missing from the runtime contract.
pub(super) fn emit_wrapper_default(
    emitter: &mut Emitter,
    class_id: u64,
    method_name: &str,
    parameter_index: usize,
) {
    let thunk_name =
        crate::codegen_support::runtime::user_wrapper_default_thunk_name(
            class_id,
            method_name,
            parameter_index,
        );
    abi::emit_call_label(
        emitter,
        &crate::names::function_symbol(&thunk_name),
    );
}

/// Returns true when no ABI conversion or runtime validation is required.
fn wrapper_types_are_directly_compatible(source_ty: &PhpType, target_ty: &PhpType) -> bool {
    source_ty.codegen_repr() == target_ty.codegen_repr()
}

/// Returns true for scalar weak-coercion pairs supported by PHP callback invocation.
fn wrapper_scalar_coercion_is_supported(source_ty: &PhpType, target_ty: &PhpType) -> bool {
    matches!(
        (source_ty.codegen_repr(), target_ty.codegen_repr()),
        (PhpType::Int, PhpType::Bool | PhpType::Float | PhpType::Str)
            | (PhpType::Str, PhpType::Bool | PhpType::Int | PhpType::Float)
            | (
                PhpType::Mixed,
                PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str
            )
            | (_, PhpType::Mixed)
    )
}

/// Probes PHP numeric-string validity before a later int or float conversion.
#[allow(clippy::too_many_arguments)]
fn emit_numeric_string_preflight(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    parameter_index: usize,
    parameter_name: Option<&str>,
    target_ty: &PhpType,
    type_expr: Option<&TypeExpr>,
    allow_float: bool,
) {
    let invalid_label = format!("{label_prefix}_arg_{parameter_index}_invalid_numeric");
    let valid_label = format!("{label_prefix}_arg_{parameter_index}_valid_numeric");
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
    abi::emit_call_label(emitter, "__rt_str_numeric_union_kind");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {}", invalid_label));         // a non-numeric string cannot satisfy an int or float parameter
            if !allow_float {
                emitter.instruction("cmp x0, #2");                              // out-of-range float forms cannot satisfy an int-only parameter
                emitter.instruction(&format!("b.eq {}", invalid_label));        // reject numeric values outside PHP integer range
            }
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect the numeric-string coercion probe result
            emitter.instruction(&format!("jz {}", invalid_label));              // reject strings PHP cannot coerce to the declared numeric type
            if !allow_float {
                emitter.instruction("cmp rax, 2");                              // out-of-range float forms cannot satisfy an int-only parameter
                emitter.instruction(&format!("je {}", invalid_label));          // reject numeric values outside PHP integer range
            }
        }
    }
    abi::emit_release_temporary_stack(emitter, 16);
    abi::emit_jump(emitter, &valid_label);
    emitter.label(&invalid_label);
    abi::emit_release_temporary_stack(emitter, 16);
    emit_wrapper_parameter_type_error(
        emitter,
        data,
        &invalid_label,
        class_name,
        method_name,
        parameter_index,
        parameter_name,
        target_ty,
        type_expr,
        &PhpType::Str,
    );
    emitter.label(&valid_label);
}

/// Emits an exact static TypeError for one incompatible wrapper callback argument.
#[allow(clippy::too_many_arguments)]
pub(in crate::codegen) fn emit_wrapper_parameter_type_error(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label_prefix: &str,
    class_name: &str,
    method_name: &str,
    parameter_index: usize,
    parameter_name: Option<&str>,
    target_ty: &PhpType,
    type_expr: Option<&TypeExpr>,
    source_ty: &PhpType,
) {
    let declared_type = type_expr
        .map(format_type_expr)
        .unwrap_or_else(|| format_php_type(target_ty));
    let parameter = parameter_name
        .map(|name| format!(" (${})", name))
        .unwrap_or_default();
    let message = format!(
        "{}::{}(): Argument #{}{} must be of type {}, {} given",
        class_name.trim_start_matches('\\'),
        method_name,
        parameter_index,
        parameter,
        declared_type,
        format_php_type(source_ty)
    );
    emit_static_throwable(
        emitter,
        data,
        label_prefix,
        "TypeError",
        "_spl_type_error_class_id",
        &message,
    );
}

/// Converts the current integer result to a canonical PHP bool payload.
fn emit_int_to_bool(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // compare the integer callback value with PHP false
            emitter.instruction("cset x0, ne");                                 // normalize every non-zero integer to bool true
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // compare the integer callback value with PHP false
            emitter.instruction("setne al");                                    // materialize bool true for every non-zero integer
            emitter.instruction("movzx rax, al");                               // clear the unused upper bool payload bits
        }
    }
}

/// Converts the current string result to PHP bool truthiness.
fn emit_string_to_bool(emitter: &mut Emitter, label_prefix: &str) {
    let false_label = format!("{label_prefix}_string_bool_false");
    let true_label = format!("{label_prefix}_string_bool_true");
    let done_label = format!("{label_prefix}_string_bool_done");
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", len_reg, false_label));  // an empty PHP string is false
            emitter.instruction(&format!("cmp {}, #1", len_reg));               // only the one-byte string "0" has another false case
            emitter.instruction(&format!("b.ne {}", true_label));               // every longer non-empty string is true
            emitter.instruction(&format!("ldrb w9, [{}]", ptr_reg));            // load the sole byte for the PHP "0" exception
            emitter.instruction("cmp w9, #48");                                 // compare the byte with ASCII zero
            emitter.instruction(&format!("b.ne {}", true_label));               // any other one-byte string is true
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", len_reg, len_reg));     // an empty PHP string is false
            emitter.instruction(&format!("jz {}", false_label));                // branch for the empty-string false case
            emitter.instruction(&format!("cmp {}, 1", len_reg));                // only the one-byte string "0" has another false case
            emitter.instruction(&format!("jne {}", true_label));                // every longer non-empty string is true
            emitter.instruction(&format!("cmp BYTE PTR [{}], 48", ptr_reg));    // compare the sole byte with ASCII zero
            emitter.instruction(&format!("jne {}", true_label));                // any other one-byte string is true
        }
    }
    emitter.label(&false_label);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&true_label);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    emitter.label(&done_label);
}

/// Formats one declared type expression using PHP error-message spelling.
fn format_type_expr(type_expr: &TypeExpr) -> String {
    match type_expr {
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::False => "false".to_string(),
        TypeExpr::Str => "string".to_string(),
        TypeExpr::Void => "null".to_string(),
        TypeExpr::Never => "never".to_string(),
        TypeExpr::Iterable => "iterable".to_string(),
        TypeExpr::Array(inner) => format!("array<{}>", format_type_expr(inner)),
        TypeExpr::Ptr(Some(name)) => format!("ptr<{}>", name.as_str()),
        TypeExpr::Ptr(None) => "ptr".to_string(),
        TypeExpr::Buffer(inner) => format!("buffer<{}>", format_type_expr(inner)),
        TypeExpr::Named(name) => name.as_str().trim_start_matches('\\').to_string(),
        TypeExpr::Nullable(inner) => format!("?{}", format_type_expr(inner)),
        TypeExpr::Union(members) => members
            .iter()
            .map(format_type_expr)
            .collect::<Vec<_>>()
            .join("|"),
        TypeExpr::Intersection(members) => members
            .iter()
            .map(format_type_expr)
            .collect::<Vec<_>>()
            .join("&"),
    }
}

/// Formats one semantic PHP type for a callback diagnostic.
fn format_php_type(php_type: &PhpType) -> String {
    match php_type {
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Void => "null".to_string(),
        PhpType::Object(name) if name == "object" => "object".to_string(),
        PhpType::Object(name) => name.trim_start_matches('\\').to_string(),
        PhpType::Resource(_) => "resource".to_string(),
        other => other.to_string(),
    }
}
