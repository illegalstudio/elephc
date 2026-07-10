//! Purpose:
//! Defines runtime callable dispatch metadata shared by indirect callback emitters.
//! Bridges AOT function signatures with runtime-selected callable values or names.
//!
//! Called from:
//! - `crate::codegen::lower_inst::callables` and EIR builtin callback lowerers.
//!
//! Key details:
//! - Cases carry the ABI entry label, optional PHP-visible name, signature metadata, and hidden captures.
//! - String-name dispatch compares against userland callable names before loading the matched descriptor.

use crate::codegen_support::abi;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::types::{callable_wrapper_sig, FunctionSig, PhpType};

#[derive(Clone)]
pub(crate) struct RuntimeCallableCase {
    pub(crate) label: String,
    pub(crate) descriptor_label: String,
    pub(crate) php_name: Option<String>,
}

pub(crate) enum RuntimeCallableSelector<'a> {
    StringNameStack {
        ptr_offset: usize,
        len_offset: usize,
        call_reg: &'a str,
    },
}

#[derive(Clone)]
pub(crate) struct RuntimeStaticMethodCallableCase {
    pub(crate) class_name: String,
    pub(crate) method_name: String,
    pub(crate) case: RuntimeCallableCase,
}

/// Returns true for builtins supported by generic runtime string-callable dispatch.
///
/// These names have stable fixed-arity EIR wrapper signatures and do not require literal,
/// by-reference, resource, hidden runtime-state, or callback-adapter semantics. Other builtins
/// remain available through direct calls or first-class callable lowering when that path supports
/// them, but runtime string dispatch must not pre-emit wrappers it cannot prove safe.
pub(crate) fn runtime_builtin_wrapper_supported(
    name: &str,
    source_arg_ty: Option<&PhpType>,
) -> bool {
    let name = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    if !runtime_builtin_name_supported(&name) {
        return false;
    }
    let source_arg_ty = source_arg_ty.map(PhpType::codegen_repr);
    match name.as_str() {
        "abs" => source_arg_ty.is_none_or(|source_arg_ty| {
            matches!(
                source_arg_ty,
                PhpType::Bool
                    | PhpType::Float
                    | PhpType::Int
                    | PhpType::Mixed
                    | PhpType::Never
                    | PhpType::TaggedScalar
                    | PhpType::Union(_)
                    | PhpType::Void
            )
        }),
        "boolval" => source_arg_ty.is_some_and(|source_arg_ty| {
            matches!(
                source_arg_ty,
                PhpType::AssocArray { .. }
                    | PhpType::Array(_)
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Int
                    | PhpType::Iterable
                    | PhpType::Never
                    | PhpType::Str
                    | PhpType::Void
            )
        }),
        "floatval" => source_arg_ty.is_some_and(|source_arg_ty| {
            matches!(
                source_arg_ty,
                PhpType::Bool
                    | PhpType::Float
                    | PhpType::Int
                    | PhpType::Never
                    | PhpType::Str
                    | PhpType::Void
            )
        }),
        "intval" => source_arg_ty.is_none_or(|source_arg_ty| {
            matches!(
                source_arg_ty,
                PhpType::Bool
                    | PhpType::Float
                    | PhpType::Int
                    | PhpType::Mixed
                    | PhpType::Never
                    | PhpType::Str
                    | PhpType::Union(_)
                    | PhpType::Void
            )
        }),
        "strlen" => source_arg_ty.is_none_or(|source_arg_ty| {
            matches!(
                source_arg_ty,
                PhpType::Mixed | PhpType::Str | PhpType::Union(_)
            )
        }),
        "strtolower" | "strtoupper" | "trim" => source_arg_ty.is_none_or(|source_arg_ty| {
            matches!(source_arg_ty, PhpType::Str)
        }),
        "gettype" => true,
        _ => false,
    }
}

/// Returns true when a builtin has a generic runtime wrapper implementation.
fn runtime_builtin_name_supported(name: &str) -> bool {
    matches!(
        name,
        "abs"
            | "boolval"
            | "floatval"
            | "gettype"
            | "intval"
            | "strlen"
            | "strtolower"
            | "strtoupper"
            | "trim"
    )
}

/// Builds a static-method runtime wrapper signature that can receive keyed variadic tails.
pub(crate) fn static_method_runtime_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
    let mut wrapper_sig = callable_wrapper_sig(sig);
    if wrapper_sig.variadic.is_some() {
        if let Some((_, ty)) = wrapper_sig.params.last_mut() {
            *ty = PhpType::Iterable;
        }
    }
    wrapper_sig
}

/// Emits assembly for branch if callable case mismatch.
pub(crate) fn emit_branch_if_callable_case_mismatch(
    selector: &RuntimeCallableSelector<'_>,
    case: &RuntimeCallableCase,
    next_case: &str,
    emitter: &mut Emitter,
    matched_label: &str,
    data: &mut DataSection,
) {
    match selector {
        RuntimeCallableSelector::StringNameStack {
            ptr_offset,
            len_offset,
            call_reg,
        } => {
            emit_branch_if_string_name_mismatch(
                case,
                *ptr_offset,
                *len_offset,
                call_reg,
                next_case,
                matched_label,
                emitter,
                data,
            );
        }
    }
}

/// Computes the callable signature metadata for specialized runtime case.
pub(crate) fn specialized_runtime_case_sig(
    sig: &FunctionSig,
    source_elem_ty: Option<&PhpType>,
) -> FunctionSig {
    let Some(source_elem_ty) = source_elem_ty else {
        return sig.clone();
    };
    let mut sig = sig.clone();
    let source_ty = source_elem_ty.codegen_repr();
    if matches!(source_ty, PhpType::Void | PhpType::Never) {
        return sig;
    }
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    for i in 0..regular_param_count {
        if sig.declared_params.get(i).copied().unwrap_or(false)
            || sig.ref_params.get(i).copied().unwrap_or(false)
        {
            continue;
        }
        if let Some((_, param_ty)) = sig.params.get_mut(i) {
            if !matches!(param_ty.codegen_repr(), PhpType::Int | PhpType::Mixed) {
                continue;
            }
            *param_ty = source_ty.clone();
        }
    }
    if sig.variadic.is_some() {
        let variadic_idx = visible_param_count.saturating_sub(1);
        if !sig
            .declared_params
            .get(variadic_idx)
            .copied()
            .unwrap_or(false)
        {
            if let Some((_, param_ty)) = sig.params.get_mut(variadic_idx) {
                *param_ty = PhpType::Array(Box::new(source_ty));
            }
        }
    }
    sig
}

/// Emits assembly for branch if string name mismatch.
#[allow(clippy::too_many_arguments)]
fn emit_branch_if_string_name_mismatch(
    case: &RuntimeCallableCase,
    ptr_offset: usize,
    len_offset: usize,
    call_reg: &str,
    next_case: &str,
    matched_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let Some(php_name) = case.php_name.as_ref() else {
        abi::emit_jump(emitter, next_case);
        return;
    };

    let mut candidates = vec![php_name.clone()];
    if !php_name.starts_with('\\') {
        candidates.push(format!("\\{}", php_name));
    }

    for candidate in candidates {
        emit_string_name_compare(
            ptr_offset,
            len_offset,
            candidate.as_bytes(),
            &matched_label,
            emitter,
            data,
        );
    }
    abi::emit_jump(emitter, next_case);

    emitter.label(&matched_label);
    abi::emit_symbol_address(emitter, call_reg, &case.descriptor_label);
}

/// Emits assembly for string name compare.
fn emit_string_name_compare(
    ptr_offset: usize,
    len_offset: usize,
    candidate: &[u8],
    matched_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (candidate_label, candidate_len) = data.add_string(candidate);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "x2", len_offset);
            abi::emit_symbol_address(emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(emitter, "x4", candidate_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("cmp x0, #0");                                  // did the runtime string callback name match this userland target?
            emitter.instruction(&format!("b.eq {}", matched_label));            // select this callable case when names match case-insensitively
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", ptr_offset);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", len_offset);
            abi::emit_symbol_address(emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(emitter, "__rt_strcasecmp");
            emitter.instruction("test rax, rax");                               // did the runtime string callback name match this userland target?
            emitter.instruction(&format!("je {}", matched_label));              // select this callable case when names match case-insensitively
        }
    }
}
