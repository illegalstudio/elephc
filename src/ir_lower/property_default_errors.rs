//! Purpose:
//! Preserves deferred trait property-default errors and the deprecations php-src raises while
//! evaluating property defaults at class initialization boundaries.
//!
//! Called from:
//! - Object construction, static-property operations, and by-name initialization thunks.
//!
//! Key details:
//! - Metadata-only class operations must not evaluate property defaults.
//! - Constructor-promoted defaults are evaluated separately when their argument is omitted.
//! - php-src initializes a class (instance and static property defaults of the class and its
//!   ancestors) on its first instantiation or static-property access, once. A deprecated
//!   constant read in one of those defaults (`DateTime::RFC7231`) is reported there, behind
//!   the same per-read once flag every other initializer read uses.

use crate::span::Span;
use crate::parser::ast::StaticReceiver;
use crate::ir::{CmpPredicate, Immediate, Op, Ownership, PhpTypePredicate, RuntimeCallTarget, RuntimeFnId, Terminator};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;
use super::context::{LoweredValue, LoweringContext};

/// Reports the deprecated constant reads php-src raises while initializing `class_name`.
///
/// php-src initializes the ancestors first, from the root, each once. Initializing one class
/// evaluates the constants of its table that are still unevaluated, then its flattened
/// instance defaults (inherited slots included), then its own static defaults. A constant is
/// evaluated once per program, so its reads keep their own once flags; an inherited instance
/// default is evaluated again for every class initialized below its declarer, so the
/// property reads report directly, inside the class's own initialization flag.
pub(crate) fn emit_initialization_deprecations(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    trigger: Span,
) {
    use super::initializer_evaluation::{InitializerFrame, LineSource};
    use super::initializer_reads as reads;
    let class_name = class_name.trim_start_matches('\\');
    // php-src declares an anonymous class when its `new` first runs (`ZEND_DECLARE_ANON_CLASS`),
    // so that is where it binds and copies its parent's and traits' defaults, before it
    // initializes.
    if crate::parser::is_anonymous_class_name(class_name) && ctx.classes.contains_key(class_name) {
        let declared = super::initializer_evaluation::flag_label("_anonymous_class_declared", class_name);
        super::initializer_evaluation::emit_unless_any_flag_set(ctx, std::slice::from_ref(&declared), trigger, |ctx| {
            super::initializer_evaluation::emit_flag_mark(ctx, &declared, trigger);
            emit_class_statement_deprecations(ctx, class_name, trigger);
        });
    }
    for owner in reads::class_initialization_chain(ctx, class_name) {
        if !class_initialization_reports(ctx, &owner) {
            continue;
        }
        let Some(label) = initialization_label(ctx, &owner) else { continue };
        let defaults = reads::class_own_initialization_defaults(ctx, &owner);
        super::initializer_evaluation::emit_until_success(ctx, &label, trigger, |ctx| {
            reads::emit_constant_table_evaluation(ctx, &owner, LineSource::Static(trigger));
            for slot in &defaults {
                let lone = reads::lone_global_read(ctx, Some(&slot.declaring), &slot.default);
                let frame = InitializerFrame::top(true, LineSource::Static(trigger), lone, None);
                // An instance default copied already evaluated (see `copied_slot_labels`) is
                // not evaluated again.
                let copied = if slot.is_static { Vec::new() } else { copied_slot_labels(ctx, &owner, slot) };
                super::initializer_evaluation::emit_unless_any_flag_set(ctx, &copied, trigger, |ctx| {
                    reads::emit_initializer_reads(ctx, &slot.default, Some(&slot.declaring), &frame);
                });
            }
        });
    }
}

/// Returns the program-wide flag symbol set when `class_name`'s initialization ran.
pub(crate) fn initialization_label(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Option<String> {
    let class_id = ctx.classes.get(class_name.trim_start_matches('\\'))?.class_id;
    Some(format!("_class_initialization_deprecations_{class_id}"))
}

/// Reports whether initializing `class_name` itself (not its ancestors) can raise a
/// deprecation: a constant of its table or one of its property defaults reads a deprecated
/// constant, directly or through user class constants.
pub(crate) fn class_initialization_reports(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    use super::initializer_reads as reads;
    reads::constant_table_reports(ctx, class_name) || reads::class_own_initialization_defaults(ctx, class_name)
        .iter()
        .any(|slot| reads::reaches_deprecated_read_in(ctx, Some(&slot.declaring), &slot.default))
}

/// Returns the flags that tell, at run time, that `class_name`'s copy of instance slot `slot`
/// was taken already evaluated. php-src copies a parent's default table when a class binds at
/// its statement, and a trait's when a class or trait binding it does: a slot is evaluated in
/// the copy when the class it was copied from had been initialized by then, at any step from
/// the declaring class down to `class_name`, or when the trait it was imported from had been.
fn copied_slot_labels(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    slot: &super::initializer_reads::InitializationDefault,
) -> Vec<String> {
    let chain = super::initializer_reads::class_initialization_chain(ctx, class_name);
    let mut labels = Vec::new();
    if let Some(position) = chain.iter().position(|class| class.eq_ignore_ascii_case(&slot.declaring)) {
        for copier in &chain[position + 1..] {
            if super::initializer_reads::is_runtime_bound(copier) {
                labels.push(copied_parent_label(copier));
            }
        }
    }
    let mut holder = slot.declaring.clone();
    let mut seen = Vec::new();
    while let Some(origin) = super::initializer_reads::property_trait_origin(&holder, &slot.name) {
        if seen.contains(&origin) {
            break;
        }
        labels.push(copied_trait_label(&holder, &origin));
        seen.push(origin.clone());
        holder = origin;
    }
    labels
}

/// Returns the flag set when class `class_name`, bound at its statement, copied its parent's
/// default table after the parent was initialized.
pub(crate) fn copied_parent_label(class_name: &str) -> String {
    super::initializer_evaluation::flag_label("_class_binding_copied_parent", &class_name.to_ascii_lowercase())
}

/// Returns the flag set when class-like `holder` copied trait `trait_name`'s defaults after the
/// trait was initialized.
pub(crate) fn copied_trait_label(holder: &str, trait_name: &str) -> String {
    super::initializer_evaluation::flag_label(
        "_class_binding_copied_trait",
        &format!("{}::{}", holder.to_ascii_lowercase(), trait_name.to_ascii_lowercase()),
    )
}

/// Returns the flag set once trait `trait_name` was initialized (by `get_class_vars()`).
pub(crate) fn trait_initialization_label(trait_name: &str) -> String {
    super::initializer_evaluation::flag_label(
        "_class_initialization_deprecations_trait",
        &trait_name.trim_start_matches('\\').to_ascii_lowercase(),
    )
}

/// Emits what binding class or trait `name` at its statement records: whether its parent and
/// each trait it uses were initialized by then, and, for each constant it copies from a trait,
/// which reads the trait's copy already evaluated.
fn emit_binding_snapshot(ctx: &mut LoweringContext<'_, '_>, name: &str, statement: Span) {
    use super::initializer_evaluation::{emit_flag_mark, emit_if_flag_set};
    if !super::initializer_reads::is_runtime_bound(name) {
        return;
    }
    if let Some(parent) = ctx.classes.get(name).and_then(|class| class.parent.clone()) {
        if let Some(parent_label) = initialization_label(ctx, &parent) {
            let copied = copied_parent_label(name);
            emit_if_flag_set(ctx, &parent_label, statement, |ctx| emit_flag_mark(ctx, &copied, statement));
        }
    }
    for trait_name in super::initializer_reads::direct_trait_uses(name) {
        let copied = copied_trait_label(name, &trait_name);
        emit_if_flag_set(ctx, &trait_initialization_label(&trait_name), statement, |ctx| {
            emit_flag_mark(ctx, &copied, statement)
        });
        for (constant, value) in super::initializer_reads::trait_constants(&trait_name) {
            let from = super::initializer_evaluation::class_constant_once_key(&trait_name, &constant);
            let to = super::initializer_evaluation::class_constant_once_key(name, &constant);
            for span in super::initializer_reads::direct_read_spans(&value) {
                let to_label = super::initializer_evaluation::once_label(&to, span);
                emit_if_flag_set(ctx, &super::initializer_evaluation::once_label(&from, span), statement, |ctx| {
                    emit_flag_mark(ctx, &to_label, statement)
                });
            }
        }
    }
}

/// Raises the deprecations php-src reports when execution reaches the class (or trait)
/// statement `class_name`: the defaults it evaluates to check that a property compatibly
/// redeclares one of a used trait (the redeclaring default, then the trait's).
pub(crate) fn emit_class_statement_deprecations(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    statement: Span,
) {
    use super::initializer_evaluation::{InitializerFrame, LineSource};
    let class_name = class_name.trim_start_matches('\\');
    let defaults = ctx
        .classes
        .get(class_name)
        .map(|class| class.trait_collision_defaults.clone())
        .unwrap_or_else(|| super::initializer_reads::trait_statement_collision_defaults(class_name));
    for collision in &defaults {
        let default = &collision.default;
        let lone = super::initializer_reads::lone_global_read(ctx, Some(class_name), default);
        let frame = InitializerFrame::top(true, LineSource::Static(statement), lone, None);
        // A trait initialized before this statement holds its default already evaluated.
        let initialized: Vec<String> = collision.trait_name.iter().map(|name| trait_initialization_label(name)).collect();
        super::initializer_evaluation::emit_unless_any_flag_set(ctx, &initialized, statement, |ctx| {
            super::initializer_reads::emit_initializer_reads(ctx, default, Some(class_name), &frame);
        });
    }
    emit_binding_snapshot(ctx, class_name, statement);
}

/// Reports whether initializing `class_name` raises a deferred error or a deprecation.
pub(crate) fn has_initialization_effect(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    ctx.classes
        .get(class_name)
        .is_some_and(|class| class.deferred_property_default_error.is_some())
        || super::initializer_reads::class_initialization_chain(ctx, class_name)
            .iter()
            .any(|owner| class_initialization_reports(ctx, owner))
}

/// Raises the recorded initialization error without allocating the invalid class, after the
/// initialization deprecations php-src reports first.
pub(super) fn for_class(
    ctx: &mut LoweringContext<'_, '_>, class_name: &str, span: Span,
) -> Option<LoweredValue> {
    emit_initialization_deprecations(ctx, class_name.trim_start_matches('\\'), span);
    let message = ctx.classes.get(class_name.trim_start_matches('\\'))
        .and_then(|class| class.deferred_property_default_error.clone())?;
    Some(super::stmt::lower_throw_access_error_expr(ctx, &message, span))
}

/// Raises the recorded initialization error while preserving an expression's expected type.
pub(super) fn for_class_as(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    span: Span,
    php_type: PhpType,
) -> Option<LoweredValue> {
    emit_initialization_deprecations(ctx, class_name.trim_start_matches('\\'), span);
    let message = ctx
        .classes
        .get(class_name.trim_start_matches('\\'))
        .and_then(|class| class.deferred_property_default_error.clone())?;
    Some(super::stmt::lower_throw_access_error_expr_as(
        ctx, &message, span, php_type,
    ))
}

/// Applies the class-wide check even when the accessed static property has a valid default.
pub(super) fn for_receiver(
    ctx: &mut LoweringContext<'_, '_>, receiver: &StaticReceiver, span: Span,
) -> Option<LoweredValue> {
    let name = super::expr::static_receiver_class_name(ctx, receiver)?;
    for_class(ctx, &name, span)
}

/// Checks a runtime class name without creating an owning dispatch temporary: raises the
/// initialization deprecations and deferred error of whichever class the name matches.
pub(super) fn for_dynamic_class(ctx: &mut LoweringContext<'_, '_>, name: LoweredValue, span: Span) {
    dispatch_dynamic_class(ctx, name, span, &has_initialization_effect, &|ctx, class, span| {
        for_class(ctx, class, span);
    });
}

/// Raises what `ReflectionClass::getDefaultProperties()` reports on every call for a runtime
/// class name, besides initializing the class: php-src evaluates every static property
/// default again for the returned array, so their deprecated reads report on each call.
pub(crate) fn for_reflection_default_properties(
    ctx: &mut LoweringContext<'_, '_>,
    name: LoweredValue,
    span: Span,
) {
    dispatch_dynamic_class(ctx, name, span, &|ctx, class| !replayed_static_defaults(ctx, class).is_empty(),
        &|ctx, class, span| emit_static_default_replay(ctx, &replayed_static_defaults(ctx, class), span));
}

/// Returns the static defaults of `class_name` that read a deprecated constant and that
/// `getDefaultProperties()` evaluates again on every call.
fn replayed_static_defaults(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<(String, Expr)> {
    super::initializer_reads::class_static_default_slots(ctx, class_name)
        .into_iter()
        .filter(|(declaring, default)| {
            super::initializer_reads::reaches_deprecated_read_in(ctx, Some(declaring), default)
        })
        .collect()
}

/// Emits the deprecations of evaluating each of `defaults` again (a copy php-src evaluates on
/// every call), at `span`.
pub(crate) fn emit_static_default_replay(
    ctx: &mut LoweringContext<'_, '_>,
    defaults: &[(String, Expr)],
    span: Span,
) {
    use super::initializer_evaluation::{InitializerFrame, LineSource};
    for (declaring, default) in defaults {
        let lone = super::initializer_reads::lone_global_read(ctx, Some(declaring), default);
        let frame = InitializerFrame::top(true, LineSource::Static(span), lone, None);
        super::initializer_reads::emit_initializer_reads(ctx, default, Some(declaring), &frame);
    }
}

/// Compares a runtime class name against every class `wants` selects and runs `apply` for the
/// one it names (case-insensitively), boxed `mixed` names included.
pub(crate) fn dispatch_dynamic_class(
    ctx: &mut LoweringContext<'_, '_>,
    name: LoweredValue,
    span: Span,
    wants: &dyn Fn(&LoweringContext<'_, '_>, &str) -> bool,
    apply: &dyn Fn(&mut LoweringContext<'_, '_>, &str, Span),
) {
    let mut classes: Vec<String> = ctx.classes.keys().filter(|class| wants(ctx, class)).cloned().collect();
    classes.sort();
    dispatch_string(ctx, name, span, &classes, true, &|ctx, class| apply(ctx, class, span));
}

/// Compares a runtime string (a boxed `mixed` string included) against each of `options` and
/// runs `apply` for the one it equals, ignoring ASCII case when `case_insensitive` is set.
pub(crate) fn dispatch_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
    options: &[String],
    case_insensitive: bool,
    apply: &dyn Fn(&mut LoweringContext<'_, '_>, &str),
) {
    if options.is_empty() { return; }
    let ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        let string = ctx.emit_value(Op::TypePredicate, vec![value.value],
            Some(Immediate::TypePredicate(PhpTypePredicate::String)), PhpType::Bool,
            Op::TypePredicate.default_effects(), Some(span));
        let check = ctx.builder.create_named_block("class_default.string", Vec::new());
        let done = ctx.builder.create_named_block("class_default.nonstring", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: string.value,
            then_target: check, then_args: Vec::new(), else_target: done, else_args: Vec::new() });
        ctx.builder.position_at_end(check);
        let unboxed = ctx.emit_value(Op::MixedUnbox, vec![value.value], None, PhpType::Str,
            Op::MixedUnbox.default_effects(), Some(span));
        ctx.builder.set_value_ownership(unboxed.value, Ownership::Borrowed);
        dispatch_string(ctx, unboxed, span, options, case_insensitive, apply);
        ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
        ctx.builder.position_at_end(done);
        return;
    }
    if ty != PhpType::Str { return; }
    let compare = if case_insensitive { RuntimeFnId::Strcasecmp } else { RuntimeFnId::Strcmp };
    for option in options {
        let literal = super::expr::lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(option.clone()), span));
        let comparison = ctx.emit_value(Op::RuntimeCall, vec![value.value, literal.value],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(compare))),
            PhpType::Int, compare.effects(), Some(span));
        let zero = super::expr::lower_expr(ctx, &Expr::new(ExprKind::IntLiteral(0), span));
        let matches = ctx.emit_value(Op::ICmp, vec![comparison.value, zero.value],
            Some(Immediate::CmpPredicate(CmpPredicate::Eq)), PhpType::Bool,
            Op::ICmp.default_effects(), Some(span));
        let error = ctx.builder.create_named_block("class_default.error", Vec::new());
        let next = ctx.builder.create_named_block("class_default.next", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: matches.value,
            then_target: error, then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(error);
        apply(ctx, option);
        ctx.builder.terminate(Terminator::Br { target: next, args: Vec::new() });
        ctx.builder.position_at_end(next);
    }
}
