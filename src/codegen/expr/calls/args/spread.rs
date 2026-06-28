//! Purpose:
//! Lowers positional and named spread argument expansion.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection, functions};
use crate::parser::ast::Expr;
use crate::types::{FunctionSig, PhpType};

use super::array_elements::{
    array_element_stride, emit_hash_lookup_for_param_or_index, load_array_element_to_result,
    push_loaded_array_element_arg, push_loaded_hash_value_arg, spread_source_elem_ty,
};
use super::common::{declared_target_ty, push_expr_arg};
use super::variadic::variadic_container_elem_ty;

/// Emits code that unpacks a spread array's elements into the remaining named parameter slots.
/// For positional (non-assoc) spreads, emits a length check before accessing elements to ensure
/// required parameters are covered. For assoc spreads, performs key-based lookups against
/// parameter names. Each element is pushed as an ABI-ready argument and its type appended to `arg_types`.
/// Returns early with no emitted code if `remaining == 0`.
pub(crate) fn emit_spread_into_named_params(
    spread_expr: &Expr,
    sig: Option<&FunctionSig>,
    spread_at_index: usize,
    regular_param_count: usize,
    context_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    arg_types: &mut Vec<PhpType>,
) {
    let remaining = regular_param_count.saturating_sub(spread_at_index);
    if remaining == 0 {
        return;
    }

    emitter.comment(&format!("unpack spread into {} {}", remaining, context_label));
    let spread_ty = functions::infer_contextual_type(spread_expr, ctx);
    let source_elem_ty = spread_source_elem_ty(&spread_ty);
    let elem_stride = array_element_stride(&source_elem_ty);
    let _ = super::super::super::emit_expr(spread_expr, emitter, ctx, data);
    let array_base_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x20",
        crate::codegen::platform::Arch::X86_64 => "r12",
    };
    emitter.instruction(&format!("mov {}, {}", array_base_reg, abi::int_result_reg(emitter))); // preserve the spread array pointer across boxing or incref helper calls
    let min_required = (0..remaining)
        .filter(|idx| {
            sig.and_then(|sig| sig.defaults.get(spread_at_index + idx))
                .and_then(|default| default.as_ref())
                .is_none()
        })
        .map(|idx| idx + 1)
        .max()
        .unwrap_or(0);
    if min_required > 0 {
        emit_spread_required_length_check(array_base_reg, min_required, emitter, ctx, data);
    }
    for idx in 0..remaining {
        let target_ty = declared_target_ty(sig, spread_at_index + idx);
        let default = sig
            .and_then(|sig| sig.defaults.get(spread_at_index + idx))
            .and_then(|default| default.as_ref());
        let pushed_ty = if matches!(spread_ty, PhpType::AssocArray { .. }) {
            let param_name = sig
                .and_then(|sig| sig.params.get(spread_at_index + idx))
                .map(|(name, _)| name.as_str());
            push_assoc_spread_element_or_default_arg(
                array_base_reg,
                param_name,
                idx,
                &source_elem_ty,
                default,
                target_ty,
                emitter,
                ctx,
                data,
            )
        } else {
            push_spread_element_or_default_arg(
                array_base_reg,
                idx,
                elem_stride,
                &source_elem_ty,
                default,
                target_ty,
                emitter,
                ctx,
                data,
            )
        };
        arg_types.push(pushed_ty);
    }
}

/// Generates a bounds check that aborts if the spread array contains fewer than `min_len` elements.
/// Loads the spread array length from `array_base_reg`, compares against `min_len`, and branches to
/// `emit_spread_too_few_args_abort` on failure. On success falls through to the next label.
fn emit_spread_required_length_check(
    array_base_reg: &str,
    min_len: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let ok_label = ctx.next_label("spread_required_len_ok");
    let fail_label = ctx.next_label("spread_required_len_fail");
    emitter.comment("validate spread covers required parameters");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [{}]", array_base_reg));      // load spread length before reading required unpacked parameters
            abi::emit_load_int_immediate(emitter, "x10", min_len as i64);
            emitter.instruction("cmp x9, x10");                                 // ensure the spread provides every required positional parameter
            emitter.instruction(&format!("b.ge {}", ok_label));                 // continue when all required spread slots are available
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [{}]", array_base_reg)); // load spread length before reading required unpacked parameters
            abi::emit_load_int_immediate(emitter, "r11", min_len as i64);
            emitter.instruction("cmp r10, r11");                                // ensure the spread provides every required positional parameter
            emitter.instruction(&format!("jge {}", ok_label));                  // continue when all required spread slots are available
        }
    }
    emitter.label(&fail_label);
    emit_spread_too_few_args_abort(emitter, data);
    emitter.label(&ok_label);
}

/// Emits a fatal runtime abort with a "too few arguments" diagnostic message.
/// Writes a fixed string to stderr and exits with code 1. Used when a spread provides
/// insufficient elements for required parameters.
fn emit_spread_too_few_args_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) =
        data.add_string(b"Fatal error: too few arguments for spread call\n");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the spread arity diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the spread arity diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal spread arity diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

/// Emits code to push either a spread element at `element_idx` or a default expression to the ABI.
/// For non-assoc (positional) spread arrays. Reads the element from the spread array at offset
/// `24 + element_idx * elem_stride` (skipping the array header). If `default` is present and the
/// spread is too short, jumps to the default path. Returns the widnened PHP type of the pushed argument.
#[allow(clippy::too_many_arguments)]
fn push_spread_element_or_default_arg(
    array_base_reg: &str,
    element_idx: usize,
    elem_stride: usize,
    source_elem_ty: &PhpType,
    default: Option<&Expr>,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if let Some(default) = default {
        let use_default = ctx.next_label("spread_default");
        let done = ctx.next_label("spread_done");
        emit_branch_if_spread_element_missing(array_base_reg, element_idx, &use_default, emitter);
        load_array_element_to_result(
            emitter,
            source_elem_ty,
            array_base_reg,
            24 + element_idx * elem_stride,
        );
        let loaded_ty =
            push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data);
        abi::emit_jump(emitter, &done);
        emitter.label(&use_default);
        let default_ty = push_expr_arg(default, target_ty, emitter, ctx, data);
        emitter.label(&done);
        return super::super::super::widen_codegen_type(&loaded_ty, &default_ty);
    }

    load_array_element_to_result(
        emitter,
        source_elem_ty,
        array_base_reg,
        24 + element_idx * elem_stride,
    );
    push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data)
}

/// Emits code to push either an associative spread element matching `param_name` or a default expression.
/// Performs a hash lookup for `param_name` in the spread array. If found, pushes the loaded value;
/// if not found and a default exists, pushes the default expression. If no default and the key is
/// missing, aborts with a fatal error. Returns the widnened PHP type of the pushed argument.
#[allow(clippy::too_many_arguments)]
fn push_assoc_spread_element_or_default_arg(
    hash_base_reg: &str,
    param_name: Option<&str>,
    element_idx: usize,
    source_elem_ty: &PhpType,
    default: Option<&Expr>,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("lookup associative spread argument");
    emit_hash_lookup_for_param_or_index(
        hash_base_reg,
        param_name,
        element_idx,
        emitter,
        ctx,
        data,
    );

    if let Some(default) = default {
        let use_default = ctx.next_label("assoc_spread_default");
        let done = ctx.next_label("assoc_spread_done");
        abi::emit_branch_if_int_result_zero(emitter, &use_default);
        let loaded_ty = push_loaded_hash_value_arg(source_elem_ty, target_ty, emitter, ctx, data);
        abi::emit_jump(emitter, &done);
        emitter.label(&use_default);
        let default_ty = push_expr_arg(default, target_ty, emitter, ctx, data);
        emitter.label(&done);
        return super::super::super::widen_codegen_type(&loaded_ty, &default_ty);
    }

    let missing = ctx.next_label("assoc_spread_missing");
    let done = ctx.next_label("assoc_spread_done");
    abi::emit_branch_if_int_result_zero(emitter, &missing);
    let loaded_ty = push_loaded_hash_value_arg(source_elem_ty, target_ty, emitter, ctx, data);
    abi::emit_jump(emitter, &done);
    emitter.label(&missing);
    emit_spread_too_few_args_abort(emitter, data);
    emitter.label(&done);
    loaded_ty
}

/// Emits a conditional branch to `label` if the spread array has fewer than `element_idx + 1` elements.
/// Loads the spread array length from `array_base_reg` and compares against `element_idx`.
/// On AArch64 uses `x9`/`x10`; on x86_64 uses `r10`/`r11`. Branches to `label` when the spread
/// is too short to contain this element (i.e., the element is missing and a default should be used).
fn emit_branch_if_spread_element_missing(
    array_base_reg: &str,
    element_idx: usize,
    label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [{}]", array_base_reg));      // load spread length before choosing spread element or default
            abi::emit_load_int_immediate(emitter, "x10", element_idx as i64);
            emitter.instruction("cmp x9, x10");                                 // check whether this optional spread element exists
            emitter.instruction(&format!("b.le {}", label));                    // use the default when the spread is too short for this slot
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [{}]", array_base_reg)); // load spread length before choosing spread element or default
            abi::emit_load_int_immediate(emitter, "r11", element_idx as i64);
            emitter.instruction("cmp r10, r11");                                // check whether this optional spread element exists
            emitter.instruction(&format!("jle {}", label));                     // use the default when the spread is too short for this slot
        }
    }
}

/// Emits a variadic array from the tail of a spread expression starting at a given offset.
pub(crate) fn emit_spread_tail_variadic_array_arg(
    spread_expr: &Expr,
    sig: Option<&FunctionSig>,
    tail_start: usize,
    regular_param_count: usize,
    context_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(context_label);
    let spread_ty = super::super::super::emit_expr(spread_expr, emitter, ctx, data);
    if matches!(spread_ty.codegen_repr(), PhpType::Iterable) {
        return emit_iterable_spread_tail_variadic_array_arg(
            sig,
            tail_start,
            regular_param_count,
            emitter,
            ctx,
            data,
        );
    }
    if matches!(spread_ty, PhpType::AssocArray { .. }) {
        return emit_assoc_spread_tail_variadic_array_arg(
            &spread_ty,
            sig,
            tail_start,
            regular_param_count,
            emitter,
            ctx,
            data,
        );
    }
    emit_indexed_spread_tail_variadic_array_arg(&spread_ty, tail_start, emitter)
}

/// Emits an indexed-array slice for a spread tail that is known to use indexed storage.
fn emit_indexed_spread_tail_variadic_array_arg(
    spread_ty: &PhpType,
    tail_start: usize,
    emitter: &mut Emitter,
) -> PhpType {
    let source_elem_ty = spread_source_elem_ty(spread_ty);
    let container_elem_ty = variadic_container_elem_ty(&source_elem_ty);
    if emitter.target.arch == crate::codegen::platform::Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // pass the spread source array pointer to the x86_64 slice helper
    }
    let offset_reg = abi::int_arg_reg_name(emitter.target, 1);
    let length_reg = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_load_int_immediate(emitter, offset_reg, tail_start as i64);
    abi::emit_load_int_immediate(emitter, length_reg, -1);
    let helper = if source_elem_ty.codegen_repr().is_refcounted() {
        "__rt_array_slice_refcounted"
    } else {
        "__rt_array_slice"
    };
    abi::emit_call_label(emitter, helper);
    super::super::super::arrays::emit_array_value_type_stamp(
        emitter,
        abi::int_result_reg(emitter),
        &container_elem_ty,
    );
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(container_elem_ty.clone())));
    PhpType::Array(Box::new(container_elem_ty))
}

/// Emits a keyed variadic hash for a spread tail that is known to use associative storage.
fn emit_assoc_spread_tail_variadic_array_arg(
    spread_ty: &PhpType,
    sig: Option<&FunctionSig>,
    tail_start: usize,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let source_elem_ty = spread_source_elem_ty(spread_ty);
    let source_hash_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x20",
        crate::codegen::platform::Arch::X86_64 => "r13",
    };
    emitter.instruction(&format!("mov {}, {}", source_hash_reg, abi::int_result_reg(emitter))); // preserve the spread source hash while building the variadic tail

    let fallback_sig;
    let effective_sig = if let Some(sig) = sig {
        sig
    } else {
        fallback_sig = fallback_variadic_sig();
        &fallback_sig
    };
    super::emit_loaded_assoc_variadic_array_arg(
        source_hash_reg,
        &source_elem_ty,
        effective_sig,
        tail_start,
        regular_param_count,
        "build associative spread variadic tail",
        emitter,
        ctx,
        data,
    )
}

/// Emits runtime dispatch for an `Iterable` spread tail, preserving indexed and hash layouts.
fn emit_iterable_spread_tail_variadic_array_arg(
    sig: Option<&FunctionSig>,
    tail_start: usize,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let indexed_case = ctx.next_label("spread_iterable_indexed");
    let hash_case = ctx.next_label("spread_iterable_hash");
    let done_label = ctx.next_label("spread_iterable_done");
    let source_hash_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x20",
        crate::codegen::platform::Arch::X86_64 => "r13",
    };

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve iterable spread pointer across heap-kind dispatch
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // classify the iterable spread payload by runtime heap kind
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("cmp x0, #2");                                  // is the iterable spread backed by an indexed array?
            emitter.instruction(&format!("b.eq {}", indexed_case));             // slice indexed iterables using the array-tail path
            emitter.instruction("cmp x0, #3");                                  // is the iterable spread backed by an associative hash?
            emitter.instruction(&format!("b.eq {}", hash_case));                // rebuild associative iterables as a keyed variadic hash
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("cmp rax, 2");                                  // is the iterable spread backed by an indexed array?
            emitter.instruction(&format!("je {}", indexed_case));               // slice indexed iterables using the array-tail path
            emitter.instruction("cmp rax, 3");                                  // is the iterable spread backed by an associative hash?
            emitter.instruction(&format!("je {}", hash_case));                  // rebuild associative iterables as a keyed variadic hash
        }
    }
    abi::emit_call_label(emitter, "__rt_iterable_unsupported_kind");            // reject non-array iterable spread tails for call forwarding

    emitter.label(&indexed_case);
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    abi::emit_pop_reg(emitter, array_arg_reg);                                  // restore indexed iterable pointer for slicing
    emit_indexed_mixed_spread_tail_slice(tail_start, emitter);
    let indexed_ty = PhpType::Array(Box::new(PhpType::Mixed));
    abi::emit_push_result_value(emitter, &indexed_ty);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&hash_case);
    abi::emit_pop_reg(emitter, source_hash_reg);                                // restore associative iterable pointer for keyed tail construction
    let fallback_sig;
    let effective_sig = if let Some(sig) = sig {
        sig
    } else {
        fallback_sig = fallback_variadic_sig();
        &fallback_sig
    };
    super::emit_loaded_assoc_variadic_array_arg(
        source_hash_reg,
        &PhpType::Mixed,
        effective_sig,
        tail_start,
        regular_param_count,
        "build associative iterable variadic tail",
        emitter,
        ctx,
        data,
    );
    abi::emit_jump(emitter, &done_label);

    emitter.label(&done_label);
    PhpType::Iterable
}

/// Emits an indexed Mixed slice from an already-restored iterable array pointer.
fn emit_indexed_mixed_spread_tail_slice(tail_start: usize, emitter: &mut Emitter) {
    let offset_reg = abi::int_arg_reg_name(emitter.target, 1);
    let length_reg = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_load_int_immediate(emitter, offset_reg, tail_start as i64);
    abi::emit_load_int_immediate(emitter, length_reg, -1);
    abi::emit_call_label(emitter, "__rt_array_slice_refcounted");
    super::super::super::arrays::emit_array_value_type_stamp(
        emitter,
        abi::int_result_reg(emitter),
        &PhpType::Mixed,
    );
}

/// Builds a permissive variadic signature for unreachable no-signature spread fallback paths.
fn fallback_variadic_sig() -> FunctionSig {
    FunctionSig {
        params: vec![(
            "rest".to_string(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        )],
        defaults: vec![None],
        return_type: PhpType::Mixed,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false],
        declared_params: vec![false],
        variadic: Some("rest".to_string()),
        deprecation: None,
    }
}
