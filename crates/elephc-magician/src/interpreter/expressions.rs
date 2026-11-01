//! Purpose:
//! Evaluates EvalIR expressions, match expressions, and function-like calls.
//!
//! Called from:
//! - `crate::interpreter::statements` for expression statements and expression-bearing statements.
//! - Eval builtin modules when they need to evaluate unevaluated argument expressions.
//!
//! Key details:
//! - PHP call argument evaluation order is preserved before binding or ABI-like materialization.
//! - Language constructs such as `eval`, `isset`, `empty`, and `unset` receive unevaluated expressions.

use super::*;

mod calls;
mod null_coalesce_assign;

pub(in crate::interpreter) use calls::*;
mod evaluation;
mod owned;

use owned::*;
pub(in crate::interpreter) use owned::{copy_scope_value, eval_output_expr};

pub(in crate::interpreter) use evaluation::{
    eval_array_access_object_matches, eval_array_get_result, eval_binary_result,
    eval_closure_object_expr, eval_dynamic_class_name, eval_dynamic_member_name, eval_match_expr,
    eval_new_object_result,
};
pub(in crate::interpreter) use evaluation::eval_object_array_cast_value;
pub(in crate::interpreter) use evaluation::eval_values_loosely_equal;
pub(in crate::interpreter) use evaluation::initialize_native_throwable_source;
use evaluation::*;
pub(in crate::interpreter) use null_coalesce_assign::{
    eval_assign,
    eval_array_append, eval_array_append_reference_bind, eval_array_append_result,
    eval_array_reference_bind, eval_by_ref_return, eval_reference_bind_result,
    eval_store_value_in_lvalue,
    eval_var_reference_bind,
};
use null_coalesce_assign::{
    eval_compound_assign, eval_null_coalesce_assign, eval_postfix_inc_dec,
};

/// Evaluates one expression to an opaque runtime-cell handle.
/// Returns whether this expression's value is an alias of storage that still owns it.
///
/// Variable reads and some assignment-like forms alias storage that still owns the result.
/// Plain assignment is different: `eval_assign` preserves an independent result owner as well
/// as the stored reference. Consumers must release that result after use.
///
/// `?:` and `??` are transparent, so they inherit the answer from whichever side can be taken.
/// A postfix `++`/`--` is deliberately absent: it retains before returning the old value.
pub(in crate::interpreter) fn eval_expr_result_aliases_storage(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::LoadVar(_)
        | EvalExpr::ReferenceBind { .. }
        | EvalExpr::ArrayAppendAssign { .. }
        | EvalExpr::CompoundAssign { .. }
        | EvalExpr::NullCoalesceAssign { .. }
        // `$a = $b = []`: the inner assignment returns the very cell it stored into `$b`, so the
        // outer store must take its own copy or both targets write through one array.
        | EvalExpr::Assign { .. }
        // A class-like constant cell is cached under the declaring class and handed out by
        // handle, exactly like a scope variable read: `eval_class_like_constant_cell` and
        // `context.enum_case()` return the SAME cell on every fetch. A static property fetch
        // is the same shape one level up (`context.static_property()`). Treating these as
        // fresh, independently-owned temporaries let a plain assignment (`$state =
        // self::STATE_B;`) bind a local directly to the shared cell; the local's NEXT
        // reassignment then released that shared cell as though the local had owned it,
        // corrupting the constant/property for every later read -- the state machine in
        // `switch ($state) { case self::STATE_A: ...; $state = self::STATE_B; break; ... }`
        // got stuck because the second transition freed the very cell the first one reused.
        | EvalExpr::ClassConstantFetch { .. }
        | EvalExpr::DynamicClassConstantFetch { .. }
        | EvalExpr::DynamicClassConstantNameFetch { .. }
        | EvalExpr::StaticPropertyGet { .. }
        | EvalExpr::DynamicStaticPropertyGet { .. }
        | EvalExpr::DynamicStaticPropertyNameGet { .. } => true,
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_expr_result_aliases_storage(then_branch.as_deref().unwrap_or(condition))
                || eval_expr_result_aliases_storage(else_branch)
        }
        EvalExpr::Unary { op: EvalUnaryOp::ErrorSuppress, expr } => {
            eval_expr_result_aliases_storage(expr)
        }
        EvalExpr::NullCoalesce { value, default } => {
            eval_expr_result_aliases_storage(value) || eval_expr_result_aliases_storage(default)
        }
        _ => false,
    }
}

/// Evaluates an expression and hands back a value safe to give to a CONSUMING sink -- one that
/// takes ownership of the reference outright instead of retaining its own, such as
/// `values.array_set` (`__rt_mixed_array_set` stores the pointer it is handed and frees it on
/// every path that does not keep it, never incrementing it first).
///
/// `eval_expr` on an aliasing expression (a `LoadVar`, a re-used property read, ...) hands back the
/// SAME cell its owner still holds. Passing that handle straight into a consuming sink gives the
/// sink the owner's only reference: the owner's next use of the same cell then walks into memory
/// the sink has since released. `eval_assign` already applies this rule before writing an aliasing
/// source into its target (`values.copy_value` when `eval_expr_result_aliases_storage` is true);
/// this is that same rule, shared for every OTHER writer that stores a raw `eval_expr` result into
/// a consuming sink. A method that stores one of its arguments into `$this->prop[]` and ALSO hands
/// that same argument to a freshly constructed object of another class is exactly the case this
/// covers: without the copy here, the array's element and the new object's promoted property ended
/// up sharing one physical reference between two logical owners, and freeing either one first left
/// the other pointing at released memory.
pub(in crate::interpreter) fn eval_expr_value_for_consuming_store(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let aliases_storage = eval_expr_result_aliases_storage(expr);
    let value = eval_expr(expr, context, scope, values)?;
    if aliases_storage {
        values.copy_value(value)
    } else {
        eval_detach_property_read(expr, value, context, values)
    }
}

/// Whether an expression reads an instance property.
pub(in crate::interpreter) fn eval_expr_reads_instance_property(expr: &EvalExpr) -> bool {
    matches!(
        expr,
        EvalExpr::PropertyGet { .. }
            | EvalExpr::NullsafePropertyGet { .. }
            | EvalExpr::DynamicPropertyGet { .. }
            | EvalExpr::NullsafeDynamicPropertyGet { .. }
    )
}

/// Gives a property read that is being STORED a cell of its own, so the store shares the
/// property's value the way php's copy-on-write does instead of sharing its cell.
///
/// `$id = $this->idTemplate;` must leave the property alone when `$id` is written next: php
/// shares the array until either side writes, then separates. The property read hands back the
/// property's own cell, whose array payload has a single owner, so the write through `$id`
/// landed in the property -- Doctrine's `ObjectHydrator` then saw every row's id appended to its
/// id TEMPLATE, never matched a row it had already hydrated, and returned the same entity once
/// per joined row. A fresh cell retains the payload, which makes the next write separate it.
/// The read's own reference is released, so ownership is unchanged whichever kind of cell the
/// read returned.
pub(in crate::interpreter) fn eval_detach_property_read(
    expr: &EvalExpr,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !eval_expr_reads_instance_property(expr) {
        return Ok(value);
    }
    let copy = values.copy_value(value)?;
    eval_release_value(context, values, value)?;
    Ok(copy)
}

/// Evaluates a boolean condition, consuming its temporary but not a borrowed scope value.
pub(in crate::interpreter) fn eval_truthy_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let value = eval_expr(expr, context, scope, values)?;
    let result = values.truthy(value);
    let cleanup = if eval_expr_result_aliases_storage(expr) {
        Ok(())
    } else {
        eval_release_value(context, values, value)
    };
    match (result, cleanup) {
        (Err(status), _) | (_, Err(status)) => Err(status),
        (Ok(result), Ok(())) => Ok(result),
    }
}

/// Evaluates one expression, naming it when it fails with nothing else to say.
///
/// A quarter of php-src's language corpus used to end in a bare `Fatal error: eval() runtime
/// failed` with no phase, no name and no line -- 225 of 2556 cases, which cannot even be GROUPED,
/// let alone fixed. The descriptions are first-writer-wins and the innermost frame writes first,
/// so a path that already names itself keeps its message and only the genuinely anonymous ones
/// pick up the expression kind here.
pub(in crate::interpreter) fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = eval_expr_with_result_ownership(expr, context, scope, values, false);
    if matches!(result, Err(EvalStatus::RuntimeFatal)) {
        note_eval_runtime_failure(
            format!("unsupported {} expression", eval_expr_kind(expr)),
            context,
        );
    }
    result
}

/// Names one expression variant for a diagnostic.
///
/// Reads the variant name off `Debug` rather than repeating sixty match arms that would drift from
/// the enum the first time somebody added a variant. Only ever reached on the failure path, which
/// is terminal, so rendering the node once costs nothing that matters.
fn eval_expr_kind(expr: &EvalExpr) -> String {
    // A CALL names the function it could not make. Without this every missing builtin produces
    // the same sentence -- "unsupported NamespacedCall expression" -- and the name is the whole
    // diagnostic: it is the difference between "somewhere in this file" and "preg_match".
    match expr {
        EvalExpr::NamespacedCall {
            name,
            fallback_name,
            ..
        } => {
            return if name.ends_with(fallback_name.as_str()) {
                format!("NamespacedCall {fallback_name}()")
            } else {
                format!("NamespacedCall {name}() (falling back to {fallback_name}())")
            };
        }
        EvalExpr::MethodCall { method, .. } => return format!("MethodCall ->{method}()"),
        _ => {}
    }
    let rendered = format!("{expr:?}");
    rendered
        .split(|ch: char| ch == '(' || ch == '{' || ch == ' ')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Evaluates a value that can be stored or returned independently of the current scope.
pub(in crate::interpreter) fn eval_owned_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_expr_with_result_ownership(expr, context, scope, values, true)?;
    if value.is_borrowed() {
        copy_scope_value(value, context, values)
    } else if eval_owned_result_may_share_storage(expr) {
        detach_owned_storage_cell(value, context, values)
    } else {
        Ok(value)
    }
}

/// Whether an owned expression result can still be the cell some storage holds.
///
/// A property read and an assignment (`$a = $b = []`) both answer with the cell they read or
/// stored, retained rather than copied; see `detach_owned_storage_cell`.
fn eval_owned_result_may_share_storage(expr: &EvalExpr) -> bool {
    match expr {
        EvalExpr::Assign { .. }
        | EvalExpr::CompoundAssign { .. }
        | EvalExpr::ArrayAppendAssign { .. }
        | EvalExpr::NullCoalesceAssign { .. } => true,
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            eval_owned_result_may_share_storage(then_branch.as_deref().unwrap_or(condition))
                || eval_owned_result_may_share_storage(else_branch)
        }
        EvalExpr::Unary { op: EvalUnaryOp::ErrorSuppress, expr } => {
            eval_owned_result_may_share_storage(expr)
        }
        EvalExpr::NullCoalesce { value, default } => {
            eval_owned_result_may_share_storage(value) || eval_owned_result_may_share_storage(default)
        }
        _ => eval_expr_reads_instance_property(expr),
    }
}

/// Evaluates one expression by variant, preserving expression evaluation order while optionally
/// owning the selected result.
fn eval_expr_with_result_ownership(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
    own_result: bool,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expr {
        EvalExpr::DeferredDefaultError(message) => throwables::eval_throw_error(message, context, values),
        EvalExpr::Array(elements) => {
            if elements
                .iter()
                .any(|element| {
                    native_default_array_spread(element).is_some() || matches!(
                        element,
                        EvalArrayElement::KeyValue { .. }
                            | EvalArrayElement::KeyReference { .. }
                    )
                })
            {
                eval_assoc_array(elements, context, scope, values)
            } else {
                eval_indexed_array(elements, context, scope, values)
            }
        }
        // `echo $a[];` is the fatal `Cannot use [] for reading` in php, so this node only ever
        // reaches the reference-source path and never produces a value.
        EvalExpr::ArrayAppendSlot { .. } => Err(EvalStatus::UnsupportedConstruct),
        EvalExpr::ArrayGet { array, index } => {
            if is_globals_array(array) {
                let name = eval_global_name(index, context, scope, values)?;
                return read_global_value(&name, context.quiet_property_fetch(), context, scope, values);
            }
            with_eval_operands(&[array, index], context, scope, values, |args, context, _, values| {
                let result = eval_array_get_result(args[0], args[1], context, values)?;
                if result.is_borrowed() { values.retain(result) } else { Ok(result) }
            })
        }
        EvalExpr::ArrayDestructureAssign { targets, value } => {
            eval_array_destructure_assign(targets, value, context, scope, values)
        }
        EvalExpr::Call { name, args } => eval_call(name, args, context, scope, values),
        EvalExpr::Cast { target, expr } => eval_cast_expr(target, expr, context, scope, values),
        EvalExpr::Const(value) => eval_const(value, values),
        EvalExpr::ConstFetch(name) => eval_const_fetch(name, context, values),
        EvalExpr::Closure {
            function,
            captures,
            is_static,
        } => eval_closure_expr(function, captures, *is_static, context, scope, values),
        EvalExpr::FunctionCallable {
            name,
            fallback_name,
        } => eval_function_callable_expr(name, fallback_name.as_deref(), context, values),
        EvalExpr::InvokableCallable { object } => {
            eval_invokable_callable_expr(object, context, scope, values)
        }
        EvalExpr::MethodCallable { object, method } => {
            eval_method_callable_expr(object, method, context, scope, values)
        }
        EvalExpr::StaticMethodCallable { class_name, method } => {
            eval_static_method_callable_expr(class_name, method, context, scope, values)
        }
        EvalExpr::DynamicStaticMethodCallable { class_name, method } => {
            eval_dynamic_static_method_callable_expr(class_name, method, context, scope, values)
        }
        EvalExpr::DynamicCall { callee, args } => {
            eval_dynamic_call(callee, args, context, scope, values)
        }
        EvalExpr::DynamicMethodCall {
            object,
            method,
            args,
        } => {
            with_eval_operands(&[object], context, scope, values, |receiver, context, scope, values| {
                let method = eval_dynamic_member_name(method, context, scope, values)?;
                with_eval_method_arguments(args, context, scope, values, |arguments, context, _, values| {
                    eval_method_call_result_with_evaluated_args(receiver[0], &method, arguments, context, values)
                })
            })
        }
        EvalExpr::DynamicNewObject { class_name, args } => {
            with_eval_operands(&[class_name], context, scope, values, |receiver, context, scope, values| {
                let class_name = eval_dynamic_class_name(receiver[0], context, values).map_err(|status| {
                    trace_dynamic_new_error("class_name", None, status, context)
                })?;
                with_eval_call_arguments(args, context, scope, values, |arguments, context, scope, values| {
                    eval_new_object_result(&class_name, arguments, context, scope, values).map_err(|status| {
                        trace_dynamic_new_error("construction", Some(&class_name), status, context)
                    })
                })
            })
        }
        EvalExpr::DynamicPropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, scope, values| {
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_get_with_result_ownership(object, &property, context, values, true)
            })
        }
        EvalExpr::DynamicStaticMethodCall {
            class_name,
            method,
            args,
        } => {
            with_eval_operands(&[class_name], context, scope, values, |receiver, context, scope, values| {
                let class_name = eval_dynamic_class_name(receiver[0], context, values)?;
                let method = eval_dynamic_member_name(method, context, scope, values)?;
                with_eval_method_arguments(args, context, scope, values, |arguments, context, scope, values| {
                    eval_static_method_call_result_from_scope(&class_name, &method, arguments, scope, context, values)
                })
            })
        }
        EvalExpr::DynamicStaticPropertyGet {
            class_name,
            property,
        } => with_eval_operands(&[class_name], context, scope, values, |classes, context, _, values| {
            let class_name = eval_dynamic_class_name(classes[0], context, values)?;
            eval_static_property_get_with_result_ownership(&class_name, property, context, values, own_result)
        }),
        EvalExpr::DynamicStaticPropertyNameGet {
            class_name,
            property,
        } => with_eval_operands(&[class_name], context, scope, values, |classes, context, scope, values| {
            let class_name = eval_dynamic_class_name(classes[0], context, values)?;
            let property = eval_dynamic_member_name(property, context, scope, values)?;
            eval_static_property_get_with_result_ownership(&class_name, &property, context, values, own_result)
        }),
        EvalExpr::DynamicClassConstantFetch {
            class_name,
            constant,
        } => with_eval_operands(&[class_name], context, scope, values, |classes, context, _, values| {
            let class_name = eval_dynamic_class_name(classes[0], context, values)?;
            eval_class_constant_fetch_result(&class_name, constant, context, values)
        }),
        EvalExpr::DynamicClassConstantNameFetch {
            class_name,
            constant,
        } => with_eval_operands(&[class_name], context, scope, values, |classes, context, scope, values| {
            let class_name = eval_dynamic_class_name(classes[0], context, values)?;
            let constant = eval_dynamic_member_name(constant, context, scope, values)?;
            eval_class_constant_fetch_result(&class_name, &constant, context, values)
        }),
        EvalExpr::DynamicClassNameFetch { class_name } => with_eval_operands(
            &[class_name],
            context,
            scope,
            values,
            |classes, context, _, values| {
                eval_dynamic_class_name_fetch_result(classes[0], context, values)
            },
        ),
        EvalExpr::Include {
            path,
            required,
            once,
        } => eval_include_expr(path, *required, *once, context, scope, values),
        EvalExpr::InstanceOf { value, target } => {
            eval_instanceof_expr(value, target, context, scope, values)
        }
        EvalExpr::LoadVar(name) => match visible_scope_cell(context, scope, name) {
            Some(cell) => {
                if own_result {
                    copy_scope_value(cell, context, values)
                } else {
                    Ok(cell)
                }
            }
            None => {
                if super::superglobals::seed_cli_superglobal_on_read(name, context, scope, values)? {
                    if let Some(cell) = visible_scope_cell(context, scope, name) {
                        return if own_result {
                            copy_scope_value(cell, context, values)
                        } else {
                            Ok(cell)
                        };
                    }
                }
                // php: `Warning: Undefined variable $x`, and the value is still null -- an unset
                // variable is a WARNING, never an error. 18 cases in the php-src sweep printed
                // the right value and no diagnostic at all.
                //
                // `@` already suppresses through `errors_suppressed()`. `??` and `isset()` are
                // the other two readers php keeps quiet, and they suppress around their own
                // operand rather than being special-cased here, because the rule is theirs.
                if !context.errors_suppressed() {
                    values.warning(&format!("Undefined variable ${name}"))?;
                }
                values.null()
            }
        },
        // `$$name` / `${expr}`: read the variable the value names, with the same undefined-name
        // warning a literal name gets.
        EvalExpr::DynamicVar(name) => {
            let name = eval_dynamic_member_name(name, context, scope, values)?;
            eval_expr(&EvalExpr::LoadVar(name), context, scope, values)
        }
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::Match {
            subject,
            arms,
            default,
        } => eval_match_expr(subject, arms, default.as_deref(), context, scope, values, own_result),
        EvalExpr::Clone(object) => with_eval_operands(
            &[object.as_ref()],
            context,
            scope,
            values,
            |args, context, _, values| eval_object_clone_result(args[0], context, values),
        ),
        EvalExpr::NamespacedCall {
            name,
            fallback_name,
            args,
        } => eval_namespaced_call(name, fallback_name, args, context, scope, values),
        EvalExpr::NamespacedConstFetch {
            name,
            fallback_name,
        } => eval_namespaced_const_fetch(name, fallback_name, context, values),
        EvalExpr::NewObject { class_name, args } => {
            with_eval_call_arguments(args, context, scope, values, |arguments, context, scope, values| {
                let class_name = eval_new_object_class_name(class_name, context).map_err(|status| {
                    trace_new_object_error("class_name", class_name, status, context)
                })?;
                eval_new_object_result(&class_name, arguments, context, scope, values)
            })
        }
        EvalExpr::NewAnonymousClass { class, args } => {
            ensure_eval_anonymous_class_decl(class, context, scope, values)?;
            with_eval_call_arguments(args, context, scope, values, |arguments, context, scope, values| {
                let class = context.class(class.name()).cloned().ok_or(EvalStatus::RuntimeFatal)?;
                eval_dynamic_class_new_object(&class, arguments, context, scope, values)
            })
        }
        EvalExpr::StaticMethodCall {
            class_name,
            method,
            args,
        } => with_eval_method_arguments(args, context, scope, values, |evaluated_args, context, scope, values| {
            let concrete = ensure_generic_class(class_name, Some(&evaluated_args), method, context, scope, values)?;
            eval_static_method_call_result_from_scope(
                &concrete,
                method,
                evaluated_args,
                scope,
                context,
                values,
            )
        }),
        EvalExpr::StaticPropertyGet {
            class_name,
            property,
        } => {
            let concrete = ensure_generic_class(class_name, None, "", context, scope, values)?;
            eval_static_property_get_with_result_ownership(&concrete, property, context, values, own_result)
        },
        EvalExpr::ClassConstantFetch {
            class_name,
            constant,
        } => {
            let concrete = ensure_generic_class(class_name, None, "", context, scope, values)?;
            let result = eval_class_constant_fetch_result(&concrete, constant, context, values);
            if let Err(status) = &result {
                if crate::eval_trace::enabled() {
                    eprintln!(
                        "[elephc-eval-trace] phase=class_constant_error class={class_name:?} constant={constant:?} status={status:?}"
                    );
                }
            }
            result
        }
        EvalExpr::ClassNameFetch { class_name } => {
            let concrete = ensure_generic_class(class_name, None, "", context, scope, values)?;
            eval_class_name_fetch_result(&concrete, context, values)
        }
        EvalExpr::MethodCall {
            object,
            method,
            args,
        } => with_eval_operands(&[object], context, scope, values, |receiver, context, scope, values| {
            with_eval_method_arguments(args, context, scope, values, |arguments, context, _, values| {
                eval_method_call_result_with_evaluated_args(receiver[0], method, arguments, context, values)
            })
        }),
        EvalExpr::NullsafeMethodCall {
            object,
            method,
            args,
        } => with_eval_operands(&[object], context, scope, values, |receiver, context, scope, values| {
            if values.is_null(receiver[0])? {
                return values.null();
            }
            with_eval_method_arguments(args, context, scope, values, |arguments, context, _, values| {
                eval_method_call_result_with_evaluated_args(receiver[0], method, arguments, context, values)
            })
        }),
        EvalExpr::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => with_eval_operands(&[object], context, scope, values, |receiver, context, scope, values| {
            if values.is_null(receiver[0])? {
                return values.null();
            }
            let method = eval_dynamic_member_name(method, context, scope, values)?;
            with_eval_method_arguments(args, context, scope, values, |arguments, context, _, values| {
                eval_method_call_result_with_evaluated_args(receiver[0], &method, arguments, context, values)
            })
        }),
        EvalExpr::NullCoalesce { value, default } => {
            // `$x ?? $default` asks whether `$x` is there and must not complain that it is not:
            // php raises no `Undefined variable` for the left operand of `??`. Suppressing around
            // it is exactly that rule, and reuses the depth counter `@` already maintains.
            //
            // Suppression alone is not enough, because an uninitialized typed property THROWS
            // rather than warning, and `@` does not stop a throw: `$o->t ?? 'D'` died with
            // `Typed property C::$t must not be accessed before initialization` where php
            // answers `'D'`. The quiet-fetch mode is the second half of the same rule.
            context.push_error_suppression();
            context.push_quiet_property_fetch();
            let value = if let EvalExpr::LoadVar(name) = value.as_ref() {
                match visible_scope_cell(context, scope, name) {
                    Some(value) if own_result => copy_scope_value(value, context, values),
                    Some(value) => Ok(value),
                    None => values.null(),
                }
            } else {
                eval_expr_with_result_ownership(value, context, scope, values, own_result)
            };
            context.pop_quiet_property_fetch();
            context.pop_error_suppression();
            let value = value?;
            if values.is_null(value)? {
                release_expr_result(value, context, values)?;
                eval_expr(default, context, scope, values)
            } else {
                Ok(value)
            }
        }
        EvalExpr::NullCoalesceAssign { target, default } => {
            eval_null_coalesce_assign(target, default, context, scope, values)
        }
        EvalExpr::CompoundAssign { target, op, value } => {
            eval_compound_assign(target, *op, value, context, scope, values)
        }
        EvalExpr::PostfixIncDec { target, increment } => {
            eval_postfix_inc_dec(target, *increment, context, scope, values)
        }
        EvalExpr::Assign { target, value } => {
            eval_assign(target, value, context, scope, values)
        }
        EvalExpr::ReferenceBindAssign { target, source } => {
            eval_var_reference_bind(target, source, context, scope, values)?;
            eval_expr(&EvalExpr::LoadVar(target.clone()), context, scope, values)
        }
        EvalExpr::ArrayAppendAssign { target, value } => {
            eval_array_append_result(target, value, context, scope, values)
        }
        EvalExpr::ReferenceBind { target, source } => {
            eval_reference_bind_result(target, source, context, scope, values)
        }
        EvalExpr::NullsafePropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, _scope, values| {
                if values.is_null(object)? { return values.null(); }
                eval_property_get_with_result_ownership(object, property, context, values, true)
            })
        }
        EvalExpr::NullsafeDynamicPropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, scope, values| {
                if values.is_null(object)? { return values.null(); }
                let property = eval_dynamic_member_name(property, context, scope, values)?;
                eval_property_get_with_result_ownership(object, &property, context, values, true)
            })
        }
        EvalExpr::PropertyGet { object, property } => {
            with_owned_receiver(object, context, scope, values, |object, context, _scope, values| {
                eval_property_get_with_result_ownership(object, property, context, values, true)
            })
        }
        EvalExpr::Print(inner) => {
            eval_output_expr(inner, context, scope, values)?;
            values.int(1)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            if let Some(then_branch) = then_branch {
                let selected = if eval_condition(condition, context, scope, values)? {
                    then_branch
                } else {
                    else_branch
                };
                return eval_expr(selected, context, scope, values);
            }
            let condition = eval_owned_expr(condition, context, scope, values)?;
            let truthy = values.truthy(condition);
            if matches!(truthy, Ok(true)) { return Ok(condition); }
            let released = eval_release_value(context, values, condition);
            truthy?;
            released?;
            eval_expr(else_branch, context, scope, values)
        }
        EvalExpr::Throw(inner) => {
            let thrown = eval_owned_expr(inner, context, scope, values)?;
            if values.type_tag(thrown)? != EVAL_TAG_OBJECT {
                return Err(EvalStatus::RuntimeFatal);
            }
            context.set_pending_throw(thrown);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalExpr::Unary {
            op: EvalUnaryOp::ErrorSuppress,
            expr,
        } => {
            context.push_error_suppression();
            crate::runtime_hooks::diag_suppression(true);
            let result = eval_expr_with_result_ownership(expr, context, scope, values, own_result);
            crate::runtime_hooks::diag_suppression(false);
            context.pop_error_suppression();
            result
        }
        EvalExpr::Unary { op, expr } => eval_unary_expr(*op, expr, context, scope, values),
        EvalExpr::Binary { op, left, right } => eval_binary_expr(*op, left, right, context, scope, values),
    }
}

/// Emits the failing dynamic-construction stage when opt-in eval tracing is enabled.
fn trace_dynamic_new_error(
    stage: &str,
    class_name: Option<&str>,
    status: EvalStatus,
    context: &ElephcEvalContext,
) -> EvalStatus {
    if crate::eval_trace::enabled() {
        let call_site = context.call_site();
        eprintln!(
            "[elephc-eval-trace] phase=dynamic_new_error stage={stage} class={class_name:?} status={status:?} file={:?} line={}",
            call_site.0,
            call_site.2,
        );
    }
    status
}
