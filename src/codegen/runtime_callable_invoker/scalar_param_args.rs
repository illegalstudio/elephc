//! Purpose:
//! Checks a boxed argument a descriptor invoker binds to a declared non-nullable `int`, `float`,
//! `string` or `bool` parameter, raising PHP's `TypeError` instead of applying the lenient cast
//! the invoker would otherwise use (`null` to `""`, `"12abc"` to `12`, an array to `1`).
//!
//! Called from:
//! - `owned_value_args::coerce()`, for the parameter `InvokerEmitContext::select_literal_parameter`
//!   selected, when the source is a boxed `mixed`/union value.
//!
//! Key details:
//! - Calls through a callable value (`call_user_func*()`, a callable held in a variable, a
//!   callback a builtin such as `array_map()` invokes) bind their arguments here. Invoker bodies
//!   are shared per signature, so the callee name comes from the descriptor, like the literal
//!   parameter guard's, and the message has no "called in" suffix (PHP's spelling for a callback
//!   a builtin invokes).
//! - Strictness follows the invocation profile word the caller passed: `-2` (strict global) and
//!   `-id-3` (strict class scope) select `strict_types=1`.
//! - `string`/`bool` leave the borrowed argument in the result register for the ordinary cast
//!   that follows. `int`/`float` call the module helpers `scalar_param_guard` emits with a
//!   `FunctionContext` (numeric strings, the precision deprecation, the range refusal) and leave
//!   the converted value instead.
//! - The argument is borrowed from the container; a refusal leaves every owner to the invoker's
//!   escape path, as the literal parameter guard does.

use super::*;
use crate::codegen::lower_inst::{
    invoker_numeric_status_reg, refuse_invoker_argument_static, refuse_invoker_argument_loaded,
    INVOKER_FLOAT_PARAM_HELPER, INVOKER_INT_PARAM_HELPER,
};

/// Module facts the invoker needs to check scalar parameters.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::codegen) struct InvokerScalarGuards {
    /// `Stringable`'s metadata id and kind for `__rt_mixed_instanceof`, when the program has it.
    pub(in crate::codegen) stringable: Option<(u64, i64)>,
    /// Whether the program links the eval bridge: an `eval()`-declared class backs its objects
    /// with a native `stdClass`, so only the bridge knows it implements `Stringable`.
    pub(in crate::codegen) eval_bridge: bool,
    /// The callee name for the message when the descriptor's own name is not PHP's (a
    /// closure's `{closure:file:line}`); `None` reads the descriptor name at run time.
    pub(in crate::codegen) callee: Option<String>,
    /// The callee is a method of an internal `ext/uri` class: coercive callers bind as before
    /// (PHP coerces `null` with a deprecation) and only `strict_types=1` callers are checked.
    pub(in crate::codegen) strict_only: bool,
}

/// The parameter whose boxed argument is checked, selected per container entry.
#[derive(Clone, Debug)]
pub(super) struct ScalarParameter {
    pub(super) index: usize,
    pub(super) name: String,
    pub(super) declared: PhpType,
}

/// Returns the scalar parameter `index` a guarded invoker checks, if any.
pub(super) fn select(sig: &FunctionSig, index: usize) -> Option<ScalarParameter> {
    let declared = crate::types::param_binding::runtime_scalar_guard_param(sig, index)?;
    Some(ScalarParameter { index, name: sig.params[index].0.clone(), declared: declared.clone() })
}

/// Checks the borrowed boxed argument in the integer result register against `parameter`.
///
/// Returns the pushed type when the check also converted the value (`int`/`float`), or `None`
/// when the argument is left in place for the ordinary cast (`string`/`bool`).
pub(super) fn emit_guard(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    guards: &InvokerScalarGuards,
    parameter: &ScalarParameter,
) -> Option<PhpType> {
    // The internal URI methods declare no checked `int`/`float` parameter a program can reach;
    // such a parameter keeps the ordinary cast rather than a strict-only conversion path.
    if guards.strict_only && matches!(parameter.declared, PhpType::Int | PhpType::Float) {
        return None;
    }
    let result = abi::int_result_reg(emitter);
    let strict = ctx.next_label("scalar_param_strict");
    let accept = ctx.next_label("scalar_param_accept");
    let refuse = ctx.next_label("scalar_param_refuse");
    let scratch = abi::symbol_scratch_reg(emitter);
    // -- keep the borrowed argument cell and select the caller's strictness --
    abi::emit_push_reg(emitter, result);
    abi::load_at_offset(emitter, scratch, INVOKER_SCOPE_OFFSET);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("asr {scratch}, {scratch}, #2"));      // recover the weak/strict scope from the encoded profile
            emitter.instruction(&format!("cmn {scratch}, #2"));                 // strict callers pass -2 or -class_id-3
            emitter.instruction(&format!("b.le {strict}"));                     // a profile at or below -2 checks strictly
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("sar {scratch}, 2"));                  // recover the weak/strict scope from the encoded profile
            emitter.instruction(&format!("cmp {scratch}, -2"));                 // strict callers pass -2 or -class_id-3
            emitter.instruction(&format!("jle {strict}"));                      // a profile at or below -2 checks strictly
        }
    }
    let converted = match parameter.declared {
        PhpType::Int | PhpType::Float => {
            emit_numeric(emitter, parameter, &strict, &accept, &refuse);
            Some(parameter.declared.clone())
        }
        _ => {
            if guards.strict_only {
                // -- coercive caller of an internal method: bind as before --
                abi::emit_jump(emitter, &accept);
            }
            emit_string_or_bool(emitter, data, guards, parameter, &strict, &accept, &refuse);
            None
        }
    };
    emit_refusal(emitter, ctx, data, guards, parameter, &refuse);
    // -- accepted: drop the saved cell, keeping the converted value or the original argument --
    emitter.label(&accept);
    if converted.is_some() {
        abi::emit_release_temporary_stack(emitter, 16);
    } else {
        abi::emit_pop_reg(emitter, result);
    }
    converted
}

/// Emits the `int`/`float` checks: the coercive module helper, or the exact strict tags.
fn emit_numeric(
    emitter: &mut Emitter,
    parameter: &ScalarParameter,
    strict: &str,
    accept: &str,
    refuse: &str,
) {
    let result = abi::int_result_reg(emitter);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    let float = parameter.declared == PhpType::Float;
    let status = invoker_numeric_status_reg(emitter.target.arch);
    // -- coercive: PHP's numeric binding rules, in the shared module helper --
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, if float { INVOKER_FLOAT_PARAM_HELPER } else { INVOKER_INT_PARAM_HELPER });
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {status}, {refuse}"));           // the helper refused the value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {status}, {status}"));           // did the helper refuse the value?
            emitter.instruction(&format!("jnz {refuse}"));                      // the helper refused the value
        }
    }
    abi::emit_jump(emitter, accept);
    // -- strict: only the declared tag binds, plus an int widened to float --
    emitter.label(strict);
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            if float {
                emitter.instruction("cmp x0, #2");                              // is the strict argument a float?
                emitter.instruction("b.ne 1f");                                 // otherwise only an int widens
                emitter.instruction(&format!("fmov d0, {payload}"));            // bind the float payload unchanged
                emitter.instruction(&format!("b {accept}"));                    // the float binds
                emitter.label("1");
                emitter.instruction("cmp x0, #0");                              // is the strict argument an int?
                emitter.instruction(&format!("b.ne {refuse}"));                 // every other tag is refused
                emitter.instruction(&format!("scvtf d0, {payload}"));           // widen the int to the float parameter
            } else {
                emitter.instruction("cmp x0, #0");                              // is the strict argument an int?
                emitter.instruction(&format!("b.ne {refuse}"));                 // every other tag is refused
                emitter.instruction(&format!("mov x0, {payload}"));             // bind the int payload
            }
        }
        Arch::X86_64 => {
            if float {
                emitter.instruction("cmp rax, 2");                              // is the strict argument a float?
                emitter.instruction("jne 1f");                                  // otherwise only an int widens
                emitter.instruction(&format!("movq xmm0, {payload}"));          // bind the float payload unchanged
                emitter.instruction(&format!("jmp {accept}"));                  // the float binds
                emitter.label("1");
                emitter.instruction("test rax, rax");                           // is the strict argument an int?
                emitter.instruction(&format!("jnz {refuse}"));                  // every other tag is refused
                emitter.instruction(&format!("cvtsi2sd xmm0, {payload}"));      // widen the int to the float parameter
            } else {
                emitter.instruction("test rax, rax");                           // is the strict argument an int?
                emitter.instruction(&format!("jnz {refuse}"));                  // every other tag is refused
                emitter.instruction(&format!("mov rax, {payload}"));            // bind the int payload
            }
        }
    }
    abi::emit_jump(emitter, accept);
}

/// Emits the `string`/`bool` checks: any scalar (and a `Stringable` for `string`) when
/// coercive, exactly the declared tag when strict.
fn emit_string_or_bool(
    emitter: &mut Emitter,
    data: &mut DataSection,
    guards: &InvokerScalarGuards,
    parameter: &ScalarParameter,
    strict: &str,
    accept: &str,
    refuse: &str,
) {
    let result = abi::int_result_reg(emitter);
    let string = parameter.declared == PhpType::Str;
    // -- coercive: every scalar binds --
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #3");                                  // tags 0..3 are int, string, float and bool
            emitter.instruction(&format!("b.ls {accept}"));                     // every scalar binds coercively
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 3");                                  // tags 0..3 are int, string, float and bool
            emitter.instruction(&format!("jbe {accept}"));                      // every scalar binds coercively
        }
    }
    if string && (guards.stringable.is_some() || guards.eval_bridge) {
        // -- coercive string: a Stringable object binds too --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #6");                              // is the argument an object?
                emitter.instruction(&format!("b.ne {refuse}"));                 // every other non-scalar is refused
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 6");                              // is the argument an object?
                emitter.instruction(&format!("jne {refuse}"));                  // every other non-scalar is refused
            }
        }
        if let Some((id, kind)) = guards.stringable {
            abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
            abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), id as i64);
            abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), kind);
            abi::emit_call_label(emitter, "__rt_mixed_instanceof");
            emit_branch_if_result_nonzero(emitter, accept);
        }
        if guards.eval_bridge {
            // -- an eval()-declared class: ask the bridge, as the direct-call guard does --
            let (target, target_len) = data.add_string(b"Stringable");
            abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 0);
            abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), 0);
            abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 2), &target);
            abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 3), target_len as i64);
            abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 4), 0);
            let symbol = emitter.target.extern_symbol("__elephc_eval_object_is_a");
            abi::emit_call_label(emitter, &symbol);
            emit_branch_if_result_nonzero(emitter, accept);
        }
    }
    abi::emit_jump(emitter, refuse);
    // -- strict: exactly the declared scalar --
    let tag = if string { 1 } else { 3 };
    emitter.label(strict);
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{tag}"));                    // strict mode accepts only the declared scalar tag
            emitter.instruction(&format!("b.eq {accept}"));                     // the exact scalar binds
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {tag}"));                    // strict mode accepts only the declared scalar tag
            emitter.instruction(&format!("je {accept}"));                       // the exact scalar binds
        }
    }
    abi::emit_jump(emitter, refuse);
}

/// Names the refused value's type and throws `TypeError` with the descriptor's callee name.
fn emit_refusal(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    guards: &InvokerScalarGuards,
    parameter: &ScalarParameter,
    refuse: &str,
) {
    let payload = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    let prefix = format!(
        "{}(): Argument #{} (${}) must be of type {}, ",
        guards.callee.as_deref().unwrap_or(""),
        parameter.index + 1,
        parameter.name,
        crate::types::diagnostic_names::php_type_name(&parameter.declared),
    );
    // A static callee name is already in the prefix; otherwise the descriptor supplies it.
    let descriptor = guards.callee.is_none().then_some(INVOKER_DESCRIPTOR_OFFSET);
    let boolean = ctx.next_label("scalar_param_bool");
    let object = ctx.next_label("scalar_param_object");
    let arms = crate::codegen::lower_inst::RUNTIME_TAGS
        .iter()
        .filter(|(tag, _)| *tag != 3 && *tag != 6)
        .map(|(tag, name)| (*tag, *name, ctx.next_label("scalar_param_tag")))
        .collect::<Vec<_>>();
    // -- refused: dispatch on the value's tag to PHP's type name --
    emitter.label(refuse);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    branch_tag(emitter, 3, &boolean);
    branch_tag(emitter, 6, &object);
    for (tag, _, label) in &arms {
        branch_tag(emitter, i64::from(*tag), label);
    }
    refuse_invoker_argument_static(emitter, data, descriptor, &prefix, "mixed");
    for (_, name, label) in &arms {
        emitter.label(label);
        refuse_invoker_argument_static(emitter, data, descriptor, &prefix, name);
    }
    // -- an object is named by its runtime class --
    emitter.label(&object);
    let fallback = data.add_string(b"object");
    let fallback_label = ctx.next_label("scalar_param_object_fallback");
    let done_label = ctx.next_label("scalar_param_object_done");
    crate::codegen::lower_inst::emit_invoker_class_name_lookup(
        emitter, payload, &fallback, &fallback_label, &done_label,
    );
    refuse_invoker_argument_loaded(emitter, data, descriptor, &prefix);
    // -- a boolean is named by its value --
    emitter.label(&boolean);
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter), payload);
    let falsy = ctx.next_label("scalar_param_false");
    abi::emit_branch_if_int_result_zero(emitter, &falsy);
    refuse_invoker_argument_static(emitter, data, descriptor, &prefix, "true");
    emitter.label(&falsy);
    refuse_invoker_argument_static(emitter, data, descriptor, &prefix, "false");
}

/// Branches to `label` when the unboxed tag in the integer result register equals `tag`.
fn branch_tag(emitter: &mut Emitter, tag: i64, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{tag}"));                    // compare the refused value's tag
            emitter.instruction(&format!("b.eq {label}"));                      // name this value category
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {tag}"));                    // compare the refused value's tag
            emitter.instruction(&format!("je {label}"));                        // name this value category
        }
    }
}

/// Branches to `accept` when the integer result register holds a non-zero match.
fn emit_branch_if_result_nonzero(emitter: &mut Emitter, accept: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz x0, {accept}"));                 // a Stringable object binds to string
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the Stringable match succeed?
            emitter.instruction(&format!("jnz {accept}"));                      // a Stringable object binds to string
        }
    }
}
