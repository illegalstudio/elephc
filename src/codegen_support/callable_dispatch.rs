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

/// The blanket refusal `runtime_fn_semantics` attaches to every typed backend builtin.
///
/// It is a policy default rather than a finding about any particular builtin, which is why
/// [`generic_wrapper_is_expressible`] may look past it. The four OTHER `StaticOnly` reasons name
/// a real obstruction — a compiler primitive, the array-pointer cursor, `constant()`'s
/// compile-time name, `fscanf`'s prelude — and are never looked past.
const TYPED_BACKEND_REFUSAL: &str = "typed backend operation has no runtime-selected wrapper contract";

/// Returns true for builtins supported by generic runtime string-callable dispatch.
///
/// `narrowed` says whether the compiler knows which names this site can reach. It has to, because
/// the ladder is inlined per call site and each case is the builtin's WHOLE lowering: measured at
/// roughly 120 lines of assembly per eligible builtin per site. A site whose callee cannot be
/// narrowed therefore keeps the small hand-declared set it has always had, and a site that knows
/// its names pays only for those.
pub(crate) fn runtime_builtin_wrapper_supported(
    name: &str,
    source_arg_ty: Option<&PhpType>,
    narrowed: bool,
) -> bool {
    let name = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    let Some(def) = crate::builtins::registry::lookup(&name) else {
        return false;
    };
    match def.spec.semantics.callable {
        crate::builtins::semantics::BuiltinCallablePolicy::Dynamic(accepts) => {
            accepts(source_arg_ty)
        }
        crate::builtins::semantics::BuiltinCallablePolicy::DynamicRuntime(target) => {
            target.callable_accepts(source_arg_ty)
        }
        crate::builtins::semantics::BuiltinCallablePolicy::StaticOnly(reason) => {
            narrowed && reason == TYPED_BACKEND_REFUSAL && generic_wrapper_is_expressible(def)
        }
    }
}

/// Returns whether the generic PHP-ABI wrapper can express this builtin's signature.
///
/// The wrapper receives its arguments by value and answers one result, so a by-reference
/// parameter has nowhere to write back and a variadic tail has no fixed arity to declare. Those
/// two are the whole obstruction; the caller still requires a first-class callable signature to
/// exist, which is what rules out the shapes neither of these tests would catch.
fn generic_wrapper_is_expressible(def: &crate::builtins::registry::BuiltinDef) -> bool {
    def.spec.variadic.is_none()
        && !def.spec.params.iter().any(|param| param.by_ref)
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
