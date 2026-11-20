//! Purpose:
//! Evaluates eval-declared class constants and enum cases when php-src first evaluates them
//! (a constant's first read, a class or enum initialization, the reflection getters), and
//! stores the result, so each is evaluated once, with its notices and errors, at that moment.
//!
//! Called from:
//! - The class-constant fetch, class and enum initialization, `ReflectionClassConstant`,
//!   `ReflectionClass::getConstant[s]()`, `ReflectionEnumBackedCase::getBackingValue()`, and
//!   the backed enum `cases()`, `from()` and `tryFrom()`.
//!
//! Key details:
//! - Nothing is evaluated at declaration. An initializer that throws (an undefined constant)
//!   throws at the first evaluation and again at the next one, as php-src leaves the constant
//!   unevaluated.
//! - A silent evaluation (eval materializing a value php-src does not compute at that point,
//!   such as a reflector being built) has no lasting effect: an unevaluated constant is
//!   evaluated without being stored, and a backed enum case that does not exist yet is not
//!   created; the materialization reports it unavailable (`UnsupportedConstruct`).
//! - A backed enum's initialization evaluates its whole constant table, then checks every case
//!   value against the backing type and the other cases, as `zend_enum_build_backed_enum_table`.
//! - A class's constant table, in php-src's order: its own constants (trait constants after
//!   them, an enum's cases and constants in declaration order), the parent's table, then each
//!   implemented interface's own constants and its parents'.

use super::*;

/// Makes sure php-src's first evaluation of class constant or enum case `holder::name` has
/// happened: it is evaluated now, once, with its notices, and stored.
pub(in crate::interpreter) fn eval_class_constant_first_evaluation(
    holder: &str,
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if eval_is_enum_case(holder, name, context) {
        return eval_enum_case_object(holder, name, context, values).map(|_| ());
    }
    eval_ensure_class_constant(holder, name, context, values)
}

/// Evaluates eval class constant `holder::name` and stores its value, unless it already was.
pub(in crate::interpreter) fn eval_ensure_class_constant(
    holder: &str,
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(value) = eval_class_constant_value(holder, name, context, values)? {
        if !value.is_borrowed() {
            values.release(value)?;
        }
    }
    Ok(())
}

/// Returns the value of eval class constant `holder::name`, or `None` when there is none:
/// the stored value (borrowed), or the first evaluation, which is stored. php-src evaluates a
/// class constant once and caches it. In a silent evaluation an unevaluated constant is
/// evaluated without being stored, and the caller owns the returned value.
pub(in crate::interpreter) fn eval_class_constant_value(
    holder: &str,
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some((owner, constant)) = context.class_constant(holder, name) else {
        return Ok(None);
    };
    if let Some(cell) = context.class_constant_cell(&owner, constant.name()) {
        return Ok(Some(cell));
    }
    let mut constant_scope = ElephcEvalScope::new();
    let value = eval_class_like_member_default(
        &owner,
        constant.trait_origin(),
        constant.value(),
        context,
        &mut constant_scope,
        values,
    )?;
    // The stored cell owns its value; a borrowed result (another constant's cell) is retained.
    let value = if value.is_borrowed() { values.retain(value)? } else { value };
    if context.deprecations_silenced() {
        return Ok(Some(value));
    }
    if let Some(replaced) = context.set_class_constant_cell(&owner, constant.name(), value) {
        values.release(replaced)?;
    }
    Ok(context.class_constant_cell(&owner, constant.name()))
}

/// Reports whether `holder::name` names a case of eval enum `holder`.
pub(in crate::interpreter) fn eval_is_enum_case(holder: &str, name: &str, context: &ElephcEvalContext) -> bool {
    context.enum_decl(holder).is_some_and(|enum_decl| enum_decl.case(name).is_some())
}

/// Returns eval enum case `enum_name::case_name`'s singleton, or `None` when there is no such
/// case. php-src creates a case object when it first evaluates the case constant, evaluating a
/// backed case's value expression then. In a silent evaluation a backed case that does not
/// exist yet is unavailable (`UnsupportedConstruct`): creating it would evaluate its value
/// before php-src does.
pub(in crate::interpreter) fn eval_enum_case_object(
    enum_name: &str,
    case_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(case) = context.enum_case(enum_name, case_name) {
        return Ok(Some(case));
    }
    let Some(enum_decl) = context.enum_decl(enum_name).cloned() else {
        return Ok(None);
    };
    let Some(case) = enum_decl.case(case_name).cloned() else {
        return Ok(None);
    };
    let backing_value = match case.value() {
        None => None,
        Some(_) if context.deprecations_silenced() => return Err(EvalStatus::UnsupportedConstruct),
        Some(value_expr) => {
            let mut case_scope = ElephcEvalScope::new();
            let value = eval_class_like_member_default(enum_decl.name(), None, value_expr, context, &mut case_scope, values)?;
            Some(if value.is_borrowed() { values.retain(value)? } else { value })
        }
    };
    initialize_eval_enum_case(&enum_decl, &case, backing_value, context, values)?;
    Ok(context.enum_case(enum_name, case_name))
}

/// Throws php-src's `Error` for a class constant or enum case whose evaluation reads itself,
/// naming the class as `display_class` (php-src names it as the reading node spells it).
pub(in crate::interpreter) fn eval_throw_self_referencing_constant<T>(
    display_class: &str,
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!("Cannot declare self-referencing constant {}::{}", display_class.trim_start_matches('\\'), name),
        context,
        values,
    )
}

/// Returns class constant or enum case `holder::name` for a fetch written `written::name`.
/// Inside a constant expression php-src fetches through `zend_get_class_constant_ex`, which
/// marks an unevaluated constant visited while evaluating it and throws the self-reference
/// `Error` (naming the class as the fetch spells it) when it reaches a visited one; a fetch in
/// ordinary code, a class initialization and reflection evaluate without marking.
pub(in crate::interpreter) fn eval_fetch_class_constant(
    written: &str,
    holder: &str,
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let is_case = eval_is_enum_case(holder, name, context);
    let evaluated = if is_case {
        context.enum_case(holder, name).is_some()
    } else {
        context
            .class_constant(holder, name)
            .is_some_and(|(owner, constant)| context.class_constant_cell(&owner, constant.name()).is_some())
    };
    let marks = !evaluated && context.in_constant_expression();
    if marks && !context.begin_class_constant_evaluation(holder, name) {
        return eval_throw_self_referencing_constant(written, name, context, values);
    }
    let result = if is_case {
        eval_enum_case_object(holder, name, context, values)
    } else {
        eval_class_constant_value(holder, name, context, values)
    };
    if marks {
        context.end_class_constant_evaluation(holder, name);
    }
    result
}

/// One silent class-constant probe before Reflection decides how to represent an unavailable value.
pub(in crate::interpreter) enum EvalClassConstantPreview {
    /// A usable constant value and whether the caller owns its runtime cell.
    Value(RuntimeCellHandle, bool),
    /// The requested metadata did not produce a value.
    Missing,
    /// Metadata exists, but cannot be materialized before normal evaluation.
    Unavailable,
    /// Evaluation threw and the preview consumed the temporary Throwable.
    Failed,
}

/// Probes a class constant without publishing its value or conflating failure with PHP null.
pub(in crate::interpreter) fn eval_class_constant_preview_state(
    holder: &str,
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalClassConstantPreview, EvalStatus> {
    context.begin_deprecation_silence();
    let value = if eval_is_enum_case(holder, name, context) {
        eval_enum_case_object(holder, name, context, values)
    } else {
        eval_class_constant_value(holder, name, context, values)
    };
    context.end_deprecation_silence();
    match value {
        Ok(Some(value)) => Ok(EvalClassConstantPreview::Value(value, !value.is_borrowed())),
        Ok(None) => Ok(EvalClassConstantPreview::Missing),
        Err(EvalStatus::UnsupportedConstruct) => Ok(EvalClassConstantPreview::Unavailable),
        Err(EvalStatus::UncaughtThrowable) => {
            if let Some(throwable) = context.take_pending_throw() {
                eval_release_value(context, values, throwable)?;
            }
            Ok(EvalClassConstantPreview::Failed)
        }
        Err(status) => Err(status),
    }
}

/// Returns a reflector preview, using null when silent materialization is unavailable or fails.
pub(in crate::interpreter) fn eval_class_constant_preview(
    holder: &str,
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<(RuntimeCellHandle, bool)>, EvalStatus> {
    match eval_class_constant_preview_state(holder, name, context, values)? {
        EvalClassConstantPreview::Value(value, owned) => Ok(Some((value, owned))),
        EvalClassConstantPreview::Missing => Ok(None),
        EvalClassConstantPreview::Unavailable | EvalClassConstantPreview::Failed => {
            Ok(Some((values.null()?, true)))
        }
    }
}

/// Maps a silent evaluation's result to `Some(value)`, or `None` when the value is unavailable
/// before php-src evaluates it (an unavailable backed case, or a Throwable, which is dropped).
pub(in crate::interpreter) fn eval_preview_outcome<T>(
    result: Result<T, EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<T>, EvalStatus> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(EvalStatus::UnsupportedConstruct) => Ok(None),
        Err(EvalStatus::UncaughtThrowable) => {
            if let Some(throwable) = context.take_pending_throw() {
                eval_release_value(context, values, throwable)?;
            }
            Ok(None)
        }
        Err(status) => Err(status),
    }
}

/// Checks a backed enum's case values as `zend_enum_build_backed_enum_table` does when the enum
/// initializes: each value, in table order, must have the backing type and differ from the
/// values before it.
pub(in crate::interpreter) fn eval_validate_backed_enum_cases(
    enum_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let Some(enum_decl) = context.enum_decl(enum_name) else { return Ok(()) };
    let Some(backing_type) = enum_decl.backing_type() else { return Ok(()) };
    let display = enum_decl.name().trim_start_matches('\\').to_string();
    let case_names: Vec<String> = eval_class_constant_table(enum_name, context)
        .into_iter()
        .filter(|(holder, name)| eval_is_enum_case(holder, name, context))
        .map(|(_, name)| name)
        .collect();
    let (expected_tag, expected_name) = match backing_type {
        EvalEnumBackingType::Int => (EVAL_TAG_INT, "int"),
        EvalEnumBackingType::String => (EVAL_TAG_STRING, "string"),
    };
    let mut seen: Vec<(String, RuntimeCellHandle)> = Vec::new();
    for case_name in case_names {
        let Some(value) = context.enum_case_value(enum_name, &case_name) else { continue };
        let tag = values.type_tag(value)?;
        if tag != expected_tag {
            let actual = match tag {
                EVAL_TAG_NULL => "null",
                EVAL_TAG_BOOL => "bool",
                EVAL_TAG_INT => "int",
                EVAL_TAG_FLOAT => "float",
                EVAL_TAG_STRING => "string",
                EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
                EVAL_TAG_RESOURCE => "resource",
                _ => "object",
            };
            return eval_throw_type_error(
                &format!("Enum case type {actual} does not match enum backing type {expected_name}"),
                context,
                values,
            );
        }
        for (existing, existing_value) in &seen {
            let equal = values.compare(EvalBinOp::StrictEq, value, *existing_value)?;
            if values.truthy(equal)? {
                return eval_throw_error(
                    &format!("Duplicate value in enum {display} for cases {existing} and {case_name}"),
                    context,
                    values,
                );
            }
        }
        seen.push((case_name, value));
    }
    Ok(())
}

/// Evaluates, in table order, every constant of `class_name`'s table php-src has not
/// evaluated yet: what a class initialization and `ReflectionClass::getConstant[s]()` do.
pub(in crate::interpreter) fn eval_class_constant_table_evaluation(
    class_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (holder, name) in eval_class_constant_table(class_name, context) {
        eval_class_constant_first_evaluation(&holder, &name, context, values)?;
    }
    Ok(())
}

/// Returns `class_name`'s constant table in php-src's order, each entry with the class-like
/// that holds it. A name appears once, at its first position.
pub(in crate::interpreter) fn eval_class_constant_table(
    class_name: &str,
    context: &ElephcEvalContext,
) -> Vec<(String, String)> {
    let mut table = Vec::new();
    push_eval_class_constant_table(class_name, context, &mut table, 0);
    table
}

/// Recursive body of [`eval_class_constant_table`]; `depth` bounds a malformed cycle.
fn push_eval_class_constant_table(
    class_name: &str,
    context: &ElephcEvalContext,
    table: &mut Vec<(String, String)>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    if let Some(enum_decl) = context.enum_decl(class_name) {
        let name = enum_decl.name().to_string();
        let order: Vec<String> = if enum_decl.member_order().is_empty() {
            enum_decl
                .cases()
                .iter()
                .map(|case| case.name().to_string())
                .chain(enum_decl.constants().iter().map(|constant| constant.name().to_string()))
                .collect()
        } else {
            enum_decl.member_order().to_vec()
        };
        let interfaces = enum_decl.interfaces().to_vec();
        for member in order {
            push_eval_constant_entry(table, &name, &member);
        }
        for interface in interfaces {
            push_eval_interface_constant_table(&interface, context, table, depth + 1);
        }
        return;
    }
    if let Some(class) = context.class(class_name) {
        let name = class.name().to_string();
        // The class's own constants come before the ones its traits import.
        let declared = class.constants().iter().filter(|constant| constant.trait_origin().is_none());
        let imported = class.constants().iter().filter(|constant| constant.trait_origin().is_some());
        let own: Vec<String> = declared.chain(imported).map(|constant| constant.name().to_string()).collect();
        let parent = class.parent().map(str::to_string);
        let interfaces = class.interfaces().to_vec();
        for constant in own {
            push_eval_constant_entry(table, &name, &constant);
        }
        if let Some(parent) = parent {
            push_eval_class_constant_table(&parent, context, table, depth + 1);
        }
        for interface in interfaces {
            push_eval_interface_constant_table(&interface, context, table, depth + 1);
        }
        return;
    }
    push_eval_interface_constant_table(class_name, context, table, depth + 1);
}

/// Appends an eval interface's table: its own constants, then its parents'.
fn push_eval_interface_constant_table(
    interface_name: &str,
    context: &ElephcEvalContext,
    table: &mut Vec<(String, String)>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    let Some(interface) = context.interface(interface_name) else { return };
    let name = interface.name().to_string();
    let own: Vec<String> = interface.constants().iter().map(|constant| constant.name().to_string()).collect();
    let parents = interface.parents().to_vec();
    for constant in own {
        push_eval_constant_entry(table, &name, &constant);
    }
    for parent in parents {
        push_eval_interface_constant_table(&parent, context, table, depth + 1);
    }
}

/// Appends `holder::name` unless the table already holds a constant of that name.
fn push_eval_constant_entry(table: &mut Vec<(String, String)>, holder: &str, name: &str) {
    if !table.iter().any(|(_, existing)| existing == name) {
        table.push((holder.to_string(), name.to_string()));
    }
}

/// Reports whether `class_name` is an eval-declared backed enum. php-src initializes a backed
/// enum (every constant and case value) on the first fetch of any of its unevaluated constants,
/// because the case objects need the enum's value table.
pub(in crate::interpreter) fn eval_is_backed_enum(class_name: &str, context: &ElephcEvalContext) -> bool {
    context.enum_decl(class_name).is_some_and(|enum_decl| enum_decl.backing_type().is_some())
}

/// Answers, from `eval()` code, the reflection getters whose value php-src evaluates on the
/// call: `ReflectionClassConstant::getValue()` and `ReflectionEnumBackedCase::getBackingValue()`
/// are the reflected constant's first evaluation, and `ReflectionProperty::getDefaultValue()`
/// / `ReflectionParameter::getDefaultValue()` evaluate a fresh copy of the default on every
/// call (an instance property default only until its class is initialized, when it is the
/// class's evaluated default). `ReflectionClass::getConstant[s]()` evaluates every constant of
/// the table first, then its own handler answers. `None` leaves the call to the other handlers
/// and the compiled getter (an AOT member, a member without a default, a default eval leaves
/// to its factory).
pub(in crate::interpreter) fn eval_reflection_evaluating_getter_result(
    object: RuntimeCellHandle,
    identity: u64,
    method_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match method_name.to_ascii_lowercase().as_str() {
        method @ ("getvalue" | "getbackingvalue") => {
            let reflected = context
                .eval_reflection_class_constant(identity)
                .map(|(holder, name, _)| (holder.to_string(), name.to_string()));
            let Some((holder, name)) = reflected else { return Ok(None) };
            if eval_is_enum_case(&holder, &name, context) {
                let Some(case) = eval_enum_case_object(&holder, &name, context, values)? else { return Ok(None) };
                if method == "getvalue" {
                    return Ok(Some(case));
                }
                return Ok(context.enum_case_value(&holder, &name));
            }
            if method == "getbackingvalue" {
                return Ok(None);
            }
            let Some((owner, constant)) = context.class_constant(&holder, &name) else { return Ok(None) };
            eval_class_constant_value(&owner, constant.name(), context, values)
        }
        "getconstant" | "getconstants" => {
            if let Some(class_name) = context.eval_reflection_class_name(identity).map(str::to_string) {
                eval_class_constant_table_evaluation(&class_name, context, values)?;
            }
            Ok(None)
        }
        "getdefaultvalue" => {
            let reflected = context
                .eval_reflection_property(identity)
                .map(|(class_name, property)| (class_name.to_string(), property.to_string()));
            if let Some((class_name, property)) = reflected {
                return eval_reflected_property_default(&class_name, &property, context, values);
            }
            let Some(reflected) = context.reflected_parameter_default(identity) else {
                return Ok(None);
            };
            // An identity outlives its object, so the entry counts only for a live parameter
            // reflector.
            let class_name = values.object_class_name(object)?;
            let is_parameter = values.string_bytes(class_name);
            values.release(class_name)?;
            if !is_parameter?.eq_ignore_ascii_case(b"ReflectionParameter") {
                return Ok(None);
            }
            eval_reflected_parameter_default(&reflected, context, values)
        }
        _ => Ok(None),
    }
}

/// Stores reflected private slots on `object` from inside class scope `scope`.
pub(in crate::interpreter) fn eval_set_reflector_slots(
    object: RuntimeCellHandle,
    scope: &str,
    slots: &[(&str, RuntimeCellHandle)],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    context.push_class_scope(scope.to_string());
    context.push_called_class_scope(scope.to_string());
    let mut result = Ok(());
    for (slot, value) in slots {
        result = values.property_set(object, slot, *value);
        if result.is_err() {
            break;
        }
    }
    context.pop_called_class_scope();
    context.pop_class_scope();
    result
}

/// Evaluates `class_name::$property`'s default for `ReflectionProperty::getDefaultValue()`:
/// the class's evaluated default once it is initialized (an instance property), otherwise a
/// fresh evaluation. `None` when the property has no eval default.
fn eval_reflected_property_default(
    class_name: &str,
    property_name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(class) = context.class(class_name) else { return Ok(None) };
    let declaring = class.name().to_string();
    let Some(property) = class
        .properties()
        .iter()
        .find(|property| property.name() == property_name && property.default().is_some())
        .cloned()
    else {
        return Ok(None);
    };
    // php-src answers an instance default from the class's evaluated default table once the
    // class is initialized; a static default is copied from the declaration on every call.
    if !property.is_static() && context.class_initialization_ran(&declaring) {
        let storage_name = eval_instance_property_storage_name(&declaring, &property);
        if let Some(cell) = context.instance_default_cell(&declaring, &storage_name) {
            return Ok(Some(cell));
        }
    }
    let Some(default) = property.default() else { return Ok(None) };
    let mut default_scope = ElephcEvalScope::new();
    eval_class_like_member_default(&declaring, property.trait_origin(), default, context, &mut default_scope, values)
        .map(Some)
}

/// Evaluates a reflected parameter default for `ReflectionParameter::getDefaultValue()`, in the
/// declaring callable's class and magic-constant scopes. `None` for a default eval does not
/// evaluate itself (a native factory, a deferred error), which the compiled getter answers.
fn eval_reflected_parameter_default(
    reflected: &crate::context::EvalReflectedParameterDefault,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    if let Some(class_name) = reflected.declaring_class.as_deref() {
        context.push_class_scope(class_name.to_string());
        context.push_called_class_scope(class_name.to_string());
    }
    if let Some((function_name, method_name, class_name, trait_name)) = reflected.magic_scope.as_ref() {
        context.push_callable_magic_scope(function_name, method_name, class_name.as_deref(), trait_name.as_deref());
    }
    let result = eval_method_parameter_default(&reflected.default, context, values);
    if reflected.magic_scope.is_some() {
        context.pop_magic_scope();
    }
    if reflected.declaring_class.is_some() {
        context.pop_called_class_scope();
        context.pop_class_scope();
    }
    match result {
        Ok(value) => Ok(Some(value)),
        Err(EvalStatus::UnsupportedConstruct) => Ok(None),
        Err(status) => Err(status),
    }
}

/// Reports whether parameter default `default`, which evaluated to `value`, yields a refcounted
/// value, which `RECV_INIT` does not cache, so such a default is evaluated again, and reports
/// again, on every call (see [`eval_result_is_refcounted`]).
pub(in crate::interpreter) fn eval_parameter_default_is_refcounted(
    default: &EvalExpr,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    // php-src never caches a default that constructs an object, whatever its result.
    if values.type_tag(value)? == EVAL_TAG_OBJECT || eval_expr_constructs_object(default) {
        return Ok(true);
    }
    eval_result_is_refcounted(default, true, context, values)
}

/// Reports whether `expr` evaluates to a refcounted value in php-src: an object, a non-empty
/// array built at run time or read from a constant, an array cast of anything but an array
/// that is not refcounted, an array `+`, or a string built at run time (a concatenation of two
/// non-empty strings, a string cast of a number that is not a single digit, a constant whose
/// value was built). An interned string, an immutable array, and a scalar are not. `folded`
/// tells whether php-src folded `expr` at compile time where it could: it does not fold the
/// branches of a conditional whose condition does not fold (`zend_eval_const_expr`), so a
/// literal-only branch there is still evaluated at run time. `RECV_INIT` caches a parameter
/// default only when it is not refcounted. Values are found by evaluating
/// subexpressions again silently (constant expressions, so without effects).
pub(in crate::interpreter) fn eval_result_is_refcounted<V: RuntimeValueOps>(
    expr: &EvalExpr,
    folded: bool,
    context: &mut ElephcEvalContext,
    values: &mut V,
) -> Result<bool, EvalStatus> {
    match expr {
        EvalExpr::ConstFetch(_) | EvalExpr::NamespacedConstFetch { .. } | EvalExpr::ClassConstantFetch { .. } => {
            return eval_constant_is_refcounted(expr, context, values);
        }
        _ => {}
    }
    if folded && eval_is_compile_time_literal(expr) {
        return Ok(false);
    }
    Ok(match expr {
        EvalExpr::Array(elements) => !elements.is_empty(),
        EvalExpr::NewObject { .. } | EvalExpr::NewAnonymousClass { .. } => true,
        EvalExpr::Binary { op: EvalBinOp::Concat, left, right } => {
            let left_empty = eval_default_condition(left, context, values, |value, values| eval_is_empty_string(value, values))?;
            let right_empty = eval_default_condition(right, context, values, |value, values| eval_is_empty_string(value, values))?;
            match (left_empty, right_empty) {
                (Some(true), _) => eval_string_result_is_refcounted(right, folded, context, values)?,
                (_, Some(true)) => eval_string_result_is_refcounted(left, folded, context, values)?,
                _ => true,
            }
        }
        EvalExpr::Binary { op: EvalBinOp::Add, left, right } => {
            eval_is_array_value(left, context, values)? == Some(true) || eval_is_array_value(right, context, values)? == Some(true)
        }
        EvalExpr::Cast { target: EvalCastType::String, expr: inner } => eval_string_result_is_refcounted(inner, folded, context, values)?,
        EvalExpr::Cast { target: EvalCastType::Array, expr: inner } => {
            eval_is_array_value(inner, context, values)? != Some(true) || eval_result_is_refcounted(inner, folded, context, values)?
        }
        EvalExpr::Ternary { condition, then_branch, else_branch } => {
            let branches_folded = folded && eval_is_compile_time_literal(condition);
            let yielded = then_branch.as_deref().unwrap_or(condition);
            match eval_default_condition(condition, context, values, |value, values| values.truthy(value))? {
                Some(true) => eval_result_is_refcounted(yielded, branches_folded, context, values)?,
                Some(false) => eval_result_is_refcounted(else_branch, branches_folded, context, values)?,
                None => {
                    eval_result_is_refcounted(yielded, branches_folded, context, values)?
                        || eval_result_is_refcounted(else_branch, branches_folded, context, values)?
                }
            }
        }
        EvalExpr::NullCoalesce { value, default } => {
            let branches_folded = folded && eval_is_compile_time_literal(value);
            match eval_default_condition(value, context, values, |value, values| Ok(values.type_tag(value)? != EVAL_TAG_NULL))? {
                Some(true) => eval_result_is_refcounted(value, branches_folded, context, values)?,
                Some(false) => eval_result_is_refcounted(default, branches_folded, context, values)?,
                None => {
                    eval_result_is_refcounted(value, branches_folded, context, values)?
                        || eval_result_is_refcounted(default, branches_folded, context, values)?
                }
            }
        }
        _ => false,
    })
}

/// Reports whether `expr` converted to a string is refcounted: a string as
/// [`eval_result_is_refcounted`] says, `"Array"` (interned) for an array, a number's digits
/// unless they are one character (php-src interns single-character strings), and an interned
/// `"1"` or `""` for a bool or null.
fn eval_string_result_is_refcounted<V: RuntimeValueOps>(
    expr: &EvalExpr,
    folded: bool,
    context: &mut ElephcEvalContext,
    values: &mut V,
) -> Result<bool, EvalStatus> {
    let tag = eval_default_condition(expr, context, values, |value, values| Ok(values.type_tag(value)? == EVAL_TAG_STRING))?;
    if tag != Some(false) {
        return eval_result_is_refcounted(expr, folded, context, values);
    }
    Ok(eval_default_condition(expr, context, values, |value, values| {
        Ok(match values.type_tag(value)? {
            EVAL_TAG_INT => !(0..=9).contains(&eval_int_value(value, values)?),
            EVAL_TAG_FLOAT => true,
            _ => false,
        })
    })?
    .unwrap_or(false))
}

/// Reports whether reading constant `expr` yields a refcounted value: an enum case object, a
/// non-empty array, or a string its initializer built at run time (a class constant's
/// initializer, or a string a `define()` argument built at run time). A predefined
/// constant holds an interned or immutable value.
fn eval_constant_is_refcounted<V: RuntimeValueOps>(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    values: &mut V,
) -> Result<bool, EvalStatus> {
    thread_local! {
        /// How many class-constant initializers the classification is following.
        static FOLLOWED_INITIALIZERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    if let EvalExpr::ClassConstantFetch { class_name, constant } = expr {
        // A constant that reads itself (`const K = self::K`) is an `Error` when evaluated; the
        // classification must not follow it forever.
        if FOLLOWED_INITIALIZERS.with(|depth| depth.get()) >= 16 {
            return Ok(false);
        }
        let class = resolve_eval_static_member_class_name(class_name, context)?;
        if eval_is_enum_case(&class, constant, context) {
            return Ok(true);
        }
        if let Some((owner, declared)) = context.class_constant(&class, constant) {
            let initializer = declared.value().clone();
            if eval_is_array_value(expr, context, values)? == Some(true) {
                return Ok(eval_default_condition(expr, context, values, |value, values| Ok(values.array_len(value)? != 0))?
                    .unwrap_or(true));
            }
            context.push_class_scope(owner.clone());
            FOLLOWED_INITIALIZERS.with(|depth| depth.set(depth.get() + 1));
            let result = eval_result_is_refcounted(&initializer, true, context, values);
            FOLLOWED_INITIALIZERS.with(|depth| depth.set(depth.get() - 1));
            context.pop_class_scope();
            return result;
        }
        return Ok(false);
    }
    let name = match expr {
        EvalExpr::ConstFetch(name) => name.as_str(),
        EvalExpr::NamespacedConstFetch { name, fallback_name } => {
            if context.constant(name).is_some() { name.as_str() } else { fallback_name.as_str() }
        }
        _ => return Ok(false),
    };
    if context.constant(name).is_none() {
        return Ok(false);
    }
    let name = name.to_string();
    let is_nonempty_array = eval_default_condition(expr, context, values, |value, values| {
        Ok(matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) && values.array_len(value)? != 0)
    })?;
    Ok(is_nonempty_array == Some(true) || context.defined_constant_was_built(&name))
}

/// Reports whether `expr` is what php-src folds to a literal at compile time: literals, arrays
/// of them, and operators and casts over them.
fn eval_is_compile_time_literal(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Const(_) => true,
        EvalExpr::Array(elements) => elements.iter().all(|element| match element {
            EvalArrayElement::Value(value) => eval_is_compile_time_literal(value),
            EvalArrayElement::KeyValue { key, value } => eval_is_compile_time_literal(key) && eval_is_compile_time_literal(value),
            _ => false,
        }),
        EvalExpr::Binary { left, right, .. } => eval_is_compile_time_literal(left) && eval_is_compile_time_literal(right),
        EvalExpr::Unary { expr, .. } | EvalExpr::Cast { expr, .. } => eval_is_compile_time_literal(expr),
        EvalExpr::Ternary { condition, then_branch, else_branch } => {
            eval_is_compile_time_literal(condition)
                && then_branch.as_deref().is_none_or(eval_is_compile_time_literal)
                && eval_is_compile_time_literal(else_branch)
        }
        EvalExpr::NullCoalesce { value, default } => eval_is_compile_time_literal(value) && eval_is_compile_time_literal(default),
        _ => false,
    }
}

/// Reports whether `expr` evaluates to an array, evaluating it again silently.
fn eval_is_array_value<V: RuntimeValueOps>(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    values: &mut V,
) -> Result<Option<bool>, EvalStatus> {
    eval_default_condition(expr, context, values, |value, values| Ok(matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)))
}

/// Reports whether `value` is the empty string.
fn eval_is_empty_string<V: RuntimeValueOps>(value: RuntimeCellHandle, values: &mut V) -> Result<bool, EvalStatus> {
    Ok(values.type_tag(value)? == EVAL_TAG_STRING && values.string_bytes(value)?.is_empty())
}

/// Evaluates a default's condition again silently and applies `test` to its value; `None` when
/// the silent evaluation cannot answer, or would construct an object (running user code again).
fn eval_default_condition<V: RuntimeValueOps>(
    condition: &EvalExpr,
    context: &mut ElephcEvalContext,
    values: &mut V,
    test: impl FnOnce(RuntimeCellHandle, &mut V) -> Result<bool, EvalStatus>,
) -> Result<Option<bool>, EvalStatus> {
    if eval_expr_constructs_object(condition) {
        return Ok(None);
    }
    let mut condition_scope = ElephcEvalScope::new();
    context.begin_deprecation_silence();
    context.enter_constant_expression();
    let value = eval_expr(condition, context, &mut condition_scope, values);
    context.leave_constant_expression();
    context.end_deprecation_silence();
    let Some(value) = eval_preview_outcome(value, context, values)? else {
        return Ok(None);
    };
    let answer = test(value, values);
    if !value.is_borrowed() {
        values.release(value)?;
    }
    answer.map(Some)
}

/// Reports whether evaluating constant expression `expr` constructs an object (`new`).
pub(in crate::interpreter) fn eval_expr_constructs_object(expr: &EvalExpr) -> bool {
    use crate::eval_ir::EvalArrayElement;
    match expr {
        EvalExpr::NewObject { .. } | EvalExpr::NewAnonymousClass { .. } => true,
        EvalExpr::ArrayGet { array, index } => eval_expr_constructs_object(array) || eval_expr_constructs_object(index),
        EvalExpr::Array(elements) => elements.iter().any(|element| match element {
            EvalArrayElement::Value(value) => eval_expr_constructs_object(value),
            EvalArrayElement::KeyValue { key, value } => eval_expr_constructs_object(key) || eval_expr_constructs_object(value),
            _ => false,
        }),
        EvalExpr::NullCoalesce { value, default } => eval_expr_constructs_object(value) || eval_expr_constructs_object(default),
        EvalExpr::Ternary { condition, then_branch, else_branch } => {
            eval_expr_constructs_object(condition)
                || then_branch.as_deref().is_some_and(eval_expr_constructs_object)
                || eval_expr_constructs_object(else_branch)
        }
        EvalExpr::Cast { expr, .. } | EvalExpr::Unary { expr, .. } => eval_expr_constructs_object(expr),
        EvalExpr::Binary { left, right, .. } => eval_expr_constructs_object(left) || eval_expr_constructs_object(right),
        _ => false,
    }
}

/// Records what eval class or trait `name` copied already evaluated when it was declared.
/// A declaration copies its parent's default table and its traits' defaults and constants as
/// they are when it is bound, which is program entry for a class php-src binds while compiling
/// the program (see `early_binding`) and its statement for any other: an instance slot is
/// copied evaluated when the parent (or trait) it comes from was initialized, or had itself
/// copied it evaluated, and a trait constant the trait already evaluated is not evaluated
/// again for the class.
pub(in crate::interpreter) fn eval_record_declaration_snapshot(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (properties, constants, parent, holder): (Vec<EvalClassProperty>, Vec<EvalClassConstant>, Option<String>, String) =
        if let Some(class) = context.class(name) {
            (class.properties().to_vec(), class.constants().to_vec(), class.parent().map(str::to_string), class.name().to_string())
        } else if let Some(trait_decl) = context.trait_decl(name) {
            (trait_decl.properties().to_vec(), trait_decl.constants().to_vec(), None, trait_decl.name().to_string())
        } else {
            return Ok(());
        };
    let mut records: Vec<(String, String)> = Vec::new();
    if let Some(parent) = parent.as_deref() {
        let parent_initialized = context.class_initialization_ran(parent);
        for ancestor in context.class_chain(parent) {
            for property in ancestor.properties().iter().filter(|property| !property.is_static()) {
                if parent_initialized || context.slot_precopied(parent, ancestor.name(), property.name()) {
                    records.push((ancestor.name().to_string(), property.name().to_string()));
                }
            }
        }
    }
    for property in properties.iter().filter(|property| !property.is_static()) {
        let Some(origin) = property.trait_origin() else { continue };
        if context.trait_decl(origin).is_some()
            && (context.class_initialization_ran(origin) || context.slot_precopied(origin, origin, property.name()))
        {
            records.push((holder.clone(), property.name().to_string()));
        }
    }
    for (declaring, property) in records {
        context.record_precopied_slot(&holder, &declaring, &property);
    }
    // A trait constant the trait already evaluated is copied with its value.
    for constant in &constants {
        let Some(origin) = constant.trait_origin() else { continue };
        if context.trait_decl(origin).is_none() || context.class_constant_cell(&holder, constant.name()).is_some() {
            continue;
        }
        if let Some(cell) = context.class_constant_cell(origin, constant.name()) {
            let copy = values.retain(cell)?;
            if let Some(replaced) = context.set_class_constant_cell(&holder, constant.name(), copy) {
                values.release(replaced)?;
            }
        }
    }
    Ok(())
}
