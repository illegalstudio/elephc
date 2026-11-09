//! Purpose:
//! Emits the catchable `Error`s a method call raises once the receiver is known not to be a
//! callable object of a class that declares that method: non-object receivers, `__call`, and
//! `Call to undefined method`.
//!
//! Called from:
//! - `method_dispatch::lower_method_call` for a statically non-object receiver.
//! - `method_dispatch::lower_mixed_method_call` after the native class ladder misses.
//!
//! Key details:
//! - Bool receivers name `true` or `false`. PHP does not say `on bool`.
//! - `__call` is dispatched here, at the runtime class, so the EIR method name stays the name
//!   the user wrote. A class that declares the method keeps the native ladder or an
//!   `ArgumentCountError` instead of falling through to `__call`.
//! - The args array is built to the impl class's EIR parameter type. The only callee-saved
//!   register this pass may use is the nested-call register, which already holds the receiver.

use super::*;

/// How the method name is passed to `__call`.
#[derive(Clone, Copy)]
enum NameAbi {
    /// Declared `string`: pointer and length.
    Str,
    /// Declared `mixed` (or a union stored as mixed): one owned box.
    Mixed,
}

/// How the user arguments are packed for `__call`'s second parameter.
enum ArgsAbi {
    /// The parameter is `mixed`: pass an owned box around a mixed array.
    BoxedMixed,
    /// The parameter is `array` of mixed, null, or a union: pass the array pointer.
    RawMixed,
    /// The parameter is `array<string>` and every operand is a string.
    RawStr,
    /// The parameter is `array<int>` or `array<bool>` and every operand fits that slot.
    RawInt(PhpType),
}

/// One runtime class whose missing method is forwarded to its instance `__call`.
struct MagicCandidate {
    class_id: u64,
    target: MethodCallTarget,
    name: NameAbi,
    args: ArgsAbi,
}

/// One runtime class that declares the method at a count this call does not satisfy.
struct ArityMismatch {
    class_id: u64,
    message: String,
}

/// Throws PHP's catchable member-call `Error` for a receiver whose static type is not an object.
///
/// `Int`, `Float`, `Str`, `Bool`, `Array`, `Void`, and `Callable` name the runtime type.
/// `Bool` loads the value so `false` and `true` stay distinct. Any other representation,
/// including iterables and pointers, stays a compile error: those values can still be objects.
pub(super) fn emit_static_non_object_method_error(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    method_name: &str,
    object_ty: &PhpType,
) -> Result<()> {
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
        emit_member_function_error(ctx, method_name, type_name);
    }
    ctx.emitter.label(&bool_arm);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    abi::emit_reg_move(ctx.emitter, tag_reg, payload);
    emit_bool_member_error(ctx, method_name, "mixed_method_on_false");
}

/// Forwards a missed native call to `__call`, or throws the undefined-method `Error`.
///
/// Classes that declare the method at another arity throw `ArgumentCountError` instead, so a
/// wrong count is not reported as an undefined method and is not handed to `__call`.
/// `done_label` is the mixed call's join point after a successful `__call`.
pub(super) fn emit_magic_or_undefined_method(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver_reg: &str,
    method_name: &str,
    done_label: &str,
) -> Result<()> {
    let magic = magic_candidates(ctx, inst, method_name)?;
    let arity = arity_mismatches(ctx, method_name, inst.operands.len())?;
    if magic.is_empty() && arity.is_empty() {
        emit_undefined_method_fatal(ctx, receiver_reg, method_name);
        return Ok(());
    }
    let undefined = ctx.next_label("mixed_method_undefined");
    let magic_labels = magic
        .iter()
        .map(|candidate| ctx.next_label(&format!("magic_call_{}", candidate.class_id)))
        .collect::<Vec<_>>();
    let arity_labels = arity
        .iter()
        .map(|mismatch| ctx.next_label(&format!("method_arity_{}", mismatch.class_id)))
        .collect::<Vec<_>>();
    let magic_pairs = magic
        .iter()
        .zip(magic_labels.iter())
        .map(|(candidate, label)| (candidate.class_id, label.clone()))
        .collect::<Vec<_>>();
    let arity_pairs = arity
        .iter()
        .zip(arity_labels.iter())
        .map(|(mismatch, label)| (mismatch.class_id, label.clone()))
        .collect::<Vec<_>>();
    let after_magic = if arity_pairs.is_empty() {
        undefined.clone()
    } else {
        ctx.next_label("mixed_method_arity")
    };
    if !magic_pairs.is_empty() {
        emit_class_id_ladder(ctx, receiver_reg, &magic_pairs, &after_magic);
    }
    if !arity_pairs.is_empty() {
        if !magic_pairs.is_empty() {
            ctx.emitter.label(&after_magic);
        }
        emit_class_id_ladder(ctx, receiver_reg, &arity_pairs, &undefined);
    }
    ctx.emitter.label(&undefined);
    emit_undefined_method_fatal(ctx, receiver_reg, method_name);
    for (candidate, label) in magic.iter().zip(magic_labels.iter()) {
        ctx.emitter.label(label);
        emit_magic_call(ctx, inst, receiver_reg, method_name, candidate, done_label)?;
    }
    for (mismatch, label) in arity.iter().zip(arity_labels.iter()) {
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
        PhpType::Callable => "Closure",
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
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&text);                                     // compare the receiver class id with this candidate
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&text);                                     // compare the receiver class id with this candidate
        }
    }
}

/// Classes that do not declare `method_name` and whose `__call` can accept this call.
fn magic_candidates(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    method_name: &str,
) -> Result<Vec<MagicCandidate>> {
    let method_key = php_symbol_key(method_name);
    let magic_key = php_symbol_key("__call");
    let user_operands = &inst.operands[1..];
    let mut candidates = Vec::new();
    for (class_name, info) in &ctx.module.class_infos {
        if info.methods.contains_key(&method_key) || !info.methods.contains_key(&magic_key) {
            continue;
        }
        let Ok(target) = resolve_method_call_target(ctx, class_name, "__call", 3) else {
            continue;
        };
        // Ordinary methods set `source_abi` when the physical parameter list is already the
        // caller-visible one, so that flag is not an adapter. A hidden `func_args` collector
        // is refused here: this packing does not synthesize the extra slot.
        let Some(magic_sig) = info.methods.get(&magic_key) else {
            continue;
        };
        if crate::codegen_support::source_method_adapters::uses_physical_func_args_abi(magic_sig)
            || target.params.len() != 2
            || target.ref_params.iter().any(|flag| *flag)
        {
            continue;
        }
        let Some(user_params) = eir_call_user_params(ctx, &target.impl_class) else {
            continue;
        };
        if target.params[0] != user_params[0].php_type.codegen_repr()
            || target.params[1] != user_params[1].php_type.codegen_repr()
        {
            continue;
        }
        let Some(name) = name_abi_for(&user_params[0].php_type) else {
            continue;
        };
        let Some(args) = args_abi_for(&user_params[1].php_type) else {
            continue;
        };
        if !operands_fit(ctx, &args, user_operands)? {
            continue;
        }
        candidates.push(MagicCandidate {
            class_id: info.class_id,
            target,
            name,
            args,
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
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

/// PHP's too-few or too-many sentence, without a source location in the middle.
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
    if signature.variadic.is_none() && provided > regular {
        return Some(format!(
            "Too many arguments to function {class_name}::{method_name}(), {provided} passed and exactly {regular} expected"
        ));
    }
    None
}

/// The two user parameters of `{impl_class}::__call`, after the instance receiver.
///
/// By-ref or variadic parameters are refused: the packed call below passes values.
fn eir_call_user_params(ctx: &FunctionContext<'_>, impl_class: &str) -> Option<Vec<FunctionParam>> {
    let function = ctx.module.class_methods.iter().find(|function| {
        !function.flags.is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(owner, name)| owner == impl_class && php_symbol_key(name) == "__call")
    })?;
    if function.params.len() != 3 {
        return None;
    }
    let user = &function.params[1..];
    if user.iter().any(|param| param.by_ref || param.variadic) {
        return None;
    }
    Some(user.to_vec())
}

/// The outgoing name ABI, or `None` when the parameter is not a string or a mixed cell.
fn name_abi_for(ty: &PhpType) -> Option<NameAbi> {
    match ty.codegen_repr() {
        PhpType::Str => Some(NameAbi::Str),
        PhpType::Mixed | PhpType::Union(_) => Some(NameAbi::Mixed),
        _ => None,
    }
}

/// The outgoing args ABI, or `None` when the packed layout would not match the body.
fn args_abi_for(ty: &PhpType) -> Option<ArgsAbi> {
    match ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => Some(ArgsAbi::BoxedMixed),
        PhpType::Array(element) => match element.codegen_repr() {
            PhpType::Mixed | PhpType::Void | PhpType::Never | PhpType::Union(_) => {
                Some(ArgsAbi::RawMixed)
            }
            PhpType::Str => Some(ArgsAbi::RawStr),
            PhpType::Int => Some(ArgsAbi::RawInt(PhpType::Int)),
            PhpType::Bool => Some(ArgsAbi::RawInt(PhpType::Bool)),
            _ => None,
        },
        _ => None,
    }
}

/// Reports whether every user operand can be stored in `args` without changing its shape.
fn operands_fit(ctx: &FunctionContext<'_>, args: &ArgsAbi, operands: &[ValueId]) -> Result<bool> {
    match args {
        ArgsAbi::BoxedMixed | ArgsAbi::RawMixed => Ok(true),
        ArgsAbi::RawStr => {
            for value in operands {
                if ctx.value_php_type(*value)? != PhpType::Str {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        ArgsAbi::RawInt(_) => {
            for value in operands {
                if !matches!(
                    ctx.value_php_type(*value)?.codegen_repr(),
                    PhpType::Int | PhpType::Bool | PhpType::False
                ) {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

/// Builds the `__call` arguments, calls the resolved body, and joins `done_label`.
fn emit_magic_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    receiver_reg: &str,
    method_name: &str,
    candidate: &MagicCandidate,
    done_label: &str,
) -> Result<()> {
    build_call_array(ctx, &inst.operands[1..], &candidate.args)?;
    spill_magic_frame(ctx, method_name, candidate.name, &candidate.args);
    place_magic_arguments(ctx, receiver_reg, candidate.name);
    emit_resolved_method_call(ctx, &candidate.target)?;
    store_method_call_result(ctx, inst, &candidate.target)?;
    release_magic_frame(ctx, candidate.name, &candidate.args);
    abi::emit_jump(ctx.emitter, done_label);
    Ok(())
}

/// Allocates the args array and leaves its pointer in one 16-byte stack slot.
fn build_call_array(
    ctx: &mut FunctionContext<'_>,
    operands: &[ValueId],
    args: &ArgsAbi,
) -> Result<()> {
    let (elem_size, elem_ty) = match args {
        ArgsAbi::BoxedMixed | ArgsAbi::RawMixed => (8, PhpType::Mixed),
        ArgsAbi::RawStr => (16, PhpType::Str),
        ArgsAbi::RawInt(ty) => (8, ty.clone()),
    };
    emit_fresh_array(ctx, operands.len().max(1), elem_size, &elem_ty);
    for value in operands {
        match args {
            ArgsAbi::BoxedMixed | ArgsAbi::RawMixed => push_boxed_element(ctx, *value)?,
            ArgsAbi::RawStr => push_string_element(ctx, *value)?,
            ArgsAbi::RawInt(_) => push_int_element(ctx, *value)?,
        }
    }
    if matches!(args, ArgsAbi::RawInt(PhpType::Bool)) {
        let result = abi::int_result_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, 0);
        crate::codegen::emit_array_value_type_stamp(ctx.emitter, result, &PhpType::Bool);
    }
    Ok(())
}

/// Allocates an indexed array and pushes its pointer.
fn emit_fresh_array(ctx: &mut FunctionContext<'_>, capacity: usize, elem_size: i64, elem_ty: &PhpType) {
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_int_immediate(ctx.emitter, arg0, capacity as i64);
    abi::emit_load_int_immediate(ctx.emitter, arg1, elem_size);
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    let result = abi::int_result_reg(ctx.emitter);
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, result, elem_ty);
    abi::emit_push_reg(ctx.emitter, result);
}

/// Boxes one user operand and appends that cell to the array saved on the stack.
fn push_boxed_element(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    super::arrays::box_value_for_mixed_container(ctx, value, &value_ty)?;
    let result = abi::int_result_reg(ctx.emitter);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    // The box currently occupies the result register, which is also argument 0 on AArch64.
    abi::emit_reg_move(ctx.emitter, arg1, result);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    abi::emit_store_to_sp(ctx.emitter, arg1, 0);
    abi::emit_reg_move(ctx.emitter, arg0, scratch);
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    crate::codegen::emit_release_pushed_refcounted_temp_after_array_push(
        ctx.emitter,
        &PhpType::Mixed,
    );
    abi::emit_push_reg(ctx.emitter, result);
    Ok(())
}

/// Appends one string operand to the array saved on the stack.
fn push_string_element(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(value, ptr, len)?;
    let staged = super::ownership::stage_string_byte_view(ctx, value, ptr, len);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_reg_move(ctx.emitter, arg1, ptr);
    abi::emit_reg_move(ctx.emitter, arg2, len);
    if staged {
        abi::emit_load_temporary_stack_slot(ctx.emitter, arg0, 16);
    } else {
        abi::emit_pop_reg(ctx.emitter, arg0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
    if staged {
        super::ownership::release_string_byte_stage(ctx, true);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Appends one integer or bool operand to the array saved on the stack.
fn push_int_element(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    let result = abi::int_result_reg(ctx.emitter);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_reg_move(ctx.emitter, arg1, result);
    abi::emit_pop_reg(ctx.emitter, arg0);
    abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
    abi::emit_push_reg(ctx.emitter, result);
    Ok(())
}

/// Replaces the 16-byte array slot with a 32-byte name-and-payload spill.
///
/// Word 0 is the name pointer or its mixed box, word 1 is the name length or unused,
/// and word 2 is the args payload. A mixed args parameter is boxed first, while the
/// array is the only stack slot, so the boxer cannot overwrite it.
fn spill_magic_frame(
    ctx: &mut FunctionContext<'_>,
    method_name: &str,
    name: NameAbi,
    args: &ArgsAbi,
) {
    if matches!(args, ArgsAbi::BoxedMixed) {
        let result = abi::int_result_reg(ctx.emitter);
        abi::emit_pop_reg(ctx.emitter, result);
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Array(Box::new(PhpType::Mixed)),
        );
        abi::emit_push_reg(ctx.emitter, result);
    }
    let (name_label, name_len) = ctx.data.add_string(method_name.as_bytes());
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr, &name_label);
    abi::emit_load_int_immediate(ctx.emitter, len, name_len as i64);
    if matches!(name, NameAbi::Mixed) {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    }
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    abi::emit_store_to_sp(ctx.emitter, scratch, 16);
    if matches!(name, NameAbi::Mixed) {
        abi::emit_store_to_sp(ctx.emitter, result, 0);
    } else {
        abi::emit_store_to_sp(ctx.emitter, ptr, 0);
        abi::emit_store_to_sp(ctx.emitter, len, 8);
    }
}

/// Moves the spilled name and payload, then the saved receiver, into the argument registers.
fn place_magic_arguments(ctx: &mut FunctionContext<'_>, receiver_reg: &str, name: NameAbi) {
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    let mut index = 1;
    let mut loads = Vec::new();
    match name {
        NameAbi::Str => {
            loads.push((index, 0));
            index += 1;
            loads.push((index, 8));
            index += 1;
        }
        NameAbi::Mixed => {
            loads.push((index, 0));
            index += 1;
        }
    }
    loads.push((index, 16));
    for (index, offset) in loads {
        let arg = abi::int_arg_reg_name(ctx.emitter.target, index);
        abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, offset);
        abi::emit_reg_move(ctx.emitter, arg, scratch);
    }
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    abi::emit_reg_move(ctx.emitter, arg0, receiver_reg);
}

/// Drops the caller's remaining references to the name box and the args payload.
fn release_magic_frame(ctx: &mut FunctionContext<'_>, name: NameAbi, args: &ArgsAbi) {
    if matches!(name, NameAbi::Mixed) {
        decref_spill(ctx, 0, &PhpType::Mixed);
    }
    decref_spill(ctx, 16, &payload_php_type(args));
    abi::emit_release_temporary_stack(ctx.emitter, 32);
}

/// Loads one spilled word into the result register and releases it when it is refcounted.
fn decref_spill(ctx: &mut FunctionContext<'_>, offset: usize, ty: &PhpType) {
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, offset);
    abi::emit_decref_if_refcounted(ctx.emitter, ty);
}

/// The PHP type of the word passed as `__call`'s args parameter.
fn payload_php_type(args: &ArgsAbi) -> PhpType {
    match args {
        ArgsAbi::BoxedMixed => PhpType::Mixed,
        ArgsAbi::RawMixed => PhpType::Array(Box::new(PhpType::Mixed)),
        ArgsAbi::RawStr => PhpType::Array(Box::new(PhpType::Str)),
        ArgsAbi::RawInt(ty) => PhpType::Array(Box::new(ty.clone())),
    }
}
