//! Purpose:
//! Emits the weak-mode typed-property guard ONCE per module per declared type and calls it from
//! every declared-slot store, instead of re-emitting the tag dispatch, the coercion diagnostics
//! and the refusals at each site.
//!
//! Called from:
//! - `emit_mixed_property_type_guard()` in the parent module, at every site that stores a boxed
//!   `Mixed`/`Union` value into a declared property.
//! - `crate::codegen::block_emit` through `emit_pending_property_guard_helpers()`, which writes
//!   the helper bodies right behind the function that first asked for them.
//!
//! Key details:
//! - Measured on the compiled Symfony console, the inline guard was 31% of the binary: 86 M of
//!   275 M instructions under `*_typed_prop_*` labels, 62 M of them the OBJECT arm alone -- the
//!   accepted-class id ladder -- repeated at 322 392 sites while the number of distinct declared
//!   types is a few thousand. Everything in a guard except two strings depends on the declared
//!   type alone, so the type is the helper's key.
//! - Helper key: the declared `PhpType`. The class and property names exist only in `TypeError`
//!   text, and that text is a RUNTIME argument (below), so one helper serves every property of
//!   the type. The class-id ladders are a function of the module, which is constant, and the
//!   `eval` bridge flag is a module fact too, so neither belongs in the key.
//! - Calling convention. In: the boxed `Mixed` pointer in the first integer argument register and
//!   a pointer to a refusal descriptor in the second. The descriptor is a two-word data record,
//!   `[pointer, length]` of the text that follows the source type in php's message --
//!   ` to property C::$p of type T` -- so the helper composes `Cannot assign <source>` + that.
//!   Out: the integer result register holds `0` when the value is accepted as it stands (the
//!   ordinary conversion path then applies) and `1` when the site must produce a replacement
//!   value itself. A guard whose declared type never coerces gets no result and no test at all.
//!   Every other outcome throws; the helper never returns after a refusal.
//! - What stays at the site is exactly the part that needs the site: the arms that BUILD a value
//!   (union member coercion, `__toString`). They store the replacement in the slot's own
//!   representation and release the site's source box according to the site's ownership of it,
//!   neither of which a shared body can know. The helper answers `1` for such an arm before doing
//!   anything to the value, so the site runs the whole arm, diagnostics included, exactly once.
//! - The helper runs BEFORE the destination is touched and only reads the box, so a refused
//!   assignment still leaves the previous slot contents owned and intact. Deprecations are
//!   written from inside the helper, before the store, in the order the inline guard wrote them.
//!   The site sets nothing the helper overwrites: the creation line of the throwable is resolved
//!   by the unwinder from the same runtime state as before, and the extra frame between the
//!   raise and the `catch` is the shape `shared_static_throw` already pins as unwindable.
//! - The descriptor lives in the reserved nested-call register for the whole helper body. Several
//!   arms push and pop scratch frames of their own, so a stack slot would move under the refusal
//!   path; the register does not, and `shared_helper`'s frame has already preserved it for the
//!   caller.
//! - Helper bodies are emitted behind the function that requested them, never inside it, so the
//!   requesting function stays one contiguous region and a parallel codegen worker can bracket
//!   the helper with the keyed-helper markers the merge already deduplicates on.

use super::*;
use crate::codegen::context::{helper_marker_scope, HELPER_MARKER_CLOSE, HELPER_MARKER_OPEN};
use crate::codegen::data_section::{DataSection, DataWord};
use crate::codegen::shared_helper::emit_shared_helper;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::Module;

/// Readable part of a guard helper's entry symbol.
const HELPER_PREFIX: &str = "typed_prop_guard";

/// Bytes of scratch frame the helper keeps: the boxed value, padded to the 16-byte stack grain.
const HELPER_FRAME_BYTES: usize = 16;
/// Where the helper parks the boxed value so a call in the middle of an arm cannot lose it.
const BOX_OFFSET: usize = 0;

/// What the helper answers in the integer result register when the value is acceptable as is.
const VERDICT_ACCEPT: i64 = 0;
/// What the helper answers when the site must build a replacement value.
const VERDICT_COERCE: i64 = 1;

/// The cache key of the helper enforcing `php_type`.
fn helper_key(php_type: &PhpType) -> String {
    format!("{}:{:?}", HELPER_PREFIX, php_type)
}

/// Emits the call to the shared guard, and the site-side coercion arms when the type has any.
///
/// Returns the label the caller must emit after its own value materialization, as the inline
/// guard did: `Some` only when a coercion arm produced a value that bypasses it.
pub(super) fn emit_outlined_guard(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    actions: &[TagAction],
) -> Result<Option<String>> {
    let key = helper_key(&slot.php_type);
    let label = ctx.keyed_global_label(&key, HELPER_PREFIX);
    ctx.shared
        .request_property_guard_helper(&key, &label, &slot.php_type);

    load_value_to_first_int_arg(ctx, value)?;
    // An inherited abstract setter words its refusal as `<prefix><source> given` instead of
    // `Cannot assign <source> to property ...`, so the prefix travels in the descriptor too.
    let (prefix, message) = match &slot.abstract_setter_type_error_prefix {
        Some(prefix) => (prefix.clone(), String::from(" given")),
        None => (
            String::from("Cannot assign "),
            format!(
                " to property {}::${} of type {}",
                slot.class_name,
                slot.property,
                php_type_name(&slot.php_type)
            ),
        ),
    };
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    let descriptor = ctx.data.add_words(vec![
        DataWord::Symbol(message_label),
        DataWord::U64(message_len as u64),
        DataWord::Symbol(prefix_label),
        DataWord::U64(prefix_len as u64),
    ]);
    let descriptor_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_symbol_address(ctx.emitter, descriptor_reg, &descriptor);
    abi::emit_call_label(ctx.emitter, &label);

    if !actions.iter().any(produces_value) {
        return Ok(None);
    }
    let fragment = label_fragment(&slot.property);
    let accept_label = ctx.next_label(&format!("typed_prop_accept_{}", fragment));
    let done_label = ctx.next_label(&format!("typed_prop_coerced_{}", fragment));
    abi::emit_branch_if_int_result_zero(ctx.emitter, &accept_label);
    emit_coercion_arms(ctx, value, slot, actions, &accept_label, &done_label)?;
    ctx.emitter.label(&accept_label);
    Ok(Some(done_label))
}

/// Writes the body of every guard helper the function just emitted asked for.
///
/// Each body is bracketed with the keyed-helper markers: a codegen worker that reaches a type
/// first emits its own copy, and the merge keeps one and drops the rest because the entry symbol
/// and every label inside come from the key alone.
pub(in crate::codegen) fn emit_pending_property_guard_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
) -> Result<()> {
    for pending in shared.take_pending_property_guards() {
        let slot = PropertySlot {
            class_name: String::new(),
            property: String::new(),
            php_type: pending.php_type.clone(),
            offset: 0,
            is_declared: true,
            is_packed: false,
            width: None,
            is_reference: false,
            abstract_setter_type_error_prefix: None,
        };
        emitter.comment(&format!(
            "{}{:012x}",
            HELPER_MARKER_OPEN,
            helper_marker_scope(&pending.key)
        ));
        let emitted = emit_shared_helper(
            module,
            emitter,
            data,
            shared,
            false,
            &pending.label,
            PhpType::Void,
            &format!("--- shared typed-property guard: {} ---", pending.label),
            |ctx| emit_helper_body(ctx, &slot),
        );
        emitter.comment(HELPER_MARKER_CLOSE);
        emitted?;
    }
    Ok(())
}

/// Emits the guard itself: dispatch on the runtime tag, accept, diagnose or refuse.
fn emit_helper_body(ctx: &mut FunctionContext<'_>, slot: &PropertySlot) -> Result<()> {
    let Some(actions) = plan_tag_actions(ctx, slot)? else {
        return Err(CodegenIrError::unsupported(format!(
            "typed property guard for PHP type {:?}",
            slot.php_type
        )));
    };
    let any_value = actions.iter().any(produces_value);
    let target = ctx.emitter.target;
    let nested = abi::nested_call_reg(ctx.emitter);
    let box_arg = abi::int_arg_reg_name(target, 0);
    let descriptor_arg = abi::int_arg_reg_name(target, 1);
    let accept_label = ctx.next_label("typed_prop_accept");
    let coerce_label = ctx.next_label("typed_prop_coerce");
    let finish_label = ctx.next_label("typed_prop_finish");
    let refuse_label = ctx.next_label("typed_prop_refuse");
    let unknown_label = ctx.next_label("typed_prop_reject");
    let arm_labels = RUNTIME_TAGS
        .iter()
        .map(|(tag, name)| ctx.next_label(&format!("typed_prop_{}_{}", tag, name)))
        .collect::<Vec<_>>();

    // The descriptor outlives every call below in the callee-saved nested-call register, and the
    // box in a frame slot, because the object arm needs it again after the bridge call.
    abi::emit_reg_move(ctx.emitter, nested, descriptor_arg);
    abi::emit_reserve_temporary_stack(ctx.emitter, HELPER_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, box_arg, BOX_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    for ((tag, _), label) in RUNTIME_TAGS.iter().zip(arm_labels.iter()) {
        super::super::super::enums::emit_mixed_tag_branch(ctx, tag_reg, i64::from(*tag), label);
    }
    abi::emit_jump(ctx.emitter, &unknown_label);

    for (((tag, type_name), label), action) in
        RUNTIME_TAGS.iter().zip(arm_labels.iter()).zip(actions.iter())
    {
        ctx.emitter.label(label);
        match action {
            TagAction::Accept => abi::emit_jump(ctx.emitter, &accept_label),
            // An object is refused BY CLASS even when no class could have satisfied the
            // declared type: php-src always prints the runtime class here, so `int $n = new T()`
            // reports `T`, never the word `object`.
            TagAction::Reject if *tag == OBJECT_TAG => emit_refuse_object(ctx, &refuse_label),
            // php-src names a refused bool BY VALUE, `true` or `false`, never `bool`.
            TagAction::Reject if *tag == BOOL_TAG => emit_refuse_bool(ctx, &refuse_label),
            TagAction::Reject => emit_refuse_source(ctx, &refuse_label, type_name),
            TagAction::AcceptIntoInt => {
                let refused = ctx.next_label(&format!("typed_prop_refused_{}", type_name));
                emit_int_destination_diagnostics(ctx, *tag, &refused);
                abi::emit_jump(ctx.emitter, &accept_label);
                ctx.emitter.label(&refused);
                emit_refuse_source(ctx, &refuse_label, type_name);
            }
            TagAction::NumericString => {
                emit_numeric_string_check(ctx.emitter, &accept_label);
                emit_refuse_source(ctx, &refuse_label, type_name);
            }
            // The arm builds a value, which only the site can place; it runs the arm whole.
            TagAction::NumericStringIntOrFloat | TagAction::CoerceScalar { .. } => {
                abi::emit_jump(ctx.emitter, &coerce_label)
            }
            TagAction::ObjectClasses(plan) => {
                emit_object_plan_check(ctx, plan, &accept_label, &coerce_label)?;
                emit_refuse_object(ctx, &refuse_label);
            }
        }
    }

    // An unbox result outside the known tag table is not a PHP value this property can hold.
    ctx.emitter.label(&unknown_label);
    emit_refuse_source(ctx, &refuse_label, "object");

    // Every refusal arrives here with the source's description in the string-result registers.
    ctx.emitter.label(&refuse_label);
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = runtime_class_messages::concat_right_operand_regs(ctx.emitter);
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_ptr, text_ptr)); // move the source description into the concat right operand
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_len, text_len)); // move its length into the concat right operand
    abi::emit_load_from_address(ctx.emitter, text_ptr, nested, 16); // the descriptor's message prefix pointer
    abi::emit_load_from_address(ctx.emitter, text_len, nested, 24); // and its length
    abi::emit_call_label(ctx.emitter, "__rt_concat"); // prefix + source description
    abi::emit_load_from_address(ctx.emitter, right_ptr, nested, 0);
    abi::emit_load_from_address(ctx.emitter, right_len, nested, 8);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::super::super::exceptions::emit_type_error_from_string_result(ctx);

    ctx.emitter.label(&accept_label);
    let result_reg = abi::int_result_reg(ctx.emitter);
    if any_value {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, VERDICT_ACCEPT);
        abi::emit_jump(ctx.emitter, &finish_label);
        ctx.emitter.label(&coerce_label);
        abi::emit_load_int_immediate(ctx.emitter, result_reg, VERDICT_COERCE);
    }
    ctx.emitter.label(&finish_label);
    abi::emit_release_temporary_stack(ctx.emitter, HELPER_FRAME_BYTES);
    Ok(())
}

/// Describes a refused source by its static type name and goes to the shared refusal.
fn emit_refuse_source(ctx: &mut FunctionContext<'_>, refuse_label: &str, source: &str) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let (label, len) = ctx.data.add_string(source.as_bytes());
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    abi::emit_jump(ctx.emitter, refuse_label);
}

/// Describes a refused bool BY VALUE, the way php-src prints it.
///
/// The value is still in the unbox payload register at the arm's entry, so a single branch on it
/// picks the spelling.
fn emit_refuse_bool(ctx: &mut FunctionContext<'_>, refuse_label: &str) {
    let true_label = ctx.next_label("typed_prop_bool_true");
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result_reg, payload_reg); // read the unboxed bool payload here
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_label);
    emit_refuse_source(ctx, refuse_label, "false");
    ctx.emitter.label(&true_label);
    emit_refuse_source(ctx, refuse_label, "true");
}

/// Describes a refused object by the class it actually holds, which only run time knows.
///
/// The name is BORROWED from the dense class-name table; the shared refusal persists the composed
/// message before the throwable takes it, because `__rt_concat` leaves its result in scratch
/// storage.
fn emit_refuse_object(ctx: &mut FunctionContext<'_>, refuse_label: &str) {
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result_reg, payload_reg); // the class-name lookup reads the object through the result register
    runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, result_reg, "object");
    abi::emit_jump(ctx.emitter, refuse_label);
}

/// Emits the accepted-class comparisons for the object arm, inside the helper.
///
/// Control reaches `accept_label` for an accepted class and `coerce_label` for a class whose
/// `__toString` a string-accepting slot can call; falling out of the sequence means the object is
/// refused. The `__toString` call itself belongs to the site.
fn emit_object_plan_check(
    ctx: &mut FunctionContext<'_>,
    plan: &ObjectPlan,
    accept_label: &str,
    coerce_label: &str,
) -> Result<()> {
    if plan.any_class {
        abi::emit_jump(ctx.emitter, accept_label);
        return Ok(());
    }
    // An object the interpreter built -- of a class only the runtime autoloader supplied, like
    // Laravel's `CompiledRoute` that Symfony's `Route::compile()` stores in `?CompiledRoute
    // $compiled` -- is answered by the bridge; the class ids below see only compiled hierarchies.
    if ctx.module.required_runtime_features.eval_bridge && !plan.class_names.is_empty() {
        let result_reg = abi::int_result_reg(ctx.emitter);
        for class_name in &plan.class_names {
            let native_fallback = ctx.next_label("typed_prop_eval_miss");
            abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, BOX_OFFSET);
            super::super::super::builtins::emit_eval_object_is_a_named_fallback_from_result(
                ctx,
                class_name.trim_start_matches('\\'),
                accept_label,
                &native_fallback,
                &native_fallback,
            )?;
            ctx.emitter.label(&native_fallback);
        }
        // The bridge call clobbered the unboxed payload the class-id comparisons read.
        let box_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
        abi::emit_load_temporary_stack_slot(ctx.emitter, box_arg, BOX_OFFSET);
        abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    }
    emit_accept_object_classes(ctx.emitter, &plan.accepted, accept_label);
    emit_accept_object_classes(ctx.emitter, &plan.stringable, coerce_label);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One helper per declared type: two properties of the same type share a key, and the key
    /// separates every type the guard decides differently.
    #[test]
    fn the_helper_key_is_the_declared_type_alone() {
        assert_eq!(helper_key(&PhpType::Int), helper_key(&PhpType::Int));
        assert_ne!(helper_key(&PhpType::Int), helper_key(&PhpType::Float));
        assert_ne!(
            helper_key(&PhpType::Object("A".to_string())),
            helper_key(&PhpType::Object("B".to_string()))
        );
        assert_ne!(
            helper_key(&PhpType::Union(vec![PhpType::Int, PhpType::Str])),
            helper_key(&PhpType::Union(vec![PhpType::Int, PhpType::Void]))
        );
    }
}
