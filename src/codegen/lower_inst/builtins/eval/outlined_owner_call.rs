//! Purpose:
//! Emits the interpreter-declared slow path of an owned-receiver method call ONCE per module per
//! key and calls it from every site, instead of re-emitting the scope push, the argument pack,
//! the bridge call, the result conversion and the releases at each non-final method call.
//!
//! Called from:
//! - `lower_eval_owned_method_call_with_result()` in `dynamic_calls`, after the inline
//!   `__rt_eval_object_has_owner` probe said the receiver belongs to an interpreter-declared class.
//! - `lower_method_call()` in `method_dispatch`, which routes the helper's own instruction to
//!   [`lower_owner_call_helper_body`].
//! - `crate::codegen::lower_inst::outlined_method_call::emit_mixed_method_call_helper()` writes the
//!   helper bodies, through the shared dynamic-property helper queue.
//!
//! Key details:
//! - The helper is an ordinary synthetic EIR function `(receiver, arg0..argN) -> result` holding the
//!   ONE `MethodCall` instruction. Inside it `lower_method_call` emits only the slow path, so the
//!   existing boxing, scope publication, status check (Throwables unwind through the helper),
//!   result conversion and releases run unchanged.
//! - Key: the method name as written, the lexical class the caller-class push publishes (carried as
//!   the helper's `lexical_class`), each operand's IR and PHP type, and the result's types and
//!   ownership. The collector flags are a function of the receiver's static type, the method name and
//!   the operand count, so they are implied by the key.
//! - A negative bridge answer (the interpreter class does not declare the method) is a miss: the
//!   helper raises `_eval_owner_miss`, returns a zero value, and the site falls back to native
//!   dispatch. Every other return clears the flag first.
//! - Stays inline: a site that forwards an eval context scope or carries a runtime closure scope
//!   (the helper has neither), an argument that would travel as a reference marker unless the
//!   receiver's declared signature proves that parameter by value (the helper only sees copies),
//!   results the register convention cannot return (tagged scalars), and more than eight arguments.
//! - `ELEPHC_NO_OUTLINED_EVAL_OWNER=1` keeps the inline slow path, for size and behaviour comparisons.

use super::*;
use crate::codegen::shared_state::{DynamicPropHelperKind, MethodCallHelperTemplate};
use crate::ir::Ownership;

/// Readable part of the helper's entry symbol.
const OWNER_CALL_PREFIX: &str = "eval_owner_call";

/// Most operands (receiver included) an outlined site passes.
const MAX_OPERANDS: usize = 9;

/// Symbol holding the miss flag the helper raises.
const MISS_FLAG_SYMBOL: &str = "_eval_owner_miss";

/// Symbols through which a site hands the helper its lexical class.
const SCOPE_PTR_SYMBOL: &str = "_eval_owner_scope_ptr";
const SCOPE_LEN_SYMBOL: &str = "_eval_owner_scope_len";

fn outlining_disabled() -> bool {
    std::env::var_os("ELEPHC_NO_OUTLINED_EVAL_OWNER").is_some()
}

/// Returns whether the body being lowered IS a shared owner-call helper.
pub(in crate::codegen::lower_inst) fn inside_owner_call_helper(ctx: &FunctionContext<'_>) -> bool {
    ctx.function.flags.is_synthetic
        && ctx
            .function
            .name
            .contains(&format!("_{}_", OWNER_CALL_PREFIX))
}

/// Publishes the lexical class the site stored, for one bridge call; the helper twin of
/// `push_native_caller_class`. Returns whether anything was pushed.
pub(super) fn push_site_caller_class(ctx: &mut FunctionContext<'_>) -> bool {
    let publishes = ctx
        .shared
        .owner_call_site_facts
        .get(&ctx.function.name)
        .is_some_and(|(_, publishes)| *publishes);
    if !publishes {
        return false;
    }
    let target = ctx.emitter.target;
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        SCOPE_PTR_SYMBOL,
        0,
    );
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_arg_reg_name(target, 1),
        SCOPE_LEN_SYMBOL,
        0,
    );
    let symbol = target.extern_symbol("__elephc_eval_push_native_caller_class");
    abi::emit_call_label(ctx.emitter, &symbol);
    true
}

/// Stores `value` into the miss flag. Clobbers only scratch registers.
pub(super) fn emit_set_owner_miss_flag(ctx: &mut FunctionContext<'_>, value: i64) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_int_immediate(ctx.emitter, scratch, value);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, MISS_FLAG_SYMBOL, 0);
}

/// Branches to `label` when the helper raised the miss flag. Clobbers only scratch registers.
fn emit_branch_if_owner_miss(ctx: &mut FunctionContext<'_>, label: &str) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, MISS_FLAG_SYMBOL, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz {scratch}, {label}")); // the interpreter class declined the call
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {scratch}, {scratch}")); // did the interpreter class decline the call?
            ctx.emitter.instruction(&format!("jnz {label}")); // fall back to native dispatch
        }
    }
}

/// Zeroes the registers the helper returns its (ignored) miss value in.
fn emit_zero_result(ctx: &mut FunctionContext<'_>, repr: &PhpType) {
    match repr {
        PhpType::Void | PhpType::Never => {}
        PhpType::Float => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("fmov d0, xzr"); // a miss returns no value
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("xorps xmm0, xmm0"); // a miss returns no value
            }
        },
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
        }
        _ => {
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
        }
    }
}

/// Whether the helper's register convention can return a value of this representation.
fn result_repr_supported(repr: &PhpType) -> bool {
    matches!(
        repr,
        PhpType::Void
            | PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Str
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Iterable
            | PhpType::Object(_)
            | PhpType::Callable
    )
}

/// The type with a top-level class name erased, for keying helpers on representation alone.
fn erase_class(ty: &PhpType) -> PhpType {
    match ty {
        PhpType::Object(_) => PhpType::Object(String::new()),
        other => other.clone(),
    }
}

/// The compiled signature the receiver's static type declares for `method_name`, if any.
fn receiver_signature<'a>(
    ctx: &'a FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
) -> Option<&'a FunctionSig> {
    let PhpType::Object(class_name) = ctx.value_php_type(object).ok()?.codegen_repr() else {
        return None;
    };
    let class_name = class_name.trim_start_matches('\\').to_string();
    let key = php_symbol_key(method_name);
    ctx.module
        .class_infos
        .get(&class_name)
        .and_then(|info| info.methods.get(&key))
        .or_else(|| {
            ctx.module
                .interface_infos
                .get(&class_name)
                .and_then(|info| info.methods.get(&key))
        })
}

/// Returns whether every argument the inline path would pass as a reference marker is proven to be
/// a by-value parameter by the receiver's declared signature.
fn arguments_survive_by_value(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
) -> Result<bool> {
    let mut signature: Option<Option<&FunctionSig>> = None;
    for (index, operand) in inst.operands.iter().enumerate().skip(1) {
        let Ok(slot) = crate::codegen::lower_inst::reference_arguments::local_slot_for_loaded_value(
            ctx, *operand,
        ) else {
            continue;
        };
        if !eval_method_argument_has_stable_ref_place(ctx, slot)
            || matches!(ctx.local_php_type(slot)?.codegen_repr(), PhpType::TaggedScalar)
        {
            continue;
        }
        let signature = *signature.get_or_insert_with(|| receiver_signature(ctx, object, method_name));
        let by_value = signature.is_some_and(|signature| {
            index - 1 < signature.params.len()
                && !signature.ref_params.get(index - 1).copied().unwrap_or(false)
        });
        if !by_value {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Lowers the interpreter-declared slow path of an owned-receiver method call through the shared
/// helper. Returns `false` without emitting anything when this site must keep the inline path.
#[allow(clippy::too_many_arguments)]
pub(super) fn try_lower_outlined_owner_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    method_name: &str,
    miss_label: &str,
    done_label: &str,
    result_type: &PhpType,
    store_result: bool,
) -> Result<bool> {
    if outlining_disabled()
        || !store_result
        || inst.op != Op::MethodCall
        || inside_owner_call_helper(ctx)
        || inst.operands.len() > MAX_OPERANDS
        || *result_type != inst.result_php_type
        || !matches!(
            inst.immediate,
            Some(Immediate::Data(_)) | Some(Immediate::ProfiledData { .. })
        )
        || !result_repr_supported(&inst.result_php_type.codegen_repr())
        || inst.result.is_some() == inst.result_type.is_void()
        || !matches!(
            inst.result_ownership,
            Ownership::Owned | Ownership::NonHeap | Ownership::MaybeOwned
        )
        || has_runtime_closure_scope(ctx)
    {
        return Ok(false);
    }
    let lexical_class = current_eval_method_class(ctx).map(str::to_string);
    if lexical_class.is_some() && has_eval_context(ctx) {
        return Ok(false);
    }
    if !arguments_survive_by_value(ctx, inst, object, method_name)? {
        return Ok(false);
    }
    let collector_flags = eval_method_call_collector_flags(ctx, inst, object, method_name)?;
    let mut operands = Vec::with_capacity(inst.operands.len());
    let mut key = format!(
        "{}:{}:{}:{}:{:?}/{:?}/{:?}",
        OWNER_CALL_PREFIX,
        method_name,
        lexical_class.is_some(),
        collector_flags,
        inst.result_type,
        erase_class(&inst.result_php_type),
        inst.result_ownership
    );
    for operand in &inst.operands {
        let php_type = ctx.raw_value_php_type(*operand)?;
        if matches!(php_type.codegen_repr(), PhpType::Void) {
            return Ok(false);
        }
        let ir_type = ctx.value_ir_type(*operand)?;
        // Boxing an object does not look at its class, so receivers of different classes share
        // one helper; the helper is typed after the first site that asked for it.
        key.push_str(&format!(":{:?}/{:?}", ir_type, erase_class(&php_type)));
        operands.push((ir_type, php_type));
    }
    let label = ctx.keyed_global_label(&key, OWNER_CALL_PREFIX);
    ctx.shared.request_dynamic_prop_helper(
        &key,
        &label,
        DynamicPropHelperKind::MixedMethodCall,
        None,
    );
    ctx.shared
        .owner_call_site_facts
        .insert(label.clone(), (collector_flags, lexical_class.is_some()));
    ctx.shared
        .method_call_templates
        .entry(key)
        .or_insert_with(|| MethodCallHelperTemplate {
            op: inst.op,
            immediate: inst.immediate.clone(),
            operands: operands.clone(),
            result_type: inst.result_type.clone(),
            result_php_type: inst.result_php_type.clone(),
            result_ownership: inst.result_ownership,
        });

    let param_types = operands
        .iter()
        .map(|(_, php_type)| php_type.codegen_repr())
        .collect::<Vec<_>>();
    let ref_params = vec![false; param_types.len()];
    let call_args = crate::codegen::lower_inst::materialize_direct_call_args_with_refs_and_options(
        ctx,
        &inst.operands,
        &param_types,
        &ref_params,
        true,
        crate::codegen::lower_inst::RefArgCellLifetime::CallOnly,
    )?;
    let caller_stack_pad_bytes =
        crate::codegen::lower_inst::direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    if let Some(class_name) = &lexical_class {
        let (name_label, name_len) = ctx.data.add_string(class_name.as_bytes());
        let scratch = abi::temp_int_reg(ctx.emitter.target);
        abi::emit_symbol_address(ctx.emitter, scratch, &name_label);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, SCOPE_PTR_SYMBOL, 0);
        abi::emit_load_int_immediate(ctx.emitter, scratch, name_len as i64);
        abi::emit_store_reg_to_symbol(ctx.emitter, scratch, SCOPE_LEN_SYMBOL, 0);
    }
    abi::emit_call_label(ctx.emitter, &label);
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    let declined_label = ctx.next_label("eval_owner_call_declined");
    emit_branch_if_owner_miss(ctx, &declined_label);
    store_if_result(ctx, inst)?;
    crate::codegen::lower_inst::emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    abi::emit_jump(ctx.emitter, done_label);

    ctx.emitter.label(&declined_label);
    crate::codegen::lower_inst::emit_call_arg_temp_cleanups(ctx, &call_args, None)?;
    abi::emit_jump(ctx.emitter, miss_label);
    Ok(true)
}

/// Lowers the shared helper's one instruction: the slow path, with the miss protocol.
pub(in crate::codegen::lower_inst) fn lower_owner_call_helper_body(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let method_name = crate::codegen::lower_inst::method_name_data(ctx, inst)?.to_string();
    let miss_label = ctx.next_label("eval_owner_helper_miss");
    let done_label = ctx.next_label("eval_owner_helper_done");
    let result_type = inst.result_php_type.clone();
    emit_owned_method_slow_path(
        ctx,
        inst,
        object,
        &method_name,
        &miss_label,
        &done_label,
        &result_type,
        true,
        true,
    )?;
    ctx.emitter.label(&miss_label);
    emit_zero_result(ctx, &result_type.codegen_repr());
    store_if_result(ctx, inst)?;
    emit_set_owner_miss_flag(ctx, 1);
    ctx.emitter.label(&done_label);
    Ok(())
}
