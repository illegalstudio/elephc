//! Purpose:
//! Enforces PHP 8.5 weak-mode typed-property assignment for values whose compile-time type
//! is only a boxed `Mixed`/`Union`, so a runtime-shaped value is coerced the way PHP coerces
//! it or rejected with a catchable `TypeError` instead of being silently cast.
//!
//! Called from:
//! - `load_property_store_value_to_result()` in the sibling property-store slice, which is
//!   the single funnel every declared-slot write goes through, including the runtime-class
//!   dispatch for `Mixed` receivers.
//!
//! Key details:
//! - The guard runs BEFORE the destination is touched: it only reads the boxed value, so a
//!   rejected assignment leaves the previous slot contents owned and intact. Every diagnostic
//!   the guard emits (the precision deprecations) is likewise emitted before the store, which
//!   is also where php-src emits it.
//! - The declared property type is known at compile time while the runtime tag is not, so the
//!   verdict for each tag is decided here and the emitted code is a plain tag dispatch. Arms
//!   that PHP accepts fall through to the ordinary conversion machinery; arms PHP coerces to
//!   a different union member produce their own boxed result and jump to the shared done
//!   label; arms PHP rejects throw.
//! - A rejected OBJECT names its runtime class, which is only known at run time, so that arm
//!   composes its message through the shared `runtime_class_messages` helpers rather than
//!   baking a static string.
//! - An `int` destination is the only one with observable coercion diagnostics: PHP 8.5
//!   deprecates a lossy float or float-string and refuses a float outside the int range
//!   entirely. Both are emitted here, before the value is materialized.
//! - A declared type this slice cannot model answers `None`, which keeps the previous lowering
//!   for that slot. `types_the_guard_does_not_model()` documents and pins exactly which those
//!   are, so the fallback is a recorded decision rather than an accident.
//! - A numeric STRING assigned to a union that declares BOTH `int` and `float` is resolved by
//!   the string's own SPELLING, which is php-src's `is_numeric_string` answering `IS_LONG`
//!   versus `IS_DOUBLE`: `"2"` is int while `"2.0"`, `"2.5"`, `"1e3"` and an integer-spelled
//!   string outside the platform int range are all float. `__rt_str_to_number` cannot decide
//!   that (it reports only a boolean numeric flag), so that arm calls `__rt_str_numeric_value`
//!   instead, the shared classifier that answers non-numeric, integer or float AND hands back
//!   the parsed value. Its (tag, value, status) result is the very triple `__rt_mixed_from_value`
//!   consumes, so the selected member is boxed from the value the classifier already parsed and
//!   the string is never scanned twice.

use super::*;
use crate::codegen::lower_inst::runtime_class_messages;
use crate::codegen_support::emit::Emitter;

/// Runtime tags `__rt_mixed_unbox` can report, with the PHP type name each carries into a
/// `TypeError` message. Tag 7 never reaches here: the unbox helper peels nested boxes.
const RUNTIME_TAGS: [(u8, &str); 10] = [
    (0, "int"),
    (1, "string"),
    (2, "float"),
    (3, "bool"),
    (4, "array"),
    (5, "array"),
    (6, "object"),
    (8, "null"),
    (9, "resource"),
    (10, "Closure"),
];

/// The runtime tag carrying a PHP string payload.
const STRING_TAG: u8 = 1;
/// The runtime tag carrying a PHP float payload.
const FLOAT_TAG: u8 = 2;
/// The runtime tag carrying a PHP object payload.
const OBJECT_TAG: u8 = 6;
/// The runtime tag carrying a PHP bool payload.
const BOOL_TAG: u8 = 3;

/// Stack layout of the string-to-int diagnostic frame.
const STRING_TO_INT_PTR_OFFSET: usize = 0;
const STRING_TO_INT_LEN_OFFSET: usize = 8;
const STRING_TO_INT_VALUE_OFFSET: usize = 16;
const STRING_TO_INT_FRAME_BYTES: usize = 32;

/// Stack layout of the `__toString` persistence frame.
const TOSTRING_SOURCE_PTR_OFFSET: usize = 0;
const TOSTRING_SOURCE_LEN_OFFSET: usize = 8;
const TOSTRING_OWNED_PTR_OFFSET: usize = 16;
const TOSTRING_OWNED_LEN_OFFSET: usize = 24;
const TOSTRING_FRAME_BYTES: usize = 32;

/// Stack layout of the boxed-string handover frame.
const BOXED_STRING_SOURCE_PTR_OFFSET: usize = 0;
const BOXED_STRING_CELL_OFFSET: usize = 16;
const BOXED_STRING_FRAME_BYTES: usize = 32;

/// What the guard does with one runtime tag.
#[derive(Clone, Debug)]
enum TagAction {
    /// PHP accepts this source type; continue into the ordinary conversion path.
    Accept,
    /// PHP accepts this source for an `int` destination, after the coercion diagnostics that
    /// destination owes: a lossy value is deprecated and an unrepresentable one is refused.
    AcceptIntoInt,
    /// PHP accepts this string only when it is a numeric string.
    NumericString,
    /// PHP resolves this string against a union declaring BOTH `int` and `float` by the
    /// string's own numeric classification, so the member is only known at run time.
    NumericStringIntOrFloat,
    /// PHP decides this object by its runtime class.
    ObjectClasses(ObjectPlan),
    /// PHP coerces this source to another union member; the arm produces the boxed result.
    ///
    /// `fallback` is the NEXT member PHP tries when the preferred one cannot represent the
    /// value. Only an `int` target can fail that way, and php-src does not stop there: a float
    /// outside the int range assigned to `int|string` becomes the STRING `"1.0E+30"`, and
    /// assigned to `int|bool` becomes `true`.
    CoerceScalar {
        target: PhpType,
        require_numeric_string: bool,
        fallback: Option<PhpType>,
    },
    /// PHP raises `TypeError` for this source type.
    Reject,
}

/// How one declared property type treats the runtime classes this module can produce.
#[derive(Clone, Debug, Default)]
struct ObjectPlan {
    /// Every object satisfies the declared type, which is what a bare `object` declares.
    any_class: bool,
    /// Runtime class ids that satisfy the declared type as they are.
    accepted: Vec<u64>,
    /// Runtime class ids publishing `__toString`, which a `string` destination accepts by
    /// calling it. Empty for every destination that is not string-shaped.
    stringable: Vec<u64>,
}

impl ObjectPlan {
    /// Returns true when no object of any class can satisfy the declared type.
    fn rejects_every_class(&self) -> bool {
        !self.any_class && self.accepted.is_empty() && self.stringable.is_empty()
    }
}

/// The PHP types a declared property type accepts, in the shape the guard reasons about.
#[derive(Clone, Debug, PartialEq)]
enum MemberKind {
    Int,
    Float,
    Str,
    Bool,
    Null,
    Array,
    /// A named class, or `AnyObject` for the bare `object` type.
    Object(String),
    AnyObject,
}

/// Returns true when the guard can enforce this declared property type.
///
/// The runtime-class dispatch consults this before excluding a class: a declared slot the
/// guard understands accepts a runtime-shaped value and validates it at run time, so the
/// class stays in the dispatch instead of silently dropping the write.
pub(super) fn property_type_accepts_runtime_shaped_value(
    ctx: &FunctionContext<'_>,
    slot: &PropertySlot,
) -> Result<bool> {
    Ok(plan_tag_actions(ctx, slot)?.is_some())
}

/// Emits the weak-mode type guard for a boxed value assigned to a declared property.
///
/// Returns the label the caller must emit AFTER its own value materialization, when the
/// guard produced coercion arms that bypass it. Falling through means the value is accepted
/// as-is and the ordinary conversion path applies.
pub(super) fn emit_mixed_property_type_guard(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
) -> Result<Option<String>> {
    let Some(actions) = plan_tag_actions(ctx, slot)? else {
        return Ok(None);
    };
    if actions.iter().all(|action| matches!(action, TagAction::Accept)) {
        return Ok(None);
    }
    let fragment = label_fragment(&slot.property);
    let accept_label = ctx.next_label(&format!("typed_prop_accept_{}", fragment));
    let done_label = ctx.next_label(&format!("typed_prop_coerced_{}", fragment));
    let unknown_label = ctx.next_label(&format!("typed_prop_reject_{}", fragment));
    let arm_labels = RUNTIME_TAGS
        .iter()
        .map(|(tag, name)| ctx.next_label(&format!("typed_prop_{}_{}_{}", fragment, tag, name)))
        .collect::<Vec<_>>();

    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    for ((tag, _), label) in RUNTIME_TAGS.iter().zip(arm_labels.iter()) {
        super::super::enums::emit_mixed_tag_branch(ctx, tag_reg, i64::from(*tag), label);
    }
    abi::emit_jump(ctx.emitter, &unknown_label);

    let mut needs_done_label = false;
    for (((tag, type_name), label), action) in
        RUNTIME_TAGS.iter().zip(arm_labels.iter()).zip(actions.iter())
    {
        ctx.emitter.label(label);
        match action {
            TagAction::Accept => abi::emit_jump(ctx.emitter, &accept_label),
            // An object is refused BY CLASS even when no class could have satisfied the
            // declared type: php-src always prints the runtime class here, so `int $n = new T()`
            // reports `T`, never the word `object`.
            TagAction::Reject if *tag == OBJECT_TAG => emit_object_property_type_error(ctx, slot),
            // php-src names a refused bool BY VALUE, `true` or `false`, never `bool`, the same
            // way `count()`'s refusal does.
            TagAction::Reject if *tag == BOOL_TAG => emit_bool_property_type_error(ctx, slot),
            TagAction::Reject => emit_property_type_error(ctx, slot, type_name),
            TagAction::AcceptIntoInt => {
                let refuse_label =
                    ctx.next_label(&format!("typed_prop_{}_refused_{}", fragment, type_name));
                emit_int_destination_diagnostics(ctx, slot, *tag, &refuse_label);
                abi::emit_jump(ctx.emitter, &accept_label);
                ctx.emitter.label(&refuse_label);
                emit_property_type_error(ctx, slot, type_name);
            }
            TagAction::NumericString => {
                emit_numeric_string_check(ctx.emitter, &accept_label);
                emit_property_type_error(ctx, slot, type_name);
            }
            TagAction::NumericStringIntOrFloat => {
                let refuse_label =
                    ctx.next_label(&format!("typed_prop_{}_refused_{}", fragment, type_name));
                emit_classified_numeric_string_member(ctx, value, slot, &refuse_label)?;
                needs_done_label = true;
                abi::emit_jump(ctx.emitter, &done_label);
                ctx.emitter.label(&refuse_label);
                emit_property_type_error(ctx, slot, type_name);
            }
            TagAction::ObjectClasses(plan) => {
                if emit_object_plan(ctx, value, slot, plan, &accept_label, &done_label)? {
                    needs_done_label = true;
                }
                emit_object_property_type_error(ctx, slot);
            }
            TagAction::CoerceScalar {
                target,
                require_numeric_string,
                fallback,
            } => {
                if *target == PhpType::Int && matches!(*tag, STRING_TAG | FLOAT_TAG) {
                    // A union that narrows to `int` owes the same PHP 8.5 diagnostics a plain
                    // `int` property owes: a lossy value is deprecated, and one the int range
                    // cannot hold moves on to the union's next member instead.
                    let refuse_label =
                        ctx.next_label(&format!("typed_prop_{}_refused_{}", fragment, type_name));
                    let coerce_label =
                        ctx.next_label(&format!("typed_prop_{}_coerce_{}", fragment, type_name));
                    emit_int_destination_diagnostics(ctx, slot, *tag, &refuse_label);
                    abi::emit_jump(ctx.emitter, &coerce_label);
                    ctx.emitter.label(&refuse_label);
                    match fallback {
                        Some(next) => {
                            emit_coerced_property_value(ctx, value, slot, next)?;
                            abi::emit_jump(ctx.emitter, &done_label);
                        }
                        None => emit_property_type_error(ctx, slot, type_name),
                    }
                    ctx.emitter.label(&coerce_label);
                } else if *require_numeric_string {
                    let coerce_label =
                        ctx.next_label(&format!("typed_prop_{}_coerce_{}", fragment, type_name));
                    emit_numeric_string_check(ctx.emitter, &coerce_label);
                    emit_property_type_error(ctx, slot, type_name);
                    ctx.emitter.label(&coerce_label);
                }
                emit_coerced_property_value(ctx, value, slot, target)?;
                needs_done_label = true;
                abi::emit_jump(ctx.emitter, &done_label);
            }
        }
    }

    // An unbox result outside the known tag table is not a PHP value this property can hold.
    ctx.emitter.label(&unknown_label);
    emit_property_type_error(ctx, slot, "object");
    ctx.emitter.label(&accept_label);
    Ok(needs_done_label.then_some(done_label))
}

/// Emits the accepted-class comparisons for one object arm.
///
/// Returns whether a `__toString` arm was emitted, which is the only path here that produces
/// the stored value itself and therefore needs the caller's shared done label.
fn emit_object_plan(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    plan: &ObjectPlan,
    accept_label: &str,
    done_label: &str,
) -> Result<bool> {
    if plan.any_class {
        abi::emit_jump(ctx.emitter, accept_label);
        return Ok(false);
    }
    if plan.accepted.is_empty() && plan.stringable.is_empty() {
        return Ok(false);
    }
    let stringify_label = ctx.next_label(&format!(
        "typed_prop_{}_tostring",
        label_fragment(&slot.property)
    ));
    emit_accept_object_classes(ctx.emitter, &plan.accepted, accept_label);
    if plan.stringable.is_empty() {
        return Ok(false);
    }
    emit_accept_object_classes(ctx.emitter, &plan.stringable, &stringify_label);
    let rejected_label = ctx.next_label(&format!(
        "typed_prop_{}_tostring_rejected",
        label_fragment(&slot.property)
    ));
    abi::emit_jump(ctx.emitter, &rejected_label);

    // PHP calls `__toString` for a string destination. The call can throw, and when it does the
    // unwinder takes over before anything below runs, so the destination still holds the value
    // it held before the assignment started.
    ctx.emitter.label(&stringify_label);
    super::super::conversions::emit_mixed_string_context_result(ctx, value)?;
    emit_persist_and_release_tostring_result(ctx);
    if slot.php_type.codegen_repr() != PhpType::Str {
        emit_box_persisted_string_and_release(ctx);
    }
    emit_release_untransferred_source_box(ctx, value, slot)?;
    abi::emit_jump(ctx.emitter, done_label);
    ctx.emitter.label(&rejected_label);
    Ok(true)
}

/// Persists the `__toString` result into the slot's own storage and releases the call's own.
///
/// `__toString` hands back an OWNED string the consumer has to release, which is why the
/// ordinary `(string)` cast path emits a release for the same value through its EIR ownership
/// annotation. The guard has no SSA value to annotate, so the release is emitted here; without
/// it every accepted Stringable assignment leaked one string per write.
///
/// `__rt_str_persist` takes a concat temporary over IN PLACE rather than duplicating it, so the
/// release is skipped when it handed back the very pointer it was given. Releasing that pointer
/// would free the string the property is about to store.
fn emit_persist_and_release_tostring_result(ctx: &mut FunctionContext<'_>) {
    let keep_label = ctx.next_label("typed_prop_tostring_taken_over");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, TOSTRING_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, TOSTRING_SOURCE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, len_reg, TOSTRING_SOURCE_LEN_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, TOSTRING_OWNED_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, len_reg, TOSTRING_OWNED_LEN_OFFSET);
    let source_reg = abi::int_result_reg(ctx.emitter);
    let scratch_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, source_reg, TOSTRING_SOURCE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch_reg, TOSTRING_OWNED_PTR_OFFSET);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", source_reg, scratch_reg)); // did str_persist take the source block over in place?
            ctx.emitter.instruction(&format!("b.eq {}", keep_label));           // a taken-over block is now the property's own storage
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", source_reg, scratch_reg)); // did str_persist take the source block over in place?
            ctx.emitter.instruction(&format!("je {}", keep_label));             // a taken-over block is now the property's own storage
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    ctx.emitter.label(&keep_label);
    abi::emit_load_temporary_stack_slot(ctx.emitter, ptr_reg, TOSTRING_OWNED_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, len_reg, TOSTRING_OWNED_LEN_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, TOSTRING_FRAME_BYTES);
}

/// Boxes an owned string into a `Mixed` cell and releases the string the box copied.
///
/// `__rt_mixed_from_value` persists a string payload into storage the CELL owns, and the source
/// reaching here is already an owned kind-1 block (`__rt_str_persist` retagged or duplicated it),
/// so the persist inside the box always duplicates rather than taking the block over. The source
/// therefore has exactly one owner left, this sequence, and leaks without the release.
fn emit_box_persisted_string_and_release(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, _) = abi::string_result_regs(ctx.emitter);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reserve_temporary_stack(ctx.emitter, BOXED_STRING_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, ptr_reg, BOXED_STRING_SOURCE_PTR_OFFSET);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    abi::emit_store_to_sp(ctx.emitter, result_reg, BOXED_STRING_CELL_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, BOXED_STRING_SOURCE_PTR_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, BOXED_STRING_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, BOXED_STRING_FRAME_BYTES);
}

/// Branches to `accepted_label` when the unboxed string payload is a PHP numeric string.
///
/// `__rt_str_to_number` reads the string through the string-result registers, which is
/// already where AArch64's unbox payload pair lands; only x86_64 needs the pointer moved.
///
/// The flag this reads is php-src's `is_numeric_string(..., allow_errors = 0)`: a LEADING
/// numeric string such as `"5abc"` answers 0 and is refused, which is what PHP 8 does for a
/// typed destination since the saner-string-to-number change. Only arithmetic contexts accept
/// the numeric prefix, and a property assignment is not one.
fn emit_numeric_string_check(emitter: &mut Emitter, accepted_label: &str) {
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    let (string_ptr_reg, _) = abi::string_result_regs(emitter);
    if string_ptr_reg != payload_reg {
        emitter.instruction(&format!("mov {}, {}", string_ptr_reg, payload_reg)); // move the unboxed string pointer into the numeric-scan input register
    }
    abi::emit_call_label(emitter, "__rt_str_to_number");
    let flag_reg = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {}, {}", flag_reg, accepted_label)); // PHP accepts a fully numeric string for this property
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", flag_reg, flag_reg));   // set flags from the numeric-string verdict
            emitter.instruction(&format!("jne {}", accepted_label));            // PHP accepts a fully numeric string for this property
        }
    }
}

/// Boxes a numeric string as the union member PHP's own numeric classification selects.
///
/// This is the `int|float` case, the one place where the destination member is decided by the
/// string's SPELLING rather than by the declared type alone. `__rt_str_numeric_value` is the
/// shared classifier for that rule: given the borrowed pointer/length pair it answers a runtime
/// value tag (`0` int, `2` float) in the int result register, the parsed value in the Mixed
/// unbox low register (the exact 64-bit integer, or the raw double bits), and a status in the
/// Mixed unbox high register that is `0` only for a WHOLE-string numeric spelling. It owns the
/// complete PHP 8.5 grammar: surrounding PHP whitespace, sign, decimal point, exponent, and
/// `strtoll` overflow all select float exactly as php-src does, while trailing junk, `"0x1A"`,
/// the empty string and whitespace alone report a nonzero status and are refused here.
///
/// That triple is exactly `__rt_mixed_from_value`'s `(tag, low, high)` contract, so the chosen
/// member is boxed straight from the value the classifier already parsed: nothing rescans the
/// string, and no coercion helper is called a second time.
///
/// Ownership follows the sibling coercion path: the fresh box is the value the slot stores, and
/// the source box EIR expected the slot to adopt is released before the arm falls through.
fn emit_classified_numeric_string_member(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    refuse_label: &str,
) -> Result<()> {
    emit_classify_numeric_string_and_box(ctx.emitter, refuse_label);
    emit_release_untransferred_source_box(ctx, value, slot)
}

/// Emits the classifier call and the boxing of whichever member it selected.
///
/// Split out of `emit_classified_numeric_string_member()` so the register contract between
/// `__rt_str_numeric_value` and `__rt_mixed_from_value` can be pinned on every supported target
/// without a lowering context: the two helpers agree on the registers only by convention, and
/// reading the length or the status out of the wrong one is silent on the host architecture.
fn emit_classify_numeric_string_and_box(emitter: &mut Emitter, refuse_label: &str) {
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    let (_, string_len_reg) = abi::string_result_regs(emitter);
    let ptr_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let len_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    // The pointer is read out of the payload register FIRST: on AArch64 the length argument
    // register IS the payload register, so the other order would classify the length instead.
    abi::emit_reg_move(emitter, ptr_arg_reg, payload_reg);
    abi::emit_reg_move(emitter, len_arg_reg, string_len_reg);
    abi::emit_call_label(emitter, "__rt_str_numeric_value");

    // The classifier reports its whole-string status in the Mixed unbox HIGH register, which is
    // the same register the borrowed string length arrived in.
    let status_reg = string_len_reg;
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbnz {}, {}", status_reg, refuse_label)); // only a whole-string numeric spelling reaches an int|float member
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", status_reg, status_reg)); // set flags from the whole-string numeric status
            emitter.instruction(&format!("jne {}", refuse_label));              // only a whole-string numeric spelling reaches an int|float member
        }
    }

    // The classifier already left the runtime tag in the boxing helper's tag register and the
    // parsed value in its low-word register; an int and a float payload both use the low word.
    let (_, high_arg_reg) = mixed_from_value_payload_regs(emitter);
    abi::emit_load_int_immediate(emitter, high_arg_reg, 0); // neither an int nor a float member uses the high payload word
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Returns the low and high payload argument registers `__rt_mixed_from_value` reads.
fn mixed_from_value_payload_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}

/// Branches to `accepted_label` when the unboxed object belongs to an accepted class.
fn emit_accept_object_classes(emitter: &mut Emitter, class_ids: &[u64], accepted_label: &str) {
    if class_ids.is_empty() {
        return;
    }
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    let class_reg = abi::secondary_scratch_reg(emitter);
    let expected_reg = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, class_reg, payload_reg, 0);
    for class_id in class_ids {
        abi::emit_load_int_immediate(emitter, expected_reg, *class_id as i64);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, {}", class_reg, expected_reg)); // compare the runtime class id with an accepted class
                emitter.instruction(&format!("b.eq {}", accepted_label));       // the object satisfies the declared property class
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, {}", class_reg, expected_reg)); // compare the runtime class id with an accepted class
                emitter.instruction(&format!("je {}", accepted_label));         // the object satisfies the declared property class
            }
        }
    }
}

/// Emits the PHP 8.5 diagnostics an `int` destination owes for one runtime source tag.
///
/// Control falls through when PHP accepts the value, after emitting any deprecation; a value
/// PHP refuses throws from here and never reaches the store. Float sources are the interesting
/// case in both directions: `3.7` is stored as `3` after a deprecation, while `NAN`, `INF`, and
/// anything outside the int range are a `TypeError` rather than a silently wrapped integer.
fn emit_int_destination_diagnostics(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    tag: u8,
    refuse_label: &str,
) {
    match tag {
        FLOAT_TAG => emit_float_into_int_diagnostics(ctx, refuse_label),
        STRING_TAG => emit_string_into_int_diagnostics(ctx, slot, refuse_label),
        _ => {}
    }
}

/// Diagnoses a float payload against PHP's implicit int-coercion rules.
fn emit_float_into_int_diagnostics(ctx: &mut FunctionContext<'_>, refuse_label: &str) {
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    super::super::mixed_narrowing::emit_float_bits_to_float_result(ctx, payload_reg);
    super::super::builtins::strings::emit_float_result_int_coercion_diagnostics(ctx, refuse_label);
}

/// Diagnoses a numeric-string payload against PHP's implicit int-coercion rules.
///
/// The original string bytes have to survive the conversion because PHP quotes them verbatim
/// in the deprecation, so the pointer/length pair is saved before `__rt_str_to_number` runs.
fn emit_string_into_int_diagnostics(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    caller_refuse_label: &str,
) {
    let _ = slot;
    let numeric_label = ctx.next_label("typed_prop_string_numeric");
    let refuse_label = ctx.next_label("typed_prop_string_refused");
    let exact_label = ctx.next_label("typed_prop_string_exact");
    let accept_label = ctx.next_label("typed_prop_string_accepted");
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(ctx.emitter);
    if string_ptr_reg != payload_reg {
        ctx.emitter
            .instruction(&format!("mov {}, {}", string_ptr_reg, payload_reg)); // move the unboxed string pointer into the numeric-scan input register
    }
    abi::emit_reserve_temporary_stack(ctx.emitter, STRING_TO_INT_FRAME_BYTES);
    abi::emit_store_to_sp(ctx.emitter, string_ptr_reg, STRING_TO_INT_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, string_len_reg, STRING_TO_INT_LEN_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
    let flag_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cbnz {}, {}", flag_reg, numeric_label)); // only a fully numeric string can reach an int property
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flag_reg, flag_reg)); // set flags from the numeric-string verdict
            ctx.emitter
                .instruction(&format!("jne {}", numeric_label)); // only a fully numeric string can reach an int property
        }
    }
    // `refuse_label` owns the release. Releasing here TOO popped the diagnostic frame twice, so
    // a non-numeric string left the stack pointer above the caller's live call-operand owner
    // record; the throw that follows then overwrote that record's cleanup pointer with a return
    // address and unwinding jumped into the stack. Reached as `clone($o, ["intProp" => "abc"])`.
    abi::emit_jump(ctx.emitter, &refuse_label);

    ctx.emitter.label(&numeric_label);
    super::super::mixed_narrowing::emit_float_result_fits_i64_or_jump(ctx, &refuse_label);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        STRING_TO_INT_VALUE_OFFSET,
    );
    emit_truncation_is_exact_jump(ctx, STRING_TO_INT_VALUE_OFFSET, &exact_label);
    emit_float_string_precision_deprecation(ctx);
    ctx.emitter.label(&exact_label);
    abi::emit_release_temporary_stack(ctx.emitter, STRING_TO_INT_FRAME_BYTES);
    abi::emit_jump(ctx.emitter, &accept_label);

    // `emit_float_result_fits_i64_or_jump` can also land here with the diagnostic frame still
    // reserved, so the refusal releases it before the throw leaves this sequence.
    ctx.emitter.label(&refuse_label);
    abi::emit_release_temporary_stack(ctx.emitter, STRING_TO_INT_FRAME_BYTES);
    abi::emit_jump(ctx.emitter, caller_refuse_label);
    ctx.emitter.label(&accept_label);
}

/// Jumps to `exact_label` when truncating the float result to the saved integer lost nothing.
fn emit_truncation_is_exact_jump(
    ctx: &mut FunctionContext<'_>,
    value_offset: usize,
    exact_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", value_offset);
            ctx.emitter.instruction("scvtf d1, x9");                            // reconstruct the truncated value for an exactness check
            ctx.emitter.instruction("fcmp d0, d1");                             // detect fractional precision loss
            ctx.emitter
                .instruction(&format!("b.eq {}", exact_label)); // integral values need no deprecation
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", value_offset);
            ctx.emitter.instruction("cvtsi2sd xmm1, r10");                      // reconstruct the truncated value for an exactness check
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // detect fractional precision loss
            ctx.emitter.instruction(&format!("je {}", exact_label));            // integral values need no deprecation
        }
    }
}

/// Emits PHP's float-string precision deprecation with the original string quoted verbatim.
fn emit_float_string_precision_deprecation(ctx: &mut FunctionContext<'_>) {
    runtime_class_messages::emit_static_diagnostic(
        ctx,
        "Deprecated: Implicit conversion from float-string \"",
        false,
    );
    let (fragment_ptr_reg, fragment_len_reg) = runtime_class_messages::diagnostic_fragment_regs(
        ctx.emitter,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, fragment_ptr_reg, STRING_TO_INT_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, fragment_len_reg, STRING_TO_INT_LEN_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    runtime_class_messages::emit_static_diagnostic(ctx, "\" to int loses precision\n", true);
}

/// Materializes the declared member PHP coerces this source to, in the slot's own storage.
///
/// A Mixed-shaped slot receives a fresh box that owns the coerced value; inline nullable-int
/// storage receives the tagged pair instead, because a `?int` slot has no box to hold and the
/// generic Mixed-to-tagged move would otherwise store a raw string pointer under a string tag.
fn emit_coerced_property_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    target: &PhpType,
) -> Result<()> {
    load_value_to_first_int_arg(ctx, value)?;
    match target {
        PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int"),
        PhpType::Float => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float"),
        PhpType::Bool => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool"),
        PhpType::Str => {
            emit_mixed_string_for_persistent_store(ctx);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "typed property coercion to PHP type {:?}",
                other
            )))
        }
    }
    if slot.php_type.codegen_repr() == PhpType::TaggedScalar {
        crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
        return emit_release_untransferred_source_box(ctx, value, slot);
    }
    emit_box_current_value_as_mixed(ctx.emitter, target);
    emit_release_untransferred_source_box(ctx, value, slot)
}

/// Releases the source box the ordinary accept path would have handed to the slot.
///
/// `can_store_boxed_value_for_mixed_property()` makes a Mixed-shaped slot ADOPT the source box
/// outright, so EIR emits no cleanup for a source it expects to be handed over. An arm that
/// stores a DIFFERENT value instead leaves that box with no owner at all, which leaked one
/// boxed cell per accepted write until the release was emitted here.
fn emit_release_untransferred_source_box(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
) -> Result<()> {
    if !matches!(
        slot.php_type.codegen_repr(),
        PhpType::Mixed | PhpType::TaggedScalar
    ) {
        return Ok(());
    }
    release_adopted_mixed_source(ctx, value, &slot.php_type)
}

/// Throws PHP's catchable typed-property `TypeError` for this source type.
fn emit_property_type_error(ctx: &mut FunctionContext<'_>, slot: &PropertySlot, source: &str) {
    let message = format!(
        "Cannot assign {} to property {}::${} of type {}",
        source,
        slot.class_name,
        slot.property,
        php_type_name(&slot.php_type)
    );
    super::super::exceptions::emit_type_error(ctx, &message);
}

/// Throws the property `TypeError` for a bool source, naming it BY VALUE.
///
/// php-src prints `true` or `false` here, never `bool`, which is the same rule its `count()`
/// refusal follows. The value is still in the unbox payload register at the arm's entry, so the
/// two messages are chosen by a single branch on it.
fn emit_bool_property_type_error(ctx: &mut FunctionContext<'_>, slot: &PropertySlot) {
    let true_label = ctx.next_label("typed_prop_bool_true");
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result_reg, payload_reg); // read the unboxed bool payload here
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &true_label);
    emit_property_type_error(ctx, slot, "false");
    ctx.emitter.label(&true_label);
    emit_property_type_error(ctx, slot, "true");
}

/// Throws the same `TypeError` for an object, naming the class the value actually holds.
///
/// php-src prints the runtime class here, not the word `object`, and the class is only known at
/// run time, so the message is composed from the dense class-name table the same way `count()`
/// composes its own refusal. The composed message is persisted before the throwable takes it,
/// because `__rt_concat` leaves its result in scratch storage.
fn emit_object_property_type_error(ctx: &mut FunctionContext<'_>, slot: &PropertySlot) {
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result_reg, payload_reg); // the class-name lookup reads the object through the result register
    runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, result_reg, "object");
    runtime_class_messages::emit_concat_static_prefix(ctx, "Cannot assign ");
    runtime_class_messages::emit_concat_static_suffix(
        ctx,
        &format!(
            " to property {}::${} of type {}",
            slot.class_name,
            slot.property,
            php_type_name(&slot.php_type)
        ),
    );
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    super::super::exceptions::emit_type_error_from_string_result(ctx);
}

/// Spells a declared property type the way PHP spells it in a `TypeError`.
fn php_type_name(php_type: &PhpType) -> String {
    match php_type {
        PhpType::Int => "int".to_string(),
        PhpType::Float => "float".to_string(),
        PhpType::Str => "string".to_string(),
        PhpType::Bool | PhpType::False => "bool".to_string(),
        PhpType::Void | PhpType::Never => "null".to_string(),
        PhpType::Mixed => "mixed".to_string(),
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
        PhpType::Iterable => "Traversable|array".to_string(),
        PhpType::Object(class_name) if class_name.is_empty() => "object".to_string(),
        PhpType::Object(class_name) => class_name.trim_start_matches('\\').to_string(),
        PhpType::Union(members) => php_union_type_name(members),
        other => format!("{:?}", other),
    }
}

/// The order php-src prints built-in union members in, after the class names.
///
/// `zend_type_to_string` walks a fixed type mask, so a union is NEVER printed in declaration
/// order: `int|string` prints as `string|int` and `null|int|string` as `string|int|null`. A
/// message built from declaration order disagrees with reference PHP for most unions, which is
/// why the spelling is normalized here instead of being joined as written.
const PHP_BUILTIN_TYPE_ORDER: [&str; 8] = [
    "object", "array", "string", "int", "float", "bool", "false", "true",
];

/// Spells a union the way PHP does, collapsing a nullable single type to `?T`.
fn php_union_type_name(members: &[PhpType]) -> String {
    let is_null = |member: &PhpType| matches!(member, PhpType::Void | PhpType::Never);
    let has_null = members.iter().any(is_null);
    let mut classes: Vec<String> = Vec::new();
    let mut builtins: Vec<String> = Vec::new();
    for member in members.iter().filter(|member| !is_null(member)) {
        for name in member_type_names(member) {
            let bucket = if PHP_BUILTIN_TYPE_ORDER.contains(&name.as_str()) {
                &mut builtins
            } else {
                &mut classes
            };
            if !bucket.contains(&name) {
                bucket.push(name);
            }
        }
    }
    builtins.sort_by_key(|name| {
        PHP_BUILTIN_TYPE_ORDER
            .iter()
            .position(|known| known == name)
            .unwrap_or(PHP_BUILTIN_TYPE_ORDER.len())
    });
    let mut named = classes;
    named.extend(builtins);
    if named.len() == 1 && has_null {
        return format!("?{}", named[0]);
    }
    if has_null {
        named.push("null".to_string());
    }
    named.join("|")
}

/// The individual names one declared union member contributes to the printed spelling.
///
/// `array` reaches codegen as the two-member union of its indexed and hash representations, so
/// both collapse to one `array`; `iterable` is stored as one member but PHP prints it as the
/// two types it stands for.
fn member_type_names(member: &PhpType) -> Vec<String> {
    match member {
        PhpType::Iterable => vec!["Traversable".to_string(), "array".to_string()],
        PhpType::Union(inner) => inner.iter().flat_map(member_type_names).collect(),
        other => vec![php_type_name(other)],
    }
}

/// Decides what the guard does with every runtime tag for this declared property type.
///
/// `None` means the declared type is outside what this slice models, so the caller keeps its
/// previous lowering unchanged rather than enforcing a semantics it cannot describe.
fn plan_tag_actions(
    ctx: &FunctionContext<'_>,
    slot: &PropertySlot,
) -> Result<Option<Vec<TagAction>>> {
    // PHP applies weak-mode property typing only to properties that DECLARE a type. An
    // untyped `public $v` accepts every value, and the inferred storage type this backend
    // gives it is an implementation detail that must never reject an assignment.
    //
    // A by-reference DESTINATION is deliberately NOT excluded here. php keeps the declared
    // type on a property that also holds a shared cell, so `$alias = &$o->p;` does not make
    // `$o->p = "nope"` legal on `public int $p`. Skipping the guard for those slots silently
    // cast the value with `__rt_mixed_cast_int` and published the result through the cell, so
    // every alias observed a value php never stores. A PACKED field is still excluded: its
    // fixed-layout storage runs the stricter `PackedFieldMixedToInt` narrowing instead.
    if !slot.is_declared || slot.is_packed {
        return Ok(None);
    }
    Ok(match &slot.php_type {
        // A `mixed` property holds every PHP value. It still has to answer here, because the
        // runtime-class dispatch drops a class the guard cannot speak for: answering `None`
        // silently lost the write instead of accepting it.
        PhpType::Mixed => Some(actions_for(|_| TagAction::Accept)),
        PhpType::Int => Some(int_actions()),
        PhpType::Float => Some(float_actions()),
        PhpType::Str => Some(string_actions(ctx)?),
        PhpType::Bool | PhpType::False => Some(bool_actions()),
        PhpType::Void | PhpType::Never => Some(null_actions()),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some(array_actions()),
        PhpType::Iterable => Some(iterable_actions(ctx)),
        PhpType::Object(class_name) if class_name.is_empty() => Some(any_object_actions()),
        PhpType::Object(class_name) => {
            Some(object_actions(ctx, std::slice::from_ref(class_name)))
        }
        PhpType::Union(members) => union_actions(ctx, members, &slot.php_type)?,
        _ => None,
    })
}

/// Builds an action list from one closure over the runtime tag table.
fn actions_for(mut action: impl FnMut(u8) -> TagAction) -> Vec<TagAction> {
    RUNTIME_TAGS.iter().map(|(tag, _)| action(*tag)).collect()
}

/// `int` properties: ints and bools pass, floats and numeric strings pass with PHP's
/// precision diagnostics, nothing else.
fn int_actions() -> Vec<TagAction> {
    actions_for(|tag| match tag {
        0 | 3 => TagAction::Accept,
        STRING_TAG | FLOAT_TAG => TagAction::AcceptIntoInt,
        _ => TagAction::Reject,
    })
}

/// `float` properties: every scalar coerces and numeric strings coerce, with no diagnostic.
///
/// A float destination loses no precision by construction: `"1e400"` is `INF` and `"5abc"` is
/// already refused by the numeric-string rule, so there is nothing left for PHP to deprecate.
fn float_actions() -> Vec<TagAction> {
    actions_for(|tag| match tag {
        0 | 2 | 3 => TagAction::Accept,
        STRING_TAG => TagAction::NumericString,
        _ => TagAction::Reject,
    })
}

/// `bool` properties: every scalar coerces, including non-numeric strings.
fn bool_actions() -> Vec<TagAction> {
    actions_for(|tag| match tag {
        0 | 1 | 2 | 3 => TagAction::Accept,
        _ => TagAction::Reject,
    })
}

/// `string` properties: every scalar coerces, and an object does when it publishes
/// `__toString`.
fn string_actions(ctx: &FunctionContext<'_>) -> Result<Vec<TagAction>> {
    let stringable = stringable_class_ids(ctx)?;
    Ok(actions_for(|tag| match tag {
        0 | 1 | 2 | 3 => TagAction::Accept,
        6 => TagAction::ObjectClasses(ObjectPlan {
            stringable: stringable.clone(),
            ..ObjectPlan::default()
        }),
        _ => TagAction::Reject,
    }))
}

/// A slot whose declared type is PHP `null` accepts only null.
fn null_actions() -> Vec<TagAction> {
    actions_for(|tag| match tag {
        8 => TagAction::Accept,
        _ => TagAction::Reject,
    })
}

/// `array` properties accept both runtime container shapes and nothing else.
fn array_actions() -> Vec<TagAction> {
    actions_for(|tag| match tag {
        4 | 5 => TagAction::Accept,
        _ => TagAction::Reject,
    })
}

/// `iterable` properties accept both runtime container shapes and every `Traversable` object.
///
/// PHP spells `iterable` as `array|Traversable`, so an object is decided BY CLASS through the same
/// object plan a class-typed property uses instead of a blanket object rejection. That is what
/// makes a refused ordinary object name its runtime class the way php-src names it.
fn iterable_actions(ctx: &FunctionContext<'_>) -> Vec<TagAction> {
    let traversable = [TRAVERSABLE_INTERFACE.to_string()];
    let class_ids = compatible_class_ids(ctx, &traversable);
    actions_for(|tag| match tag {
        4 | 5 => TagAction::Accept,
        6 => TagAction::ObjectClasses(ObjectPlan {
            accepted: class_ids.clone(),
            ..ObjectPlan::default()
        }),
        _ => TagAction::Reject,
    })
}

/// The bare `object` type accepts every object and refuses every non-object.
fn any_object_actions() -> Vec<TagAction> {
    actions_for(|tag| match tag {
        6 => TagAction::ObjectClasses(ObjectPlan {
            any_class: true,
            ..ObjectPlan::default()
        }),
        _ => TagAction::Reject,
    })
}

/// Class-typed properties accept the runtime classes that satisfy the declared class list.
fn object_actions(ctx: &FunctionContext<'_>, class_names: &[String]) -> Vec<TagAction> {
    let class_ids = compatible_class_ids(ctx, class_names);
    actions_for(|tag| match tag {
        6 => TagAction::ObjectClasses(ObjectPlan {
            accepted: class_ids.clone(),
            ..ObjectPlan::default()
        }),
        _ => TagAction::Reject,
    })
}

/// Collects the runtime class ids in this module that satisfy any of the declared classes.
fn compatible_class_ids(ctx: &FunctionContext<'_>, class_names: &[String]) -> Vec<u64> {
    let mut ids = ctx
        .module
        .class_infos
        .iter()
        .filter(|(name, _)| {
            class_names
                .iter()
                .any(|declared| object_type_is_a(ctx, name, declared))
        })
        .map(|(_, class_info)| class_info.class_id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Collects the runtime class ids publishing a no-argument `__toString`.
///
/// The same candidate list the shared `Mixed` string ladder dispatches on, so a class this
/// accepts is exactly a class that ladder can convert.
fn stringable_class_ids(ctx: &FunctionContext<'_>) -> Result<Vec<u64>> {
    let mut ids = super::super::method_dispatch::mixed_method_candidates(ctx, "__toString", 1)?
        .into_iter()
        .map(|candidate| candidate.class_id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

/// Declared unions, including nullable types, spelled as PHP's weak-mode acceptance rules.
fn union_actions(
    ctx: &FunctionContext<'_>,
    members: &[PhpType],
    declared: &PhpType,
) -> Result<Option<Vec<TagAction>>> {
    let expanded = members
        .iter()
        .flat_map(expand_union_member)
        .collect::<Vec<_>>();
    let Some(kinds) = expanded
        .iter()
        .map(classify_member)
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    let has = |wanted: &MemberKind| kinds.iter().any(|kind| kind == wanted);
    let object_names = kinds
        .iter()
        .filter_map(|kind| match kind {
            MemberKind::Object(name) => Some(name.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    // Inline nullable-int storage holds an int or PHP null and nothing else, so every other
    // scalar source is narrowed to int here rather than accepted as-is.
    let boxed = declared.codegen_repr() == PhpType::Mixed;
    if !boxed {
        let narrow = |require_numeric_string: bool| {
            if has(&MemberKind::Int) {
                TagAction::CoerceScalar {
                    target: PhpType::Int,
                    require_numeric_string,
                    fallback: None,
                }
            } else {
                TagAction::Reject
            }
        };
        return Ok(Some(actions_for(|tag| match tag {
            0 if has(&MemberKind::Int) => TagAction::Accept,
            1 => narrow(true),
            2 | 3 => narrow(false),
            8 if has(&MemberKind::Null) => TagAction::Accept,
            _ => TagAction::Reject,
        })));
    }
    let class_ids = compatible_class_ids(ctx, &object_names);
    // A union with a `string` member accepts a Stringable object exactly like a plain `string`
    // property does, so the same `__toString` classes are admitted here.
    let stringable = if has(&MemberKind::Str) {
        stringable_class_ids(ctx)?
    } else {
        Vec::new()
    };
    let object_plan = ObjectPlan {
        any_class: has(&MemberKind::AnyObject),
        accepted: class_ids,
        stringable,
    };
    // PHP's weak-mode union resolution prefers an exact member, then coerces in int, float,
    // string, bool order among the members the union actually declares.
    let member_type = |kind: &MemberKind| match kind {
        MemberKind::Int => PhpType::Int,
        MemberKind::Float => PhpType::Float,
        MemberKind::Str => PhpType::Str,
        _ => PhpType::Bool,
    };
    let scalar_action = |exact: MemberKind, order: &[MemberKind]| -> TagAction {
        if has(&exact) {
            return TagAction::Accept;
        }
        let mut declared = order.iter().filter(|kind| has(kind));
        let Some(target) = declared.next().map(&member_type) else {
            return TagAction::Reject;
        };
        TagAction::CoerceScalar {
            target,
            require_numeric_string: false,
            fallback: declared.next().map(&member_type),
        }
    };
    let string_action = || union_string_action(&kinds);
    Ok(Some(actions_for(|tag| match tag {
        0 => scalar_action(
            MemberKind::Int,
            &[MemberKind::Float, MemberKind::Str, MemberKind::Bool],
        ),
        1 => string_action(),
        2 => scalar_action(
            MemberKind::Float,
            &[MemberKind::Int, MemberKind::Str, MemberKind::Bool],
        ),
        3 => scalar_action(
            MemberKind::Bool,
            &[MemberKind::Int, MemberKind::Float, MemberKind::Str],
        ),
        4 | 5 if has(&MemberKind::Array) => TagAction::Accept,
        6 if !object_plan.rejects_every_class() => TagAction::ObjectClasses(object_plan.clone()),
        8 if has(&MemberKind::Null) => TagAction::Accept,
        _ => TagAction::Reject,
    })))
}

/// Chooses the union member PHP resolves a runtime STRING against.
///
/// A declared `string` member takes the value as-is. Otherwise PHP coerces, and the order is
/// not simply the declaration order: a union declaring BOTH `int` and `float` does not prefer
/// `int`, it asks the string which of the two it SPELLS, so that verdict is deferred to the
/// runtime classifier instead of being decided here. With only one numeric member declared the
/// choice is static again, and a numeric string is still required for it; a union with neither
/// numeric member falls back to `bool`, which accepts every string.
fn union_string_action(kinds: &[MemberKind]) -> TagAction {
    let has = |wanted: &MemberKind| kinds.iter().any(|kind| kind == wanted);
    if has(&MemberKind::Str) {
        return TagAction::Accept;
    }
    if has(&MemberKind::Int) && has(&MemberKind::Float) {
        return TagAction::NumericStringIntOrFloat;
    }
    for (kind, target) in [
        (MemberKind::Int, PhpType::Int),
        (MemberKind::Float, PhpType::Float),
    ] {
        if has(&kind) {
            return TagAction::CoerceScalar {
                target,
                require_numeric_string: true,
                fallback: None,
            };
        }
    }
    if has(&MemberKind::Bool) {
        return TagAction::CoerceScalar {
            target: PhpType::Bool,
            require_numeric_string: false,
            fallback: None,
        };
    }
    TagAction::Reject
}

/// Expands one declared union member into the members the guard classifies.
///
/// `iterable` is PHP's shorthand for `array|Traversable`, so it contributes BOTH of those rather
/// than a kind of its own: `?iterable` then reuses the same array arm and the same by-class object
/// plan every other union already resolves with, instead of acquiring a parallel rule. The printed
/// spelling is composed from the DECLARED members, so expanding here does not change any message.
fn expand_union_member(member: &PhpType) -> Vec<PhpType> {
    match member {
        PhpType::Iterable => vec![
            PhpType::Array(Box::new(PhpType::Mixed)),
            PhpType::Object(TRAVERSABLE_INTERFACE.to_string()),
        ],
        other => vec![other.clone()],
    }
}

/// Classifies one declared union member, or answers `None` for a member this slice cannot
/// reason about. See `types_the_guard_does_not_model()` for what those are and why.
fn classify_member(member: &PhpType) -> Option<MemberKind> {
    match member {
        PhpType::Int => Some(MemberKind::Int),
        PhpType::Float => Some(MemberKind::Float),
        PhpType::Str => Some(MemberKind::Str),
        PhpType::Bool | PhpType::False => Some(MemberKind::Bool),
        PhpType::Void | PhpType::Never => Some(MemberKind::Null),
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some(MemberKind::Array),
        PhpType::Object(class_name) if class_name.is_empty() => Some(MemberKind::AnyObject),
        PhpType::Object(class_name) => Some(MemberKind::Object(class_name.clone())),
        _ => None,
    }
}

/// The declared property types this guard deliberately does not model, each with the reason.
///
/// Reached only by the tests, which is the point: the list is the recorded decision for every
/// type the guard answers `None` for, so the fallback is never a silent gap. Keeping it next to
/// `plan_tag_actions` means a new `PhpType` has to be classified in one of the two places.
#[cfg(test)]
fn types_the_guard_does_not_model() -> Vec<(PhpType, &'static str)> {
    vec![
        (
            PhpType::Callable,
            "`callable` is not a legal PHP property type and the checker already refuses it with \
             `Property C::$p cannot use type callable`, so there is no assignment to guard",
        ),
        (
            PhpType::Pointer(None),
            "`ptr<T>` is an elephc systems extension whose slot holds a raw address; its \
             declared-property contract refuses a runtime-shaped write at COMPILE time, and \
             turning that into a runtime coercion would hide the error the contract exists to \
             raise",
        ),
        (
            PhpType::Buffer(Box::new(PhpType::Int)),
            "`buffer<T>` holds a raw buffer header pointer with the same compile-time contract \
             as `ptr<T>`",
        ),
        (
            PhpType::Packed("Point".to_string()),
            "packed slots are fixed-layout fields written through `emit_packed_field_store`, \
             which the guard is never reached for; `slot.is_packed` excludes them earlier",
        ),
        (
            PhpType::Resource(None),
            "`resource` is not a declarable PHP property type; the variant exists for runtime \
             handles produced by builtins",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both arch-specific halves of the guard read the unboxed payload from the register the
    /// shared unbox contract names, on every supported target.
    ///
    /// Host execution runs only one architecture, and the register that carries the unbox
    /// payload is NOT the same one on both: reading the string pointer or the object class id
    /// out of the wrong register is silent on the host and wrong everywhere else. The numeric
    /// check needs no move on AArch64, where the payload already sits in the string-result
    /// pointer register, and exactly one move on x86_64, where it does not.
    #[test]
    fn the_guard_reads_the_unbox_payload_register_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(target);

            let mut emitter = Emitter::new(target);
            emit_numeric_string_check(&mut emitter, ".L_accept");
            let (string_ptr_reg, _) = abi::string_result_regs(&emitter);
            let numeric = emitter.output();
            assert!(
                numeric.contains("__rt_str_to_number"),
                "{name}: the numeric-string check must use the shared PHP numeric scanner"
            );
            let expected_moves = usize::from(string_ptr_reg != payload_reg);
            assert_eq!(
                numeric
                    .lines()
                    .filter(|line| line.trim().starts_with(&format!("mov {string_ptr_reg}, ")))
                    .count(),
                expected_moves,
                "{name}: expected {expected_moves} payload move(s) into {string_ptr_reg}, got `{numeric}`"
            );
            if expected_moves == 1 {
                assert!(
                    numeric.contains(&format!("mov {string_ptr_reg}, {payload_reg}")),
                    "{name}: the numeric check must read the string pointer from {payload_reg}"
                );
            }

            let mut emitter = Emitter::new(target);
            emit_accept_object_classes(&mut emitter, &[7], ".L_accept");
            let class_reg = abi::secondary_scratch_reg(&emitter);
            let classes = emitter.output();
            assert!(
                classes.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with(&format!("ldr {class_reg}, [{payload_reg}"))
                        || line.starts_with(&format!("mov {class_reg}, QWORD PTR [{payload_reg}"))
                }),
                "{name}: the class check must read the class id through {payload_reg}, got `{classes}`"
            );
        }
    }

    /// The `int|float` classifier arm honours the shared register contract on every target.
    ///
    /// `__rt_str_numeric_value` borrows the string as an ordinary pointer/length pair while the
    /// guard receives it in the Mixed unbox payload/high pair, and on AArch64 the length
    /// argument register IS the payload register, so the two moves have to happen in one order
    /// only. The classifier then answers in exactly the registers `__rt_mixed_from_value`
    /// consumes, which is what makes the selected member box the already-parsed value instead
    /// of rescanning the string, so no second numeric helper may appear here.
    #[test]
    fn the_int_or_float_classifier_arm_matches_the_shared_register_contract() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(target);
            let ptr_arg_reg = abi::int_arg_reg_name(target, 0);
            let len_arg_reg = abi::int_arg_reg_name(target, 1);

            let mut emitter = Emitter::new(target);
            emit_classify_numeric_string_and_box(&mut emitter, ".L_refuse");
            let (_, status_reg) = abi::string_result_regs(&emitter);
            let (_, high_arg_reg) = mixed_from_value_payload_regs(&emitter);
            let asm = emitter.output();

            let classify = asm.find("__rt_str_numeric_value").unwrap_or_else(|| {
                panic!("{name}: the int|float arm must use the shared numeric classifier")
            });
            let box_call = asm.find("__rt_mixed_from_value").unwrap_or_else(|| {
                panic!("{name}: the classified member must be boxed from the parsed value")
            });
            assert!(
                classify < box_call,
                "{name}: the classifier must run before the member is boxed, got `{asm}`"
            );
            assert!(
                !asm.contains("__rt_str_to_number") && !asm.contains("__rt_mixed_cast_"),
                "{name}: the parsed value must not be rescanned or recoerced, got `{asm}`"
            );

            // x86_64 already receives the payload in the pointer argument register, so the
            // pointer move only exists on the target where the two registers differ.
            let pointer_move = format!("mov {ptr_arg_reg}, {payload_reg}");
            let pointer_at = if ptr_arg_reg == payload_reg {
                assert!(
                    !asm.contains(&pointer_move),
                    "{name}: the string pointer is already in {ptr_arg_reg}, got `{asm}`"
                );
                0
            } else {
                asm.find(&pointer_move).unwrap_or_else(|| {
                    panic!("{name}: the classifier must borrow the pointer from {payload_reg}")
                })
            };
            if len_arg_reg != status_reg {
                let length_at = asm
                    .find(&format!("mov {len_arg_reg}, {status_reg}"))
                    .unwrap_or_else(|| {
                        panic!("{name}: the classifier must borrow the length from {status_reg}")
                    });
                assert!(
                    length_at > pointer_at && length_at < classify,
                    "{name}: the length move must not clobber {payload_reg} first, got `{asm}`"
                );
            }

            let refusal = if target.arch == Arch::AArch64 {
                format!("cbnz {status_reg}, .L_refuse")
            } else {
                "jne .L_refuse".to_string()
            };
            let refusal_at = asm.find(&refusal).unwrap_or_else(|| {
                panic!("{name}: a nonzero whole-string status must refuse, got `{asm}`")
            });
            assert!(
                classify < refusal_at && refusal_at < box_call,
                "{name}: the refusal must sit between the classifier and the box, got `{asm}`"
            );
            assert!(
                asm.lines().any(|line| {
                    let line = line.trim();
                    line.starts_with(&format!("mov {high_arg_reg}, #0"))
                        || line.starts_with(&format!("mov {high_arg_reg}, 0"))
                        || line.starts_with(&format!("xor {high_arg_reg}, {high_arg_reg}"))
                }),
                "{name}: the unused high payload word must be cleared, got `{asm}`"
            );
        }
    }

    /// Only a union declaring BOTH `int` and `float` defers the string member to run time.
    ///
    /// Every other shape keeps its static choice, so this pins the exact boundary the runtime
    /// classifier was added for and makes sure it did not swallow the single-member unions or
    /// the `string`/`bool` fallbacks that were already right.
    #[test]
    fn only_an_int_and_float_union_defers_the_string_member_to_run_time() {
        let deferred = |kinds: &[MemberKind]| {
            matches!(
                union_string_action(kinds),
                TagAction::NumericStringIntOrFloat
            )
        };
        assert!(deferred(&[MemberKind::Int, MemberKind::Float]));
        assert!(deferred(&[
            MemberKind::Float,
            MemberKind::Int,
            MemberKind::Bool
        ]));
        assert!(deferred(&[
            MemberKind::Int,
            MemberKind::Float,
            MemberKind::Null
        ]));
        // A declared `string` member wins before any numeric classification happens.
        assert!(matches!(
            union_string_action(&[MemberKind::Int, MemberKind::Float, MemberKind::Str]),
            TagAction::Accept
        ));
        // One numeric member leaves the destination static, and still demands a numeric string.
        for (kinds, expected) in [
            (vec![MemberKind::Int, MemberKind::Array], PhpType::Int),
            (vec![MemberKind::Float, MemberKind::Bool], PhpType::Float),
        ] {
            match union_string_action(&kinds) {
                TagAction::CoerceScalar {
                    target,
                    require_numeric_string,
                    fallback,
                } => {
                    assert_eq!(target, expected, "static member for {kinds:?}");
                    assert!(require_numeric_string, "numeric rule for {kinds:?}");
                    assert!(fallback.is_none(), "no fallback for {kinds:?}");
                }
                other => panic!("{kinds:?} must coerce statically, got {other:?}"),
            }
        }
        // With no numeric member declared `bool` accepts every string, numeric or not.
        match union_string_action(&[MemberKind::Bool, MemberKind::Array]) {
            TagAction::CoerceScalar {
                target,
                require_numeric_string,
                ..
            } => {
                assert_eq!(target, PhpType::Bool);
                assert!(!require_numeric_string);
            }
            other => panic!("a bool union must coerce, got {other:?}"),
        }
        assert!(matches!(
            union_string_action(&[MemberKind::Array, MemberKind::Null]),
            TagAction::Reject
        ));
    }

    /// Union spellings must match php-src's canonical order, not the declaration order.
    ///
    /// `zend_type_to_string` prints class names first and then walks a fixed built-in type
    /// mask, so a message joined in declaration order disagrees with reference PHP for almost
    /// every union. Each expectation here is the exact string PHP 8.5 puts in the `TypeError`.
    #[test]
    fn union_type_names_use_php_canonical_order() {
        let int = PhpType::Int;
        let float = PhpType::Float;
        let string = PhpType::Str;
        let boolean = PhpType::Bool;
        let null = PhpType::Void;
        let array = PhpType::php_array();
        let class = PhpType::Object("Box".to_string());
        let cases: Vec<(Vec<PhpType>, &str)> = vec![
            (vec![int.clone(), string.clone()], "string|int"),
            (vec![string.clone(), int.clone()], "string|int"),
            (vec![float.clone(), int.clone()], "int|float"),
            (
                vec![null.clone(), int.clone(), string.clone()],
                "string|int|null",
            ),
            (vec![string.clone(), null.clone()], "?string"),
            (vec![int.clone(), null.clone()], "?int"),
            (
                vec![
                    boolean.clone(),
                    string.clone(),
                    int.clone(),
                    float.clone(),
                    array.clone(),
                    null.clone(),
                ],
                "array|string|int|float|bool|null",
            ),
            (vec![array.clone(), boolean.clone()], "array|bool"),
            (vec![class.clone(), int.clone()], "Box|int"),
            (
                vec![PhpType::Iterable, string.clone()],
                "Traversable|array|string",
            ),
        ];
        for (members, expected) in cases {
            assert_eq!(
                php_union_type_name(&members),
                expected,
                "canonical spelling for {members:?}"
            );
        }
    }

    /// The bare `object` type and `mixed` print the way PHP prints them.
    #[test]
    fn bare_object_and_mixed_type_names_match_php() {
        assert_eq!(php_type_name(&PhpType::Object(String::new())), "object");
        assert_eq!(php_type_name(&PhpType::Mixed), "mixed");
        assert_eq!(php_type_name(&PhpType::Iterable), "Traversable|array");
    }

    /// Every declared type the guard declines to model is a recorded decision with a reason.
    ///
    /// The point of the assertion is not the list's contents but that a type reaching the
    /// `None` fallback has to be entered here first: a silent no-plan fallback for a type PHP
    /// does constrain would drop a write instead of refusing it.
    #[test]
    fn unmodelled_declared_types_are_recorded_with_a_reason() {
        for (php_type, reason) in types_the_guard_does_not_model() {
            assert!(
                !reason.is_empty(),
                "{php_type:?} must record why the guard does not model it"
            );
            assert!(
                classify_member(&php_type).is_none(),
                "{php_type:?} is recorded as unmodelled but classifies as a union member"
            );
        }
    }

    /// The types the guard DOES model must not silently fall into the unmodelled list.
    #[test]
    fn modelled_declared_types_are_not_recorded_as_unmodelled() {
        let unmodelled = types_the_guard_does_not_model();
        for php_type in [
            PhpType::Int,
            PhpType::Float,
            PhpType::Str,
            PhpType::Bool,
            PhpType::Mixed,
            PhpType::Iterable,
            PhpType::Object(String::new()),
            PhpType::Object("Box".to_string()),
            PhpType::php_array(),
        ] {
            assert!(
                !unmodelled.iter().any(|(other, _)| other == &php_type),
                "{php_type:?} is modelled by the guard and must not be listed as unmodelled"
            );
        }
    }
}
