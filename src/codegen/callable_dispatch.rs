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
use crate::codegen::context::{Context, DeferredClosure};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::{function_symbol, Name};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, StmtKind, Visibility};
use crate::span::Span;
use crate::types::{
    callable_wrapper_sig, first_class_callable_builtin_sig, FunctionSig, PhpType,
};
use crate::types::checker::builtins::supported_builtin_function_names;

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

/// Provides the Runtime callable cases helper used by the callable dispatch module.
pub(crate) fn runtime_callable_cases(
    ctx: &mut Context,
    captures: &[(String, PhpType, bool)],
    source_elem_ty: Option<&PhpType>,
) -> Vec<RuntimeCallableCase> {
    let mut cases = Vec::new();
    if captures.is_empty() {
        for name in supported_builtin_function_names() {
            if runtime_builtin_wrapper_excluded(name) {
                continue;
            }
            let Some(sig) = first_class_callable_builtin_sig(name) else {
                continue;
            };
            let wrapper_sig = callable_wrapper_sig(&sig);
            let label = ensure_runtime_builtin_wrapper(ctx, name, &wrapper_sig);
            cases.push(RuntimeCallableCase {
                label,
                php_name: Some((*name).to_string()),
                sig: specialized_runtime_case_sig(&wrapper_sig, source_elem_ty),
                captures: Vec::new(),
            });
        }
        for (class_name, method_name, sig) in runtime_static_method_wrappers(ctx) {
            let wrapper_sig = callable_wrapper_sig(&sig);
            let label =
                ensure_runtime_static_method_wrapper(ctx, &class_name, &method_name, &wrapper_sig);
            cases.push(RuntimeCallableCase {
                label,
                php_name: Some(format!("{}::{}", class_name, method_name)),
                sig: specialized_runtime_case_sig(&wrapper_sig, source_elem_ty),
                captures: Vec::new(),
            });
        }
    }
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

/// Provides the Runtime static method wrappers helper used by the callable dispatch module.
fn runtime_static_method_wrappers(ctx: &Context) -> Vec<(String, String, FunctionSig)> {
    let mut wrappers = Vec::new();
    for (class_name, class_info) in &ctx.classes {
        for (method_name, sig) in &class_info.static_methods {
            if !class_info
                .static_method_visibilities
                .get(method_name)
                .is_some_and(|visibility| matches!(visibility, Visibility::Public))
            {
                continue;
            }
            wrappers.push((class_name.clone(), method_name.clone(), sig.clone()));
        }
    }
    wrappers.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    wrappers
}

/// Provides the Runtime builtin wrapper excluded helper used by the callable dispatch module.
fn runtime_builtin_wrapper_excluded(name: &str) -> bool {
    matches!(name, "iterator_apply" | "preg_replace_callback")
}

/// Ensures runtime builtin wrapper is available before the caller continues.
fn ensure_runtime_builtin_wrapper(
    ctx: &mut Context,
    name: &str,
    sig: &FunctionSig,
) -> String {
    if let Some(label) = ctx.runtime_callable_builtin_wrappers.get(name) {
        return label.clone();
    }

    let label = ctx.next_label("callable_builtin");
    let params: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: label.clone(),
        params,
        body: builtin_wrapper_body(name, sig),
        sig: sig.clone(),
        captures: Vec::new(),
        hidden_params: Vec::new(),
        current_class: None,
        needed: true,
    });
    ctx.runtime_callable_builtin_wrappers
        .insert(name.to_string(), label.clone());
    label
}

/// Ensures runtime static method wrapper is available before the caller continues.
fn ensure_runtime_static_method_wrapper(
    ctx: &mut Context,
    class_name: &str,
    method_name: &str,
    sig: &FunctionSig,
) -> String {
    let key = format!("{}::{}", class_name, method_name);
    if let Some(label) = ctx.runtime_callable_static_method_wrappers.get(&key) {
        return label.clone();
    }

    let label = ctx.next_label("callable_static_method");
    let params: Vec<String> = sig.params.iter().map(|(name, _)| name.clone()).collect();
    ctx.deferred_closures.push(DeferredClosure {
        label: label.clone(),
        params,
        body: static_method_wrapper_body(class_name, method_name, sig),
        sig: sig.clone(),
        captures: Vec::new(),
        hidden_params: Vec::new(),
        current_class: None,
        needed: true,
    });
    ctx.runtime_callable_static_method_wrappers
        .insert(key, label.clone());
    label
}

/// Builds the synthetic method body for static method wrapper.
fn static_method_wrapper_body(class_name: &str, method_name: &str, sig: &FunctionSig) -> Vec<Stmt> {
    let last_param_idx = sig.params.len().saturating_sub(1);
    let args: Vec<Expr> = sig
        .params
        .iter()
        .enumerate()
        .map(|(idx, (param_name, _))| {
            let var = Expr::new(ExprKind::Variable(param_name.clone()), Span::dummy());
            if sig.variadic.is_some() && idx == last_param_idx {
                Expr::new(ExprKind::Spread(Box::new(var)), Span::dummy())
            } else {
                var
            }
        })
        .collect();
    let call = Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(Name::from(class_name.to_string())),
            method: method_name.to_string(),
            args,
        },
        Span::dummy(),
    );

    if sig.return_type == PhpType::Void {
        vec![
            Stmt::new(StmtKind::ExprStmt(call), Span::dummy()),
            Stmt::new(StmtKind::Return(None), Span::dummy()),
        ]
    } else {
        vec![Stmt::new(StmtKind::Return(Some(call)), Span::dummy())]
    }
}

/// Builds the synthetic method body for builtin wrapper.
fn builtin_wrapper_body(name: &str, sig: &FunctionSig) -> Vec<Stmt> {
    let last_param_idx = sig.params.len().saturating_sub(1);
    let args: Vec<Expr> = sig
        .params
        .iter()
        .enumerate()
        .map(|(idx, (param_name, _))| {
            let var = Expr::new(ExprKind::Variable(param_name.clone()), Span::dummy());
            if sig.variadic.is_some() && idx == last_param_idx {
                Expr::new(ExprKind::Spread(Box::new(var)), Span::dummy())
            } else {
                var
            }
        })
        .collect();
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified(name),
            args,
        },
        Span::dummy(),
    );

    if sig.return_type == PhpType::Void {
        vec![
            Stmt::new(StmtKind::ExprStmt(call), Span::dummy()),
            Stmt::new(StmtKind::Return(None), Span::dummy()),
        ]
    } else {
        vec![Stmt::new(StmtKind::Return(Some(call)), Span::dummy())]
    }
}

/// Emits assembly for branch if callable case mismatch.
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

/// Computes the callable signature metadata for specialized runtime case.
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

/// Emits assembly for branch if address mismatch.
fn emit_branch_if_address_mismatch(
    call_reg: &str,
    candidate_label: &str,
    next_case: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", candidate_label);
            emitter.instruction(&format!("cmp {}, x9", call_reg));              // does the runtime callable entry match this AOT signature case?
            emitter.instruction(&format!("b.ne {}", next_case));                // try the next callable signature case when the pointer differs
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", candidate_label);
            emitter.instruction(&format!("cmp {}, r10", call_reg));             // does the runtime callable entry match this AOT signature case?
            emitter.instruction(&format!("jne {}", next_case));                 // try the next callable signature case when the pointer differs
        }
    }
}

/// Emits assembly for branch if string name mismatch.
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
