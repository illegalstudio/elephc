//! Purpose:
//! Defines runtime callable dispatch metadata shared by indirect callback emitters.
//! Bridges AOT function signatures with runtime-selected callable values or names.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::call_user_func_array`
//!
//! Key details:
//! - Cases carry the ABI entry label, optional PHP-visible name, signature metadata, and hidden captures.
//! - String-name dispatch compares against userland callable names before loading the matched entry address.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::types::{FunctionSig, PhpType};

#[derive(Clone)]
pub(crate) struct RuntimeCallableCase {
    pub(crate) label: String,
    pub(crate) php_name: Option<String>,
    pub(crate) sig: FunctionSig,
    pub(crate) captures: Vec<(String, PhpType, bool)>,
}

pub(crate) enum RuntimeCallableSelector<'a> {
    Address(&'a str),
    StringNameStack {
        ptr_offset: usize,
        len_offset: usize,
        call_reg: &'a str,
    },
}

pub(crate) fn runtime_callable_cases(
    ctx: &mut Context,
    captures: &[(String, PhpType, bool)],
    source_elem_ty: Option<&PhpType>,
) -> Vec<RuntimeCallableCase> {
    let mut cases = Vec::new();
    for (name, sig) in &ctx.functions {
        if ctx.extern_functions.contains_key(name) {
            continue;
        }
        cases.push(RuntimeCallableCase {
            label: function_symbol(name),
            php_name: Some(name.clone()),
            sig: specialized_runtime_case_sig(sig, source_elem_ty),
            captures: Vec::new(),
        });
    }
    for deferred in &mut ctx.deferred_closures {
        if deferred.hidden_params.as_slice() != captures {
            continue;
        }
        let sig = specialized_runtime_case_sig(&deferred.sig, source_elem_ty);
        deferred.sig = sig.clone();
        cases.push(RuntimeCallableCase {
            label: deferred.label.clone(),
            php_name: None,
            sig,
            captures: captures.to_vec(),
        });
    }
    cases.sort_by(|left, right| left.label.cmp(&right.label));
    cases.dedup_by(|left, right| left.label == right.label);
    cases
}

pub(crate) fn emit_branch_if_callable_case_mismatch(
    selector: &RuntimeCallableSelector<'_>,
    case: &RuntimeCallableCase,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match selector {
        RuntimeCallableSelector::Address(call_reg) => {
            emit_branch_if_address_mismatch(call_reg, &case.label, next_case, emitter);
        }
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
                emitter,
                ctx,
                data,
            );
        }
    }
}

fn specialized_runtime_case_sig(
    sig: &FunctionSig,
    source_elem_ty: Option<&PhpType>,
) -> FunctionSig {
    let Some(source_elem_ty) = source_elem_ty else {
        return sig.clone();
    };
    let mut sig = sig.clone();
    let visible_param_count = sig.params.len();
    let regular_param_count = if sig.variadic.is_some() {
        visible_param_count.saturating_sub(1)
    } else {
        visible_param_count
    };
    let source_ty = source_elem_ty.codegen_repr();
    for i in 0..regular_param_count {
        if sig.declared_params.get(i).copied().unwrap_or(false)
            || sig.ref_params.get(i).copied().unwrap_or(false)
        {
            continue;
        }
        if let Some((_, param_ty)) = sig.params.get_mut(i) {
            if !matches!(param_ty.codegen_repr(), PhpType::Int) {
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

fn emit_branch_if_address_mismatch(
    call_reg: &str,
    candidate_label: &str,
    next_case: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", candidate_label);
            emitter.instruction(&format!("cmp {}, x9", call_reg));              // does the runtime callable pointer match this AOT signature case?
            emitter.instruction(&format!("b.ne {}", next_case));                // try the next callable signature case when the pointer differs
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", candidate_label);
            emitter.instruction(&format!("cmp {}, r10", call_reg));             // does the runtime callable pointer match this AOT signature case?
            emitter.instruction(&format!("jne {}", next_case));                 // try the next callable signature case when the pointer differs
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_branch_if_string_name_mismatch(
    case: &RuntimeCallableCase,
    ptr_offset: usize,
    len_offset: usize,
    call_reg: &str,
    next_case: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let Some(php_name) = case.php_name.as_ref() else {
        abi::emit_jump(emitter, next_case);
        return;
    };

    let matched_label = ctx.next_label("callable_string_match");
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
    abi::emit_symbol_address(emitter, call_reg, &case.label);
}

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
