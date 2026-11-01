//! Purpose:
//! Evaluates PHP null-coalescing assignment expressions against writable EvalIR locations.
//!
//! Called from:
//! - `super::eval_expr()` for `EvalExpr::NullCoalesceAssign`.
//!
//! Key details:
//! - Target expressions and array indexes are evaluated once in source order.
//! - The default expression stays lazy and writes back through nested array/property locations.

use super::*;

/// One writable location, with an observation only when a read was requested.
enum EvaluatedLocation {
    GlobalVariable {
        name: String,
        current: Option<RuntimeCellHandle>,
    },
    Variable {
        name: String,
        current: Option<RuntimeCellHandle>,
        current_is_borrowed: bool,
        ownership: ScopeCellOwnership,
    },
    Property {
        object: RuntimeCellHandle,
        property: String,
        current: Option<RuntimeCellHandle>,
    },
    StaticProperty {
        class_name: String,
        property: String,
        current: Option<RuntimeCellHandle>,
    },
    ArrayElement {
        parent: Box<EvaluatedLocation>,
        index: RuntimeCellHandle,
        current: Option<RuntimeCellHandle>,
    },
    ArrayAccessElement {
        object: RuntimeCellHandle,
        index: RuntimeCellHandle,
        current: Option<RuntimeCellHandle>,
    },
}

impl EvaluatedLocation {
    /// Returns the value observed while evaluating this writable location.
    const fn current(&self) -> RuntimeCellHandle {
        match self {
            Self::GlobalVariable { current, .. }
            | Self::Variable { current, .. }
            | Self::Property { current, .. }
            | Self::StaticProperty { current, .. }
            | Self::ArrayElement { current, .. }
            | Self::ArrayAccessElement { current, .. } => {
                current.expect("a read location must carry an observed value")
            }
        }
    }

    /// Returns whether the observed value is owned by an existing scope entry or class slot.
    ///
    /// A static property read hands out the very cell the class slot holds
    /// (`context.static_property()` does not retain it), exactly like a borrowed scope variable.
    /// Treating it as owned made `self::$registry ??= self::create()` release the slot's null
    /// cell before storing into that slot (a double release), and hand the stored object back
    /// UNRETAINED on every later call, so the caller's release destroyed the object the class
    /// still held -- Doctrine DBAL's `Type::getTypeRegistry()`, whose registry was destructed after
    /// its first use and read back as a string.
    const fn current_is_borrowed(&self) -> bool {
        matches!(
            self,
            Self::Variable {
                current_is_borrowed: true,
                ..
            } | Self::StaticProperty { .. }
        )
    }

    /// Returns whether the observed value is a cell some storage still holds, so this
    /// evaluation must not release it.
    ///
    /// A static property fetch hands out the slot's own cell, like a scope variable read (see
    /// `eval_expr_result_aliases_storage`). Releasing it as a temporary freed the slot's null
    /// cell before `self::$instance ??= new self()` stored into it, and the next read of the
    /// property found whatever reused that cell -- Carbon's `FactoryImmutable::getDefaultInstance()`
    /// returned an int, then an `InputBag`.
    const fn current_is_held_by_storage(&self) -> bool {
        self.current_is_borrowed() || matches!(self, Self::StaticProperty { .. })
    }
}

/// Evaluates `target ??= default`, preserving PHP's lazy RHS and single target evaluation.
pub(super) fn eval_null_coalesce_assign(
    target: &EvalExpr,
    default: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // `$t ??= $v` reads the target in PHP's QUIET fetch mode: an uninitialized typed property
    // is "absent" and gets assigned, it does not raise. Only the READ is quiet — the default
    // expression is evaluated normally, so a throw inside it still surfaces.
    context.push_quiet_property_fetch();
    let location = evaluate_location(target, context, scope, values);
    context.pop_quiet_property_fetch();
    let mut location = location?;
    let current = location.current();
    if !values.is_null(current)? {
        let result = if location.current_is_borrowed() {
            values.retain(current)?
        } else {
            current
        };
        // No write follows: the containers and keys this location fetched go unused.
        abandon_location(location, context, values)?;
        return Ok(result);
    }
    if !location.current_is_held_by_storage() {
        eval_release_value(context, values, current)?;
        if let EvaluatedLocation::GlobalVariable { current, .. } = &mut location {
            *current = None;
        }
    }
    let assigned = eval_expr(default, context, scope, values)?;
    // The default ran AFTER the target's containers were observed, and it may have written to
    // them: `self::$hydrators[$scope] ??= self::getHydrator($scope)` where `getHydrator()`
    // stores `self::$hydrators['stdClass']`. The observed array was released by that store, so
    // writing through it wrote into a reused cell -- "Cannot use object of type stdClass as
    // array" -- and would have dropped the default's own entry besides. PHP fetches the
    // containers for the write only now; so do we, reusing the indexes already evaluated so no
    // index expression runs twice.
    context.push_quiet_property_fetch();
    let location = refresh_element_containers(location, context, scope, values);
    context.pop_quiet_property_fetch();
    write_location(location?, assigned, true, context, scope, values)?;
    Ok(assigned)
}

/// Releases the element value an array-element location read, which no write consumes.
///
/// `write_location` writes through the element's PARENT container; the element's own observed
/// value is only an operand (of a compound assignment) or unused (a plain write), and an owned
/// read of it leaked one reference per element write. (`??=` already released its observation
/// before running the default.)
fn release_element_observation(
    location: &mut EvaluatedLocation,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let EvaluatedLocation::ArrayElement { current, .. } = location {
        if let Some(observed) = current.take() {
            if !observed.is_borrowed() {
                eval_release_value(context, values, observed)?;
            }
        }
    }
    Ok(())
}

/// Re-reads the containers of an array-element location, keeping its evaluated indexes.
fn refresh_element_containers(
    location: EvaluatedLocation,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedLocation, EvalStatus> {
    match location {
        EvaluatedLocation::ArrayElement { parent, index, current } => Ok(EvaluatedLocation::ArrayElement {
            parent: Box::new(refresh_container(*parent, context, scope, values)?),
            index,
            current,
        }),
        other => Ok(other),
    }
}

/// Re-reads one container level (and every level above it) for a pending element write.
fn refresh_container(
    location: EvaluatedLocation,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedLocation, EvalStatus> {
    match location {
        EvaluatedLocation::StaticProperty { class_name, property, .. } => {
            let current = eval_static_property_get_result(&class_name, &property, context, values)?;
            Ok(EvaluatedLocation::StaticProperty { class_name, property, current: Some(current) })
        }
        EvaluatedLocation::Property { object, property, current: stale } => {
            // The container read before the default ran is superseded, and nothing consumed it.
            release_stale_container(stale, context, values)?;
            let current = eval_property_array_target_get_result(object, &property, context, values)?;
            Ok(EvaluatedLocation::Property { object, property, current: Some(current) })
        }
        EvaluatedLocation::GlobalVariable { name, .. } => {
            let current = read_global_value(&name, true, context, scope, values)?;
            Ok(EvaluatedLocation::GlobalVariable { name, current: Some(current) })
        }
        EvaluatedLocation::Variable { name, .. } => {
            let entry = scope_entry(context, scope, &name).filter(|entry| entry.flags().is_visible());
            let current_is_borrowed = entry.is_some();
            let ownership = entry
                .as_ref()
                .map(|entry| entry.flags().ownership)
                .unwrap_or(ScopeCellOwnership::Owned);
            let current = entry.map_or_else(|| values.null(), |entry| Ok(entry.cell()))?;
            Ok(EvaluatedLocation::Variable { name, current: Some(current), current_is_borrowed, ownership })
        }
        EvaluatedLocation::ArrayElement { parent, index, current: stale } => {
            release_stale_container(stale, context, values)?;
            let parent = refresh_container(*parent, context, scope, values)?;
            let current = eval_array_get_result(parent.current(), index, context, values)?;
            Ok(EvaluatedLocation::ArrayElement { parent: Box::new(parent), index, current: Some(current) })
        }
        other @ EvaluatedLocation::ArrayAccessElement { .. } => Ok(other),
    }
}

/// Releases what a location fetched for a write that will not happen: element keys and the
/// containers above the target. The target's own observed value is the caller's to settle.
fn abandon_location(
    location: EvaluatedLocation,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match location {
        EvaluatedLocation::ArrayElement { parent, index, .. } => {
            if !index.is_borrowed() {
                eval_release_value(context, values, index)?;
            }
            abandon_container(*parent, context, values)
        }
        EvaluatedLocation::ArrayAccessElement { index, .. } => {
            if index.is_borrowed() {
                return Ok(());
            }
            eval_release_value(context, values, index)
        }
        _ => Ok(()),
    }
}

/// Releases a container location's own observed value, then everything above it.
fn abandon_container(
    location: EvaluatedLocation,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let held = location.current_is_held_by_storage();
    let observed = match &location {
        EvaluatedLocation::GlobalVariable { current, .. }
        | EvaluatedLocation::Variable { current, .. }
        | EvaluatedLocation::Property { current, .. }
        | EvaluatedLocation::StaticProperty { current, .. }
        | EvaluatedLocation::ArrayElement { current, .. }
        | EvaluatedLocation::ArrayAccessElement { current, .. } => *current,
    };
    if let Some(observed) = observed {
        if !held && !observed.is_borrowed() {
            eval_release_value(context, values, observed)?;
        }
    }
    abandon_location(location, context, values)
}

/// Releases an owned container read that a refreshed location replaces.
fn release_stale_container(
    stale: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match stale {
        Some(stale) if !stale.is_borrowed() => eval_release_value(context, values, stale),
        _ => Ok(()),
    }
}

/// Evaluates a compound assignment expression and writes its computed result back to the target.
pub(super) fn eval_compound_assign(
    target: &EvalExpr,
    op: EvalBinOp,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut location = evaluate_location(target, context, scope, values)?;
    let current = location.current();
    // The observed value and the right-hand side are operands only: `eval_binary_result`
    // borrows both and returns a fresh result. A property or element read handed this
    // evaluation its own reference, which was never released -- on success, and on a
    // right-hand side that throws (`$o->s .= f();` in a `try`) -- so every statement-form
    // property compound assignment leaked its old value.
    let value = match eval_owned_expr(value, context, scope, values) {
        Ok(value) => value,
        Err(status) => {
            release_compound_operand(&mut location, current, None, context, values)?;
            abandon_location(location, context, values)?;
            return Err(status);
        }
    };
    let assigned = eval_binary_result(op, current, value, context, values);
    let assigned = match assigned {
        Ok(assigned) => assigned,
        Err(status) => {
            eval_release_value(context, values, value)?;
            release_compound_operand(&mut location, current, None, context, values)?;
            abandon_location(location, context, values)?;
            return Err(status);
        }
    };
    if value != assigned {
        eval_release_value(context, values, value)?;
    }
    release_compound_operand(&mut location, current, Some(assigned), context, values)?;
    write_location(location, assigned, true, context, scope, values)?;
    Ok(assigned)
}

/// Releases a compound assignment's observed value when this evaluation owns it.
///
/// A value some storage still holds (a scope variable, a static property slot) is borrowed. A
/// global's observed copy is released by `write_location` itself, so it is detached instead.
fn release_compound_operand(
    location: &mut EvaluatedLocation,
    current: RuntimeCellHandle,
    assigned: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if location.current_is_held_by_storage() || current.is_borrowed() || assigned == Some(current) {
        return Ok(());
    }
    if let EvaluatedLocation::GlobalVariable { current: observed, .. } = location {
        *observed = None;
    }
    eval_release_value(context, values, current)
}

/// Evaluates a postfix increment or decrement and returns the value observed before mutation.
pub(super) fn eval_postfix_inc_dec(
    target: &EvalExpr,
    increment: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let location = evaluate_location(target, context, scope, values)?;
    let current = location.current();
    let result = values.retain(current)?;
    let updated = eval_inc_dec_value(current, increment, values)?;
    write_location(location, updated, false, context, scope, values)?;
    Ok(result)
}

/// Evaluates a plain assignment expression and returns the stored value.
pub(in crate::interpreter) fn eval_assign(
    target: &EvalExpr,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let trace = crate::eval_trace::enabled();
    // php evaluates the right-hand side BEFORE it fetches the container it writes into (the
    // write fetches are delayed opcodes), so a value that itself writes the same property is
    // seen. Symfony's generated container is written that way:
    // `$container->privates['a'] = new A(($container->privates['b'] ?? self::getB($container)))`
    // stores 'b' while building the value, and fetching 'privates' first then storing the stale
    // array back lost 'b' -- the next lookup built a second instance of that shared service.
    // Only a side-effect-free index path is reordered, so no key expression runs out of turn.
    let value_first = eval_assignment_fetch_is_deferrable(target);
    let early_value = if value_first {
        Some(eval_expr(value, context, scope, values).map_err(|status| {
            if trace {
                eprintln!("[elephc-eval-trace] phase=assign_error stage=value value={value:?} status={status:?}");
            }
            status
        })?)
    } else {
        None
    };
    let mut location = evaluate_plain_assignment_location(target, context, scope, values).map_err(|status| {
        if trace {
            eprintln!("[elephc-eval-trace] phase=assign_error stage=location target={target:?} status={status:?}");
        }
        status
    })?;
    release_element_observation(&mut location, context, values)?;
    let assigned = match early_value {
        Some(assigned) => assigned,
        None => eval_expr(value, context, scope, values).map_err(|status| {
            if trace {
                eprintln!("[elephc-eval-trace] phase=assign_error stage=value value={value:?} status={status:?}");
            }
            status
        })?,
    };
    let assigned = if eval_expr_result_aliases_storage(value) {
        values.copy_value(assigned)?
    } else {
        eval_detach_property_read(value, assigned, context, values)?
    };
    write_location(location, assigned, true, context, scope, values).map_err(|status| {
        if trace {
            eprintln!("[elephc-eval-trace] phase=assign_error stage=write target={target:?} status={status:?}");
        }
        status
    })?;
    Ok(assigned)
}

/// Whether an element write into a property may fetch its container after the value is built.
///
/// True for `$obj->p[k1][k2] = ...` (and the static-property form) whose receiver and keys are
/// constants or plain variables, so evaluating them later changes nothing observable.
fn eval_assignment_fetch_is_deferrable(target: &EvalExpr) -> bool {
    fn pure_operand(expr: &EvalExpr) -> bool {
        matches!(expr, EvalExpr::Const(_) | EvalExpr::LoadVar(_) | EvalExpr::ClassConstantFetch { .. })
    }
    let EvalExpr::ArrayGet { .. } = target else {
        return false;
    };
    let mut current = target;
    loop {
        match current {
            EvalExpr::ArrayGet { array, index } => {
                if !pure_operand(index) {
                    return false;
                }
                current = array;
            }
            EvalExpr::PropertyGet { object, .. } => return pure_operand(object),
            EvalExpr::StaticPropertyGet { .. } => return true,
            _ => return false,
        }
    }
}

/// Binds a variable to a non-variable lvalue's persistent PHP reference target.
///
/// `$x = &$y;` aliases two scope names and stays on `set_reference_alias()`; every other PHP
/// reference source — a property, a static property, an array element, their dynamic-name forms
/// — names storage outside the scope, so the variable is rebound as a borrowed view of the
/// source's current cell plus the reference target that later writes travel back through.
pub(in crate::interpreter) fn eval_var_reference_bind(
    name: &str,
    source: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (source_target, source_value) = eval_reference_source(source, context, scope, values)
        .map_err(|status| {
            if crate::eval_trace::enabled() {
                eprintln!(
                    "[elephc-eval-trace] phase=reference_bind_error target={name:?} source={source:?} status={status:?}"
                );
            }
            status
        })?;
    // `$array = &$array[$key]` over a by-reference PARAMETER (Laravel's `Arr::set()` walks a dotted
    // key this way): the element target would name the function's own `$array`, which this very
    // statement rebinds -- so the write-back at the end of the call then copied the innermost
    // element over the caller's variable. The element is an element of whatever `$array` aliased,
    // so name it through that storage instead.
    //
    // Another local referencing the same array (`$cv = &$data; ... $cv = &$cv[$k];`, the key-path
    // walk of dflydev's `Data::set()` and league/commonmark's configuration) names it best: the
    // element becomes "element $k of `$data`". Without one, `$array` is a by-reference parameter
    // and the caller's storage is its reference target.
    let source_target = match (&source_target, source) {
        (
            EvalReferenceTarget::ArrayElement {
                scope: element_scope,
                array_name,
                index,
            },
            EvalExpr::ArrayGet { array, .. },
        ) if array_name == name
            && matches!(array.as_ref(), EvalExpr::LoadVar(base) if base == name) =>
        {
            if let Some(peer) = scope.reference_peer(name) {
                EvalReferenceTarget::ArrayElement {
                    scope: element_scope.clone(),
                    array_name: peer,
                    index: index.clone(),
                }
            } else {
                match scope.reference_target(name).cloned() {
                    Some(outer) => EvalReferenceTarget::NestedArrayElement {
                        array_target: Box::new(outer),
                        index: index.clone(),
                    },
                    None => source_target,
                }
            }
        }
        _ => source_target,
    };
    let replaced = scope.rebind_reference(
        name.to_string(),
        source_value,
        ScopeCellOwnership::Borrowed,
    );
    scope.set_reference_target(name.to_string(), source_target);
    if let Some(replaced) = replaced {
        eval_release_value(context, values, replaced)?;
    }
    Ok(())
}

/// Binds an array element lvalue to a persistent PHP reference source expression.
pub(in crate::interpreter) fn eval_array_reference_bind(
    target: &EvalExpr,
    source: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (source_target, source_value) = eval_reference_source(source, context, scope, values)?;
    let location = evaluate_plain_assignment_location(target, context, scope, values)?;
    write_reference_location(location, source_target, source_value, context, scope, values)
}

/// Binds a NEWLY APPENDED element of one writable array lvalue to a persistent reference source.
///
/// The target is a general lvalue, not a bare name: PHP appends through a nested element as
/// readily as through a variable, and `$loops[$k][] = &$path;` is that shape.
pub(in crate::interpreter) fn eval_array_append_reference_bind(
    target: &EvalExpr,
    source: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let (source_target, source_value) = eval_reference_source(source, context, scope, values)?;
    let parent = evaluate_location(target, context, scope, values)?;
    let array = if values.is_null(parent.current())? {
        values.array_new(1)?
    } else {
        parent.current()
    };
    let index = eval_array_append_key(array, values)?;
    let location = EvaluatedLocation::ArrayElement {
        parent: Box::new(parent),
        index,
        current: None,
    };
    write_reference_location(location, source_target, source_value, context, scope, values)
}

/// How many `return` statements of by-reference functions have run in this process.
///
/// `$x = &f()` reads it around the call: an unchanged count means no by-reference function
/// returned during the call, so the callee is a by-value one and php's "Only variables should be
/// assigned by reference" notice applies. A by-reference callee that returned a non-variable has
/// already raised its own notice, and php raises only that one.
static BY_REF_RETURN_EXECUTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Executes `return EXPR;` inside a function declared to return BY REFERENCE.
///
/// The caller may bind to what comes back, so the reference has to survive the activation being
/// drained. Whatever outlives the call owns its reference: the value cell is RETAINED into the
/// context, and the target is rewritten to one that outlives the frame -- a static local names
/// the context store, a by-reference parameter's element names the CALLER's target, and a plain
/// local, whose storage really does die with the call, keeps only the cell.
///
/// A non-reference return is not an error: `php -n` 8.5.6 emits
/// `Notice: Only variable references should be returned by reference` and binds a copy.
pub(in crate::interpreter) fn eval_by_ref_return(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    BY_REF_RETURN_EXECUTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if !eval_expr_binds_a_reference_source(expr) {
        // Same diagnostic channel every other notice-level message here uses.
        values.warning("Only variable references should be returned by reference")?;
        let value = eval_expr(expr, context, scope, values)?;
        return Ok(EvalControl::Return(value));
    }
    let (target, value) = eval_reference_source(expr, context, scope, values)?;
    let target = eval_persistent_return_target(target, context, scope);
    // An object target names its owner by cell handle, and for `return $this->p;` that handle is
    // the activation's own `$this` cell, which the frame releases on exit: the caller's alias
    // keeps a reference of its own, as a property alias bound inside a method does
    // (`eval_property_reference_target` callers). Symfony's `ContextListener::onKernelResponse`
    // binds `&$session->getUsageIndex()` and its next write read the freed cell.
    let target = match target {
        EvalReferenceTarget::ObjectProperty {
            object,
            property,
            access_scope,
        } => EvalReferenceTarget::ObjectProperty {
            object: values.retain(object)?,
            property,
            access_scope,
        },
        other => other,
    };
    let retained = values.retain(value)?;
    if let Some(unclaimed) = context.set_pending_return_reference(target, retained) {
        eval_release_value(context, values, unclaimed)?;
    }
    Ok(EvalControl::Return(value))
}

/// Returns whether one expression is a CALL, whose reference comes from the callee's `return`.
fn eval_expr_is_call_shaped(expr: &EvalExpr) -> bool {
    matches!(
        expr,
        EvalExpr::Call { .. }
            | EvalExpr::NamespacedCall { .. }
            | EvalExpr::DynamicCall { .. }
            | EvalExpr::MethodCall { .. }
            | EvalExpr::NullsafeMethodCall { .. }
            | EvalExpr::DynamicMethodCall { .. }
            | EvalExpr::NullsafeDynamicMethodCall { .. }
            | EvalExpr::StaticMethodCall { .. }
            | EvalExpr::DynamicStaticMethodCall { .. }
    )
}

/// Returns whether one returned expression names storage PHP can hand back by reference.
fn eval_expr_binds_a_reference_source(expr: &EvalExpr) -> bool {
    matches!(
        expr,
        EvalExpr::LoadVar(_)
            | EvalExpr::ArrayGet { .. }
            | EvalExpr::PropertyGet { .. }
            | EvalExpr::DynamicPropertyGet { .. }
            | EvalExpr::StaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyGet { .. }
            | EvalExpr::DynamicStaticPropertyNameGet { .. }
    )
}

/// Rewrites a returned reference target into one that outlives the activation scope.
fn eval_persistent_return_target(
    target: EvalReferenceTarget,
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
) -> EvalReferenceTarget {
    let function = context.current_function().unwrap_or_default().to_string();
    match target {
        EvalReferenceTarget::Variable { scope: _, name } => {
            eval_persistent_name_target(&function, &name, context, scope)
        }
        EvalReferenceTarget::ArrayElement {
            scope: _,
            array_name,
            index,
        } => {
            let array_target =
                eval_persistent_name_target(&function, &array_name, context, scope);
            EvalReferenceTarget::NestedArrayElement {
                array_target: Box::new(array_target),
                index,
            }
        }
        target => target,
    }
}

/// Returns the persistent target one scope NAME denotes, or a bare cell when nothing outlives it.
fn eval_persistent_name_target(
    function: &str,
    name: &str,
    context: &ElephcEvalContext,
    scope: &ElephcEvalScope,
) -> EvalReferenceTarget {
    if let Some(target) = scope.reference_target(name) {
        // A by-reference PARAMETER already points at the caller's storage, which outlives this
        // call by construction.
        return target.clone();
    }
    if context.static_local(function, name).is_some() {
        return EvalReferenceTarget::StaticLocal {
            function: function.to_string(),
            name: name.to_string(),
        };
    }
    // A plain local's storage really does die with the call. PHP keeps the VALUE alive because
    // the reference is refcounted, and that is what the retained cell does here; there is no
    // storage left for a write to reach, which is what php shows too.
    EvalReferenceTarget::Cell {
        cell: scope
            .visible_cell(name)
            .unwrap_or_else(|| RuntimeCellHandle::from_raw(std::ptr::null_mut())),
    }
}

/// Binds one reference and returns the bound VALUE, for `TARGET = &SOURCE` in expression position.
///
/// PHP's value here is the bound value as a COPY rather than a second alias: after
/// `$a = ($b = &$one); $one = 9;` php reports `$b` as 9 and `$a` as the 1 it copied. Returning
/// the source's current cell gives exactly that, because the outer assignment copies it.
pub(in crate::interpreter) fn eval_reference_bind_result(
    target: &EvalExpr,
    source: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    // Two plain NAMES alias SYMMETRICALLY -- writing either updates both -- and only the scope's
    // named-alias table models that; a one-way reference target does not. This is the same split
    // the statement world makes between `ReferenceAssign` and `VarReferenceBind`.
    if let (EvalExpr::LoadVar(name), EvalExpr::LoadVar(source_name)) = (target, source) {
        for replaced in set_reference_alias(context, scope, name, source_name, values)? {
            eval_release_value(context, values, replaced)?;
        }
        return visible_scope_cell(context, scope, name).map_or_else(|| values.null(), Ok);
    }
    let (source_target, source_value) = eval_reference_source(source, context, scope, values)?;
    // A NAME aliases through the scope's own alias table, which is what makes two names update
    // each other symmetrically; `write_reference_location` knows only element targets and would
    // refuse this one. The two are separate mechanisms in the statement world too
    // (`ReferenceAssign` and `VarReferenceBind` against `ArrayReferenceBind`).
    if let EvalExpr::LoadVar(name) = target {
        let replaced = scope.rebind_reference(
            name.to_string(),
            source_value,
            ScopeCellOwnership::Borrowed,
        );
        scope.set_reference_target(name.to_string(), source_target);
        if let Some(replaced) = replaced {
            eval_release_value(context, values, replaced)?;
        }
        return Ok(source_value);
    }
    let location = evaluate_plain_assignment_location(target, context, scope, values)?;
    write_reference_location(location, source_target, source_value, context, scope, values)?;
    Ok(source_value)
}

/// Appends a value through any writable array lvalue while preserving PHP evaluation order.
pub(in crate::interpreter) fn eval_array_append(
    target: &EvalExpr,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    // The appended cell is the one the array now holds, exactly as `EvalExpr::Assign` returns
    // the cell it stored, so a statement append has nothing of its own to release.
    eval_array_append_result(target, value, context, scope, values).map(|_| ())
}

/// Appends a value and returns the ASSIGNED value, which is what `$a[] = $v` evaluates to.
///
/// PHP's append is an assignment expression: `return $this->rules[] = $r;` returns `$r`, and
/// `$a[] = $b[] = 'x'` appends the same value to both. The returned handle ALIASES the array's
/// element rather than carrying a reference of its own, which is the rule
/// `eval_expr_result_aliases_storage` records for every assignment shape.
pub(in crate::interpreter) fn eval_array_append_result(
    target: &EvalExpr,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if target.is_call_result() {
        return eval_call_result_append(target, value, context, scope, values);
    }
    let location = evaluate_location_with_fetch(target, LocationFetch::ArrayParent, context, scope, values)?;
    let current = location.current();
    if values.type_tag(current)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(current, context, values)? {
            return Err(EvalStatus::RuntimeFatal);
        }
        let offset = values.null()?;
        let value = eval_expr(value, context, scope, values)?;
        let result = eval_method_call_result(current, "offsetSet", vec![offset, value], context, values)?;
        // `offsetSet()` returns void in PHP; the expression's value is the ASSIGNED one.
        eval_release_value(context, values, result)?;
        return Ok(value);
    }
    let array = if values.is_null(current)? {
        values.array_new(1)?
    } else {
        current
    };
    let index = eval_array_append_key(array, values)?;
    let value = eval_expr_value_for_consuming_store(value, context, scope, values)?;
    let updated = values.array_set(array, index, value)?;
    write_location(location, updated, false, context, scope, values)?;
    Ok(value)
}

/// `f()[] = $v`: the call's result is a temporary. An `ArrayAccess` object takes the value through
/// `offsetSet(null, $v)`; any other result is dropped with the append, as in php. Either way the
/// expression is the assigned value.
fn eval_call_result_append(
    target: &EvalExpr,
    value: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let receiver = eval_owned_expr(target, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    if values.type_tag(receiver)? == EVAL_TAG_OBJECT {
        if !eval_array_access_object_matches(receiver, context, values)? {
            eval_release_value(context, values, receiver)?;
            return Err(EvalStatus::RuntimeFatal);
        }
        let offset = values.null()?;
        let result =
            eval_method_call_result(receiver, "offsetSet", vec![offset, value], context, values)?;
        eval_release_value(context, values, result)?;
    }
    eval_release_value(context, values, receiver)?;
    Ok(value)
}

/// Evaluates a plain-assignment target without reading an existing property value first.
fn evaluate_plain_assignment_location(
    target: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedLocation, EvalStatus> {
    match target {
        EvalExpr::ArrayGet { array, index } if is_globals_array(array) => {
            Ok(EvaluatedLocation::GlobalVariable {
                name: eval_global_name(index, context, scope, values)?,
                current: None,
            })
        }
        EvalExpr::LoadVar(name) => {
            let entry = scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible());
            Ok(EvaluatedLocation::Variable {
                name: name.clone(),
                current: None,
                current_is_borrowed: false,
                ownership: entry.map_or(ScopeCellOwnership::Owned, |entry| entry.flags().ownership),
            })
        }
        // `$$name = v`: the name is evaluated once, then the write is an ordinary variable write.
        EvalExpr::DynamicVar(name) => {
            let name = eval_dynamic_member_name(name, context, scope, values)?;
            evaluate_plain_assignment_location(&EvalExpr::LoadVar(name), context, scope, values)
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            Ok(EvaluatedLocation::Property {
                object,
                property: property.clone(),
                current: None,
            })
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            Ok(EvaluatedLocation::Property {
                object,
                property,
                current: None,
            })
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => Ok(EvaluatedLocation::StaticProperty {
            class_name: class_name.clone(),
            property: property.clone(),
            current: None,
        }),
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            Ok(EvaluatedLocation::StaticProperty {
                class_name,
                property: property.clone(),
                current: None,
            })
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            Ok(EvaluatedLocation::StaticProperty {
                class_name,
                property,
                current: None,
            })
        }
        EvalExpr::ArrayGet { array, index } => {
            // A plain `$object[$key] = $value` on an ArrayAccess object is ONE `offsetSet()`
            // call: php never reads the element first. Observing it through `offsetGet()` ran
            // user code php does not run, and an `offsetGet()` that throws for an unknown key
            // made the write itself throw -- Symfony Intl's `RingBuffer`, written by
            // `BufferedBundleReader::read()` as `$this->buffer[$hash] = ...`, so every locale
            // bundle read failed with "The index ... does not exist". Only a compound
            // assignment observes the element, and it goes through `evaluate_location`.
            let parent =
                evaluate_location_with_fetch(array, LocationFetch::ArrayParent, context, scope, values)?;
            let container = parent.current();
            let index = eval_expr(index, context, scope, values)?;
            if values.type_tag(container)? == EVAL_TAG_OBJECT
                && eval_array_access_object_matches(container, context, values)?
            {
                return Ok(EvaluatedLocation::ArrayAccessElement {
                    object: container,
                    index,
                    current: None,
                });
            }
            let current = eval_array_get_result(container, index, context, values)?;
            Ok(EvaluatedLocation::ArrayElement {
                parent: Box::new(parent),
                index,
                current: Some(current),
            })
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// How the components of an lvalue chain are fetched.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LocationFetch {
    /// The location's own value is wanted, so an uninitialized typed property raises.
    Read,
    /// The location is the CONTAINER of an array write, so an uninitialized typed property
    /// auto-initializes to an array instead of raising.
    ///
    /// PHP fetches every link of an lvalue chain in write mode: `$o->p['k'] = 1` initializes
    /// `$o->p` rather than complaining that it was never set, and the same holds nested
    /// (`$o->p['k']['j'] = 1`). Only the link the assignment finally lands on is a plain write;
    /// everything to its left is a container fetch.
    ArrayParent,
}

/// Evaluates the receiver and index components of one supported writable expression once.
fn evaluate_location(
    target: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedLocation, EvalStatus> {
    evaluate_location_with_fetch(target, LocationFetch::Read, context, scope, values)
}

/// Evaluates one writable expression's components under an explicit fetch mode.
fn evaluate_location_with_fetch(
    target: &EvalExpr,
    fetch: LocationFetch,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvaluatedLocation, EvalStatus> {
    match target {
        EvalExpr::ArrayGet { array, index } if is_globals_array(array) => {
            let name = eval_global_name(index, context, scope, values)?;
            let quiet = fetch == LocationFetch::ArrayParent || context.quiet_property_fetch();
            let current = read_global_value(&name, quiet, context, scope, values)?;
            Ok(EvaluatedLocation::GlobalVariable { name, current: Some(current) })
        }
        EvalExpr::LoadVar(name) => {
            let entry = scope_entry(context, scope, name).filter(|entry| entry.flags().is_visible());
            let current_is_borrowed = entry.is_some();
            let ownership = entry
                .as_ref()
                .map(|entry| entry.flags().ownership)
                .unwrap_or(ScopeCellOwnership::Owned);
            let current = entry.map_or_else(|| values.null(), |entry| Ok(entry.cell()))?;
            Ok(EvaluatedLocation::Variable {
                name: name.clone(),
                current: Some(current),
                current_is_borrowed,
                ownership,
            })
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let current = evaluate_property_component(object, property, fetch, context, values)?;
            Ok(EvaluatedLocation::Property {
                object,
                property: property.clone(),
                current: Some(current),
            })
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let current = evaluate_property_component(object, &property, fetch, context, values)?;
            Ok(EvaluatedLocation::Property {
                object,
                property,
                current: Some(current),
            })
        }
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => {
            let current = eval_static_property_get_result(class_name, property, context, values)?;
            Ok(EvaluatedLocation::StaticProperty {
                class_name: class_name.clone(),
                property: property.clone(),
                current: Some(current),
            })
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let current =
                eval_static_property_get_result(&class_name, property, context, values)?;
            Ok(EvaluatedLocation::StaticProperty {
                class_name,
                property: property.clone(),
                current: Some(current),
            })
        }
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => {
            let class_name = eval_expr(class_name, context, scope, values)?;
            let class_name = eval_dynamic_class_name(class_name, context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            let current =
                eval_static_property_get_result(&class_name, &property, context, values)?;
            Ok(EvaluatedLocation::StaticProperty {
                class_name,
                property,
                current: Some(current),
            })
        }
        EvalExpr::ArrayGet { array, index } => {
            // Everything to the left of an index is a CONTAINER, so it auto-initializes.
            let parent =
                evaluate_location_with_fetch(array, LocationFetch::ArrayParent, context, scope, values)?;
            let container = parent.current();
            let index = eval_expr(index, context, scope, values)?;
            let mut current = eval_array_get_result(container, index, context, values)?;
            // An array element about to be written THROUGH is copied first. Its container may be
            // shared (`clone $o` copies slots, not nested arrays), and the element's own count
            // only reflects the container, so writing it in place would reach every sharer.
            // The copy is consumed by the write-back, which stores it over the element.
            if fetch == LocationFetch::ArrayParent
                && !matches!(values.type_tag(container)?, EVAL_TAG_OBJECT)
                && matches!(values.type_tag(current)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC)
            {
                // An array holding PHP references is written in place: its reference slots are
                // keyed by this very payload, and a copy would silently unlink every one of them.
                if !context.array_has_element_aliases(current) {
                    // The copy replaces the read, whose own reference nothing else releases.
                    let copied = values.copy_value(current)?;
                    if !current.is_borrowed() {
                        eval_release_value(context, values, current)?;
                    }
                    current = copied;
                }
            }
            if values.type_tag(container)? == EVAL_TAG_OBJECT
                && eval_array_access_object_matches(container, context, values)?
            {
                return Ok(EvaluatedLocation::ArrayAccessElement {
                    object: container,
                    index,
                    current: Some(current),
                });
            }
            Ok(EvaluatedLocation::ArrayElement {
                parent: Box::new(parent),
                index,
                current: Some(current),
            })
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Reads one instance-property component of an lvalue chain under the given fetch mode.
fn evaluate_property_component(
    object: RuntimeCellHandle,
    property: &str,
    fetch: LocationFetch,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match fetch {
        LocationFetch::Read => eval_property_get_result(object, property, context, values),
        LocationFetch::ArrayParent => {
            eval_property_array_target_get_result(object, property, context, values)
        }
    }
}

/// Writes an evaluated value back through a previously materialized location.
/// Stores one ALREADY EVALUATED value into any writable PHP lvalue.
///
/// Ownership follows `eval_assign`'s convention, which is what every other writer here uses: the
/// caller does not release afterwards. The variable and element arms consume the reference, the
/// property arms retain, and mixing the two by releasing here would over-release the first two.
pub(in crate::interpreter) fn eval_store_value_in_lvalue(
    target: &EvalExpr,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let location = evaluate_plain_assignment_location(target, context, scope, values)?;
    write_location(location, value, false, context, scope, values)
}

fn write_location(
    location: EvaluatedLocation,
    value: RuntimeCellHandle,
    preserve_value_for_result: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match location {
        EvaluatedLocation::GlobalVariable { name, current } => {
            // A container update may transfer the observed copy itself to the slot.
            if let Some(current) = current.filter(|current| *current != value) {
                eval_release_value(context, values, current)?;
            }
            let stored = if preserve_value_for_result { values.retain(value)? } else { value };
            store_global_value(name, stored, context, scope, values)
        }
        EvaluatedLocation::Variable {
            name, ownership, ..
        } => {
            if let Some(target) = scope.reference_target(&name).cloned() {
                write_back_method_ref_target(&target, value, context, values)?;
            }
            let stored = if preserve_value_for_result {
                values.retain(value)?
            } else {
                value
            };
            for replaced in set_scope_cell(context, scope, name, stored, ownership, values)? {
                eval_release_value(context, values, replaced)?;
            }
            Ok(())
        }
        EvaluatedLocation::Property {
            object, property, ..
        } => {
            let result = eval_property_set_result(object, &property, value, context, values);
            // The receiver was evaluated by `eval_expr`: a variable read hands back a borrowed
            // handle, but any other receiver (`$a[0]->p = v`, `$o->inner->p = v`, `f()->p = v`)
            // is an owned reference this write is the last user of. Nothing released it, so the
            // receiver and everything it reaches stayed live until exit.
            let released = if object.is_borrowed() {
                Ok(())
            } else {
                eval_release_value(context, values, object)
            };
            result.and(released)
        }
        EvaluatedLocation::StaticProperty {
            class_name,
            property,
            ..
        } => {
            // The setter takes the reference it is handed. An assignment whose VALUE is used
            // (`return static::$validators = [...];`) also hands `value` back to its caller, so
            // without a reference of its own the static and the caller shared one: the caller's
            // release after a `foreach` freed the array the static still held.
            let stored = if preserve_value_for_result { values.retain(value)? } else { value };
            eval_static_property_set_result(&class_name, &property, stored, context, values)
        }
        EvaluatedLocation::ArrayElement { parent, index, .. } => {
            let container = if values.is_null(parent.current())? {
                values.array_new(1)?
            } else {
                parent.current()
            };
            let container = eval_array_set_target_for_index(container, index, values)?;
            if let Some(target) = eval_array_reference_key(index, values)?
                .and_then(|key| context.array_element_alias(container, &key).cloned())
            {
                return write_back_method_ref_target(&target, value, context, values);
            }
            // A property setter, and an enclosing element's `array_set` (the runtime's
            // `__elephc_eval_value_array_set` retains the value it stores), take a reference of
            // their own to the container, where a variable write consumes it: the container this
            // write fetched (a detached copy, or a fresh array) is then still this write's to
            // release. The key is only borrowed by the set.
            let parent_retains = matches!(
                *parent,
                EvaluatedLocation::Property { .. } | EvaluatedLocation::ArrayElement { .. }
            );
            let updated = values.array_set(container, index, value)?;
            if !index.is_borrowed() {
                eval_release_value(context, values, index)?;
            }
            write_location(*parent, updated, false, context, scope, values)?;
            if parent_retains {
                eval_release_value(context, values, updated)?;
            }
            Ok(())
        }
        EvaluatedLocation::ArrayAccessElement { object, index, .. } => {
            let result =
                eval_method_call_result(object, "offsetSet", vec![index, value], context, values)?;
            eval_release_value(context, values, result)
        }
    }
}

/// Resolves an lvalue expression's current value and persistent PHP reference target.
fn eval_reference_source(
    source: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(EvalReferenceTarget, RuntimeCellHandle), EvalStatus> {
    if let EvalExpr::ArrayAppendSlot { target } = source {
        // PHP CREATES the element and binds to it: after `$c = &$a[];` the array has one more
        // entry, holding null, and writing `$c` writes that entry. Appending the null first and
        // then resolving the source as `target[index]` reuses every rule the ordinary element
        // path already applies, instead of inventing a second way to name an element.
        let index = eval_array_append_slot_index(target, context, scope, values)?;
        let element = EvalExpr::ArrayGet {
            array: target.clone(),
            index: Box::new(EvalExpr::Const(EvalConst::Int(index))),
        };
        return eval_reference_source(&element, context, scope, values);
    }
    if eval_expr_is_call_shaped(source) {
        // A by-reference RETURN leaves its reference on the context, already retained. Taking it
        // transfers that ownership to this bind; a callee that was NOT declared by reference
        // leaves nothing, and php binds a copy there rather than failing.
        let by_ref_returns_before = BY_REF_RETURN_EXECUTIONS.load(std::sync::atomic::Ordering::Relaxed);
        let value = eval_expr(source, context, scope, values)?;
        if let Some((target, referenced)) = context.take_pending_return_reference() {
            eval_release_value(context, values, referenced)?;
            return Ok((target, value));
        }
        if BY_REF_RETURN_EXECUTIONS.load(std::sync::atomic::Ordering::Relaxed) == by_ref_returns_before {
            values.warning("Notice: Only variables should be assigned by reference")?;
        }
        return Ok((EvalReferenceTarget::Cell { cell: value }, value));
    }
    if let EvalExpr::LoadVar(source) = source {
        if let Some(target) = scope.reference_target(source).cloned() {
            let value = visible_scope_cell(context, scope, source).map_or_else(
                || values.null(),
                Ok,
            )?;
            return Ok((target, value));
        }
    }
    // `&$o->p` on an uninitialized typed property has PHP's own rule, and it is neither the
    // ordinary read's nor the quiet fetch's: non-nullable raises a DIFFERENT sentence, nullable
    // initializes to null and binds. The check runs before the generic by-reference
    // materializer, which would otherwise perform an ordinary read and raise the wrong message.
    match source {
        EvalExpr::PropertyGet { object, property } => {
            let receiver = eval_expr(object, context, scope, values)?;
            eval_property_reference_bind_precheck(receiver, property, context, values)?;
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            let receiver = eval_expr(object, context, scope, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_property_reference_bind_precheck(receiver, &property, context, values)?;
        }
        _ => {}
    }
    let (value, target) = eval_call_arg_value(source, context, scope, values)?;
    target
        .map(|target| (target, value))
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Appends a null element to one writable array lvalue and returns the integer key it took.
///
/// This is the element `$target[]` names as a reference SOURCE. PHP creates it before the bind,
/// which is observable: `count()` grows by one and the new entry reads back as null even if the
/// bound name is never written.
fn eval_array_append_slot_index(
    target: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<i64, EvalStatus> {
    let location = evaluate_location(target, context, scope, values)?;
    let current = location.current();
    let array = if values.is_null(current)? {
        values.array_new(1)?
    } else {
        current
    };
    let index = eval_array_append_key(array, values)?;
    let key = eval_int_value(index, values)?;
    let empty = values.null()?;
    let updated = values.array_set(array, index, empty)?;
    write_location(location, updated, false, context, scope, values)?;
    Ok(key)
}

/// Writes one by-reference assignment through an already evaluated array-element location.
fn write_reference_location(
    location: EvaluatedLocation,
    source_target: EvalReferenceTarget,
    source_value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let EvaluatedLocation::ArrayElement { parent, index, .. } = location else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    let container = if values.is_null(parent.current())? {
        values.array_new(1)?
    } else {
        parent.current()
    };
    let container = eval_array_set_target_for_index(container, index, values)?;
    let updated = values.array_set(container, index, source_value)?;
    let key = eval_array_reference_key(index, values)?.ok_or(EvalStatus::RuntimeFatal)?;
    let updated_identity = values.raw_value_word(updated)?;
    if crate::eval_trace::enabled() {
        eprintln!(
            "[elephc-eval-trace] phase=array_reference_bind identity={updated_identity:#x} key={key:?}",
        );
    }
    context.bind_array_element_alias(updated, key, source_target);
    write_location(*parent, updated, false, context, scope, values)
}
