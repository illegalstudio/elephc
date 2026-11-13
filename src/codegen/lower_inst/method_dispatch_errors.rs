//! Purpose:
//! Emits the catchable `Error`s a method call raises once the receiver is known not to be a
//! callable object of a class that declares that method: non-object receivers and
//! `Call to undefined method`.
//!
//! Called from:
//! - `method_dispatch::lower_method_call` for a statically non-object receiver.
//! - `method_dispatch::lower_mixed_method_call` after the native class ladder misses.
//!
//! Key details:
//! - Bool receivers name `true` or `false`. PHP does not say `on bool`.
//! - EIR selects magic hooks, binds their arguments and owns exceptional cleanup.
//! - This emitter only diagnoses native dispatch misses and receiver types.

use super::*;

/// One runtime class that declares the method at a count this call does not satisfy.
struct ArityMismatch {
    class_id: u64,
    message: String,
}

/// Throws PHP's catchable member-call `Error` for a receiver whose static type is not an object.
///
/// `Int`, `Float`, `Str`, `Bool`, `Array` and `Void` name the runtime type.
/// `Bool` loads the value so `false` and `true` stay distinct. Any other representation,
/// including iterables and pointers, stays a compile error: those values can still be objects.
pub(super) fn emit_static_non_object_method_error(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
    object_ty: &PhpType,
) -> Result<()> {
    if *object_ty == PhpType::Callable {
        super::exceptions::emit_error(ctx, &format!("Call to undefined method Closure::{method_name}()"));
        return Ok(());
    }
    match static_member_type_name(object_ty)? {
        Some(type_name) => emit_member_function_error(ctx, method_name, type_name),
        None => {
            ctx.load_value_to_result(object)?;
            emit_bool_member_error(ctx, method_name, "method_on_false");
        }
    }
    Ok(())
}

/// Throws the member-call `Error` for a boxed receiver whose runtime tag is not an object.
///
/// The unboxed tag is still in the integer result and the payload is still in the mixed
/// payload register. Tag 8 stays `on null`. A bool payload selects `on true` or `on false`.
pub(super) fn emit_mixed_non_object_method_fatal(
    ctx: &mut FunctionContext<'_>,
    method_name: &str,
) {
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let bool_arm = ctx.next_label("mixed_method_bool");
    let mut arms = Vec::new();
    for (tag, type_name) in super::objects::RUNTIME_TAGS
        .iter()
        .filter(|(tag, _)| *tag != super::objects::BOOL_TAG)
    {
        arms.push((
            *tag,
            (*type_name).to_string(),
            ctx.next_label(&format!("mixed_method_on_{type_name}")),
        ));
    }
    for (tag, _, label) in &arms {
        super::enums::emit_mixed_tag_branch(ctx, tag_reg, i64::from(*tag), label);
    }
    super::enums::emit_mixed_tag_branch(
        ctx,
        tag_reg,
        i64::from(super::objects::BOOL_TAG),
        &bool_arm,
    );
    // A tag outside the table is not produced by the unbox helper. Name it as null.
    emit_member_function_error(ctx, method_name, "null");
    for (_, type_name, label) in &arms {
        ctx.emitter.label(label);
        if type_name == "Closure" {
            super::exceptions::emit_error(ctx, &format!("Call to undefined method Closure::{method_name}()"));
        } else {
            emit_member_function_error(ctx, method_name, type_name);
        }
    }
    ctx.emitter.label(&bool_arm);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_reg_move(ctx.emitter, tag_reg, payload);
    emit_bool_member_error(ctx, method_name, "mixed_method_on_false");
}

/// Throws an arity error for a declared native method, or names the missing runtime method.
///
/// Magic hooks are selected and their arguments are rooted in EIR before reaching this path.
pub(super) fn emit_undefined_or_arity_error(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver_reg: &str,
    method_name: &str,
) -> Result<()> {
    let arity = arity_mismatches(ctx, method_name, inst.operands.len())?;
    let undefined = ctx.next_label("mixed_method_undefined");
    let pairs = arity.iter().map(|mismatch| (
        mismatch.class_id, ctx.next_label(&format!("method_arity_{}", mismatch.class_id)),
    )).collect::<Vec<_>>();
    emit_class_id_ladder(ctx, receiver_reg, &pairs, &undefined);
    ctx.emitter.label(&undefined);
    emit_undefined_method_fatal(ctx, receiver_reg, method_name);
    for (mismatch, (_, label)) in arity.iter().zip(&pairs) {
        ctx.emitter.label(label);
        super::exceptions::emit_argument_count_error(ctx, &mismatch.message, None);
    }
    Ok(())
}

/// The PHP type name for a static non-object receiver, or `None` when the bool must be loaded.
fn static_member_type_name(ty: &PhpType) -> Result<Option<&'static str>> {
    let name = match ty {
        PhpType::Bool => return Ok(None),
        PhpType::False => "false",
        PhpType::Int => "int",
        PhpType::Float => "float",
        PhpType::Str => "string",
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
        PhpType::Void | PhpType::Never => "null",
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "method call receiver for PHP type {:?}",
                other
            )));
        }
    };
    Ok(Some(name))
}

/// Throws `Call to a member function {method}() on {type_name}`.
fn emit_member_function_error(ctx: &mut FunctionContext<'_>, method_name: &str, type_name: &str) {
    super::exceptions::emit_error(
        ctx,
        &format!("Call to a member function {method_name}() on {type_name}"),
    );
}

/// Splits the integer result into `on true` and `on false`.
///
/// Zero is `false`. The false arm is reached only by that branch.
fn emit_bool_member_error(ctx: &mut FunctionContext<'_>, method_name: &str, false_prefix: &str) {
    let false_label = ctx.next_label(false_prefix);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &false_label);
    emit_member_function_error(ctx, method_name, "true");
    ctx.emitter.label(&false_label);
    emit_member_function_error(ctx, method_name, "false");
}

/// Throws `Call to undefined method Class::method()` using the receiver's runtime class.
fn emit_undefined_method_fatal(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    method_name: &str,
) {
    super::runtime_class_messages::emit_runtime_class_name_to_string_result(
        ctx,
        receiver_reg,
        "object",
    );
    super::runtime_class_messages::emit_concat_static_prefix(ctx, "Call to undefined method ");
    super::runtime_class_messages::emit_concat_static_suffix(
        ctx,
        &format!("::{method_name}()"),
    );
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::exceptions::emit_error_from_string_result(ctx);
}

/// Compares the receiver's class id with each pair and jumps to `fallthrough` otherwise.
fn emit_class_id_ladder(
    ctx: &mut FunctionContext<'_>,
    receiver_reg: &str,
    pairs: &[(u64, String)],
    fallthrough: &str,
) {
    if pairs.is_empty() {
        abi::emit_jump(ctx.emitter, fallthrough);
        return;
    }
    let class_id_reg = abi::symbol_scratch_reg(ctx.emitter);
    let compare_reg = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_from_address(ctx.emitter, class_id_reg, receiver_reg, 0);
    for (class_id, label) in pairs {
        abi::emit_load_int_immediate(ctx.emitter, compare_reg, *class_id as i64);
        emit_cmp_regs(ctx, class_id_reg, compare_reg);
        abi::emit_branch_if_equal_wide(ctx.emitter, label);
    }
    abi::emit_jump(ctx.emitter, fallthrough);
}

/// Compares two integer registers. The following branch reads the condition.
fn emit_cmp_regs(ctx: &mut FunctionContext<'_>, left: &str, right: &str) {
    let text = format!("cmp {left}, {right}");
    ctx.emitter.instruction(&text);                                             // compare the receiver class id with this candidate
}

/// Classes that declare `method_name` but not at this operand count.
///
/// A count that is in range for a defaulted or variadic signature is omitted: the native
/// ladder only emits an exact arity, and reporting that call as undefined would be worse
/// than leaving it for the final undefined-method error.
fn arity_mismatches(
    ctx: &FunctionContext<'_>,
    method_name: &str,
    operand_count: usize,
) -> Result<Vec<ArityMismatch>> {
    let method_key = php_symbol_key(method_name);
    let mut mismatches = Vec::new();
    for (class_name, info) in &ctx.module.class_infos {
        let Some(signature) = info.methods.get(&method_key) else {
            continue;
        };
        let source_count =
            crate::codegen_support::source_method_adapters::source_visible_signature(signature)
                .ok()
                .map(|source| source.params.len() + 1);
        if signature.params.len() + 1 == operand_count || source_count == Some(operand_count) {
            continue;
        }
        let Some(message) = arity_message(class_name, method_name, signature, operand_count) else {
            continue;
        };
        mismatches.push(ArityMismatch {
            class_id: info.class_id,
            message,
        });
    }
    mismatches.sort_by_key(|mismatch| mismatch.class_id);
    Ok(mismatches)
}

/// PHP's too-few sentence, without a source location in the middle.
fn arity_message(
    class_name: &str,
    method_name: &str,
    signature: &FunctionSig,
    operand_count: usize,
) -> Option<String> {
    let provided = operand_count.saturating_sub(1);
    let regular = crate::types::call_args::regular_param_count(signature);
    let required = crate::types::call_args::required_param_count(signature);
    if provided < required {
        let bound = if required < regular { "at least" } else { "exactly" };
        return Some(format!(
            "Too few arguments to function {class_name}::{method_name}(), {provided} passed and {bound} {required} expected"
        ));
    }
    None
}
