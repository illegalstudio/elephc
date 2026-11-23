//! Purpose:
//! Late-static calls whose lexical target is a private static method of the scope class.
//!
//! Called from:
//! - `crate::ir_lower::expr::static_method_calls::lower_static_method_call_bound()`.
//!
//! Key details:
//! - A private static method is not inherited, so it has no static-vtable slot and
//!   `static::m()` used to bind `A::m()` for every called class. php-src looks `m` up in the
//!   late-bound class instead: a subclass that redeclares `m` runs its own method, or raises
//!   `Error` when that redeclaration is private to it.
//! - A redeclared instance method runs on `$this` from an instance method and raises the
//!   non-static `Error` from a static one. A private redeclaration falls back to the called
//!   class's `__call()` (with `$this`) or `__callStatic()`, as php-src's static lookup does.
//! - An abstract redeclaration has no body: an abstract called class raises php-src's
//!   "Cannot call abstract method" `Error`, and a caller with `$this` gets no arm for it.
//! - Only called classes whose lookup differs from the scope's method get a dispatch arm. A
//!   call whose method no subclass redeclares lowers exactly as before.
//! - Each arm lowers the arguments against the signature of the method it calls, so a
//!   redeclaration with its own parameter types or defaults binds them like a direct call.

use std::borrow::Cow;

use super::*;

/// What `static::m()` resolves to for one group of late-bound called classes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PrivateLateStaticArm {
    /// The called class inherits a non-private static `m` from `class`: call it, forwarding
    /// `static`.
    Call(String),
    /// The called class inherits a non-private instance `m` from `class` and the caller has
    /// `$this`: call it on `$this`.
    CallOnThis(String),
    /// The called class's `m` is private to it and it inherits `__callStatic()` from `class`.
    CallStaticMagic(String),
    /// The called class's `m` is private to it, the caller has `$this` and the called class
    /// inherits `__call()` from `class`.
    CallMagicOnThis(String),
    /// The called class sees a private `m` declared by `class`, which the scope cannot call.
    PrivateError(String),
    /// The called class inherits an instance `m` from `class` and the caller has no `$this`.
    NonStaticError(String),
    /// The called class is abstract and inherits an abstract `m` from `class`, which has no
    /// body to call.
    AbstractError(String),
}

/// The first declaration of the method php-src's lookup in the called class finds below the
/// scope.
struct Redeclaration {
    class: String,
    is_static: bool,
    private: bool,
    is_abstract: bool,
}

/// Lowers `static::m()` when `m` is a private static method of the lexical class and some
/// subclass redeclares it.
///
/// Returns `None` when the ordinary lowering already matches php-src: the receiver is not
/// `static`, the scope's method is not private, or no subclass redeclares it. `$this->m()`
/// reaches the static path with an object receiver and keeps the scope's private method, as
/// php-src's instance-call lookup does.
pub(super) fn try_lower_private_late_static_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
    object_receiver: Option<crate::ir::ValueId>,
) -> Option<LoweredValue> {
    if object_receiver.is_some() || !matches!(receiver, StaticReceiver::Static) {
        return None;
    }
    let scope = ctx.current_class.clone()?;
    let arms = private_late_static_arms(ctx, &scope, method);
    if arms.is_empty() {
        return None;
    }
    if let Some(dispatch) = same_signature_dispatch(ctx, receiver, method, &arms) {
        return Some(lower_static_method_call_bound(
            ctx, receiver, method, args, expr, None, StaticCallBinding::Dispatched(&dispatch),
        ));
    }
    Some(lower_private_late_static_dispatch(ctx, receiver, method, args, expr, &scope, arms))
}

/// Encodes the arms of a dispatch codegen can run inside one call, as `Class=id,id;Class=id`,
/// when every arm calls a static redeclaration whose signature is the scope method's.
///
/// The arguments then bind identically for every called class, so the call keeps one
/// argument evaluation and one result, and only the branch to the callee depends on the called
/// class. The EIR optimizer sees an ordinary call, as for a method no subclass redeclares.
fn same_signature_dispatch(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    arms: &[(PrivateLateStaticArm, Vec<u64>)],
) -> Option<String> {
    let reference_assignment = ctx
        .reference_call_context
        .as_ref()
        .is_some_and(|context| context.depth == ctx.expression_depth);
    if reference_assignment {
        return None;
    }
    let key = php_symbol_key(method);
    let scope_signature = static_method_implementation_signature(ctx, receiver, method)?;
    let scope_class = static_receiver_class_name(ctx, receiver)?;
    let has_late_static_return = |class_name: &str| {
        ctx.classes.get(class_name).is_none_or(|info| {
            info.late_static_static_method_returns.contains_key(&key)
        })
    };
    if has_late_static_return(&scope_class) || scope_signature.deprecation.is_some() {
        return None;
    }
    let mut encoded = Vec::with_capacity(arms.len());
    for (arm, class_ids) in arms {
        let PrivateLateStaticArm::Call(class_name) = arm else {
            return None;
        };
        let signature = static_method_implementation_signature(
            ctx, &static_receiver_for_class_name(class_name), method,
        )?;
        let same = signature.params == scope_signature.params
            && signature.param_type_exprs == scope_signature.param_type_exprs
            // A default such as `self::X` names each declaring class, so only scalar literal
            // defaults bind alike.
            && signature.defaults == scope_signature.defaults
            && signature.defaults.iter().flatten().all(|default| matches!(default.kind,
                ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_)
                    | ExprKind::BoolLiteral(_) | ExprKind::Null))
            && signature.return_type == scope_signature.return_type
            && signature.declared_return == scope_signature.declared_return
            && signature.by_ref_return == scope_signature.by_ref_return
            && signature.ref_params == scope_signature.ref_params
            && signature.declared_params == scope_signature.declared_params
            && signature.variadic == scope_signature.variadic
            && signature.deprecation.is_none()
            && signature.is_generator == scope_signature.is_generator;
        if !same || has_late_static_return(class_name) {
            return None;
        }
        let ids = class_ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        encoded.push(format!("{class_name}={ids}"));
    }
    Some(encoded.join(";"))
}

/// Groups the descendants of `scope` whose late-bound lookup of `method` differs from the
/// scope's own private static method, sorted by class id for deterministic dispatch.
fn private_late_static_arms(
    ctx: &LoweringContext<'_, '_>,
    scope: &str,
    method: &str,
) -> Vec<(PrivateLateStaticArm, Vec<u64>)> {
    let key = php_symbol_key(method);
    let Some(scope_info) = ctx.classes.get(scope) else {
        return Vec::new();
    };
    let scope_private = scope_info.static_method_visibilities.get(&key) == Some(&Visibility::Private)
        && scope_info.static_method_impl_classes.get(&key).map(String::as_str) == Some(scope);
    if !scope_private {
        return Vec::new();
    }
    let mut classes = ctx
        .classes
        .iter()
        .filter(|(name, _)| name.as_str() != scope)
        .map(|(name, info)| (name.as_str(), info.class_id))
        .collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_id)| *class_id);
    // `static::m()` in a method with `$this` is a call on `$this` for an instance method, and
    // `$this` is an instance of the called class.
    let has_this = ctx.local_slots.contains_key("this");
    let mut arms: Vec<(PrivateLateStaticArm, Vec<u64>)> = Vec::new();
    for (class_name, class_id) in classes {
        let Some(found) = late_bound_lookup(ctx, class_name, scope, &key) else {
            continue;
        };
        let arm = if found.is_abstract {
            // Only an abstract called class can find an abstract `m`, and `$this` is never an
            // instance of one, so the arm is unreachable from a caller with `$this`.
            if has_this {
                continue;
            }
            PrivateLateStaticArm::AbstractError(found.class)
        } else if found.private {
            magic_arm(ctx, class_name, has_this)
                .unwrap_or(PrivateLateStaticArm::PrivateError(found.class))
        } else if found.is_static {
            PrivateLateStaticArm::Call(found.class)
        } else if has_this {
            PrivateLateStaticArm::CallOnThis(found.class)
        } else {
            PrivateLateStaticArm::NonStaticError(found.class)
        };
        match arms.iter_mut().find(|(existing, _)| *existing == arm) {
            Some((_, ids)) => ids.push(class_id),
            None => arms.push((arm, vec![class_id])),
        }
    }
    arms
}

/// Returns the magic method php-src calls instead of a private `m` the scope cannot reach:
/// `__call()` when the caller has `$this`, else `__callStatic()`, as the called class inherits
/// them.
fn magic_arm(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    has_this: bool,
) -> Option<PrivateLateStaticArm> {
    let info = ctx.classes.get(class_name)?;
    let call = php_symbol_key("__call");
    if has_this && info.methods.contains_key(&call) {
        let owner = info.method_impl_classes.get(&call).map_or(class_name, String::as_str);
        return Some(PrivateLateStaticArm::CallMagicOnThis(owner.to_string()));
    }
    let call_static = php_symbol_key("__callStatic");
    if info.static_methods.contains_key(&call_static) {
        let owner = info
            .static_method_impl_classes
            .get(&call_static)
            .map_or(class_name, String::as_str);
        return Some(PrivateLateStaticArm::CallStaticMagic(owner.to_string()));
    }
    None
}

/// Walks from `class_name` up to `scope` and returns the redeclaration of `key` that php-src's
/// lookup in the called class finds, or `None` when it finds the scope's own method (or when
/// `class_name` does not descend from `scope`).
fn late_bound_lookup(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    scope: &str,
    key: &str,
) -> Option<Redeclaration> {
    let mut cursor = class_name;
    let mut found = None;
    while cursor != scope {
        let info = ctx.classes.get(cursor)?;
        if found.is_none() {
            let abstract_static = info.abstract_static_methods.contains(key)
                && info.static_method_declaring_classes.get(key).map(String::as_str) == Some(cursor);
            if (abstract_static
                || info.static_method_impl_classes.get(key).map(String::as_str) == Some(cursor))
                && info.static_methods.contains_key(key)
            {
                found = Some(Redeclaration {
                    class: cursor.to_string(),
                    is_static: true,
                    private: info.static_method_visibilities.get(key) == Some(&Visibility::Private),
                    is_abstract: abstract_static,
                });
            } else if info.methods.contains_key(key)
                && (info.method_impl_classes.get(key).map(String::as_str) == Some(cursor)
                    || info.method_declaring_classes.get(key).map(String::as_str) == Some(cursor))
            {
                found = Some(Redeclaration {
                    class: cursor.to_string(),
                    is_static: false,
                    private: info.method_visibilities.get(key) == Some(&Visibility::Private),
                    is_abstract: info.abstract_methods.contains(key),
                });
            }
        }
        cursor = info.parent.as_deref()?;
    }
    found
}

/// Returns the signature of the method one dispatch arm calls, or `None` for an arm that throws.
fn arm_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    arm: &PrivateLateStaticArm,
    method: &str,
) -> Option<&'a FunctionSig> {
    match arm {
        PrivateLateStaticArm::Call(class_name) => {
            static_method_implementation_signature(ctx, &static_receiver_for_class_name(class_name), method)
        }
        PrivateLateStaticArm::CallOnThis(class_name) => {
            class_method_signature(ctx, class_name, &php_symbol_key(method))
        }
        PrivateLateStaticArm::CallStaticMagic(class_name) => static_method_implementation_signature(
            ctx, &static_receiver_for_class_name(class_name), "__callStatic",
        ),
        PrivateLateStaticArm::CallMagicOnThis(class_name) => {
            class_method_signature(ctx, class_name, &php_symbol_key("__call"))
        }
        PrivateLateStaticArm::PrivateError(_)
        | PrivateLateStaticArm::NonStaticError(_)
        | PrivateLateStaticArm::AbstractError(_) => None,
    }
}

/// Returns the result type one dispatch arm's call is lowered with, or `None` for an arm that
/// throws.
fn arm_result_type(
    ctx: &LoweringContext<'_, '_>,
    arm: &PrivateLateStaticArm,
    method: &str,
) -> Option<PhpType> {
    match arm {
        PrivateLateStaticArm::Call(class_name) => lexical_static_method_call_expr_type_for_ir(
            ctx, &static_receiver_for_class_name(class_name), method,
        ),
        PrivateLateStaticArm::CallStaticMagic(class_name) => {
            lexical_static_method_call_expr_type_for_ir(
                ctx, &static_receiver_for_class_name(class_name), "__callStatic",
            )
        }
        PrivateLateStaticArm::CallOnThis(class_name)
        | PrivateLateStaticArm::CallMagicOnThis(class_name) => {
            let method = match arm {
                PrivateLateStaticArm::CallOnThis(_) => php_symbol_key(method),
                _ => php_symbol_key("__call"),
            };
            let nominal = class_method_signature(ctx, class_name, &method).map(|signature| {
                normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type))
            })?;
            Some(match ctx
                .classes
                .get(class_name)
                .and_then(|info| info.late_static_method_returns.get(&method))
            {
                Some(return_type) => late_static_return_type_for_ir(ctx, return_type, class_name),
                None => nominal,
            })
        }
        PrivateLateStaticArm::PrivateError(_)
        | PrivateLateStaticArm::NonStaticError(_)
        | PrivateLateStaticArm::AbstractError(_) => None,
    }
}

/// Returns the result type of `static::m()` when it dispatches over redeclarations of a private
/// static method, or `None` when the call binds the scope's method alone.
///
/// The dispatch stores every arm into one merge temporary of this type, and callers that pick
/// storage before lowering the call (array literals, merges) must see the same type: a
/// redeclaration returning `int` beside a private method returning `string` joins to `mixed`.
pub(in crate::ir_lower) fn private_late_static_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
) -> Option<PhpType> {
    if !matches!(receiver, StaticReceiver::Static) {
        return None;
    }
    let scope = ctx.current_class.as_deref()?;
    let arms = private_late_static_arms(ctx, scope, method);
    if arms.is_empty() {
        return None;
    }
    Some(joined_result_type(ctx, receiver, method, &arms))
}

/// Joins the scope method's result type with every called redeclaration's: the shared type when
/// they agree (`void` when all are `void`), `mixed` otherwise.
fn joined_result_type(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    arms: &[(PrivateLateStaticArm, Vec<u64>)],
) -> PhpType {
    let fallback_type = lexical_static_method_call_expr_type_for_ir(ctx, receiver, method)
        .unwrap_or(PhpType::Mixed);
    let same = arms.iter().all(|(arm, _)| {
        arm_result_type(ctx, arm, method).is_none_or(|ty| ty == fallback_type)
    });
    if same { fallback_type } else { PhpType::Mixed }
}

/// Emits the called-class dispatch: one compare per affected class, one arm per resolution,
/// and the scope's own method for every other called class.
fn lower_private_late_static_dispatch(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    expr: &Expr,
    scope: &str,
    arms: Vec<(PrivateLateStaticArm, Vec<u64>)>,
) -> LoweredValue {
    let span = expr.span;
    let result_type = joined_result_type(ctx, receiver, method, &arms);
    // `$r =& static::m()`: each arm adopts its own reference cell into the assignment's staging
    // (a by-value arm gets a fresh cell), so no arm can leave the binding to another's lease.
    let reference_assignment = ctx
        .reference_call_context
        .as_ref()
        .is_some_and(|context| context.depth == ctx.expression_depth);
    if reference_assignment {
        let fallback = static_method_implementation_signature(ctx, receiver, method);
        let targets = arms.iter().filter_map(|(arm, _)| arm_signature(ctx, arm, method));
        if fallback.into_iter().chain(targets).any(|signature| signature.by_ref_return
            && !signature.return_type.reference_payload_compatible(&result_type))
        {
            crate::ir_lower::diagnostics::refuse(span,
                "Unsupported reference assignment: a late-bound redeclaration returns a \
                 different reference payload than the private method");
        }
    }
    let temp_name = (!reference_assignment && result_type != PhpType::Void)
        .then(|| ctx.declare_owned_hidden_temp(result_type.clone()));
    let split_initialized = ctx.initialized_slots_snapshot();
    let merge = ctx.builder.create_named_block("private_static.merge", Vec::new());
    let called_class_id = ctx.emit_value(
        Op::LoadCalledClassId,
        Vec::new(),
        None,
        PhpType::Int,
        Op::LoadCalledClassId.default_effects(),
        Some(span),
    );
    let mut arm_blocks = Vec::new();
    for (arm, class_ids) in arms {
        let block = ctx.builder.create_named_block("private_static.arm", Vec::new());
        for class_id in class_ids {
            let class_id_value = ctx.emit_value(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(class_id as i64)),
                PhpType::Int,
                Op::ConstI64.default_effects(),
                Some(span),
            );
            let is_called_class = ctx.emit_value(
                Op::ICmp,
                vec![called_class_id.value, class_id_value.value],
                Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
                PhpType::Bool,
                Op::ICmp.default_effects(),
                Some(span),
            );
            let skip = ctx.builder.create_named_block("private_static.skip", Vec::new());
            ctx.builder.terminate(Terminator::CondBr {
                cond: is_called_class.value,
                then_target: block,
                then_args: Vec::new(),
                else_target: skip,
                else_args: Vec::new(),
            });
            ctx.builder.position_at_end(skip);
        }
        arm_blocks.push((block, arm));
    }
    let mut reachable_initialized = Vec::new();
    // -- every other called class runs the scope's own private method --
    let fallback_type = lexical_static_method_call_expr_type_for_ir(ctx, receiver, method)
        .unwrap_or(PhpType::Mixed);
    if reference_assignment {
        ctx.reference_call_context.as_mut().unwrap().adopted = false;
    }
    let value = lower_static_method_call_bound(
        ctx, receiver, method, args, expr, None, StaticCallBinding::Resolved,
    );
    finish_private_late_static_arm(
        ctx, temp_name.as_deref(), &result_type, value, merge, &mut reachable_initialized,
        reference_assignment, &fallback_type, span,
    );
    for (block, arm) in arm_blocks {
        ctx.builder.position_at_end(block);
        ctx.restore_initialized_slots(split_initialized.clone());
        let (target_class, target_method, binding, call_args) = match &arm {
            PrivateLateStaticArm::PrivateError(class_name) => {
                // php-src refuses the call before evaluating its arguments.
                crate::ir_lower::stmt::lower_throw_access_error(
                    ctx,
                    &format!("Call to private method {class_name}::{method}() from scope {scope}"),
                    span,
                );
                continue;
            }
            PrivateLateStaticArm::NonStaticError(class_name) => {
                crate::ir_lower::stmt::lower_throw_access_error(
                    ctx,
                    &format!("Non-static method {class_name}::{method}() cannot be called statically"),
                    span,
                );
                continue;
            }
            PrivateLateStaticArm::AbstractError(class_name) => {
                crate::ir_lower::stmt::lower_throw_access_error(
                    ctx,
                    &format!("Cannot call abstract method {class_name}::{method}()"),
                    span,
                );
                continue;
            }
            PrivateLateStaticArm::Call(class_name) => {
                (class_name.as_str(), method, StaticCallBinding::Forwarded, Cow::Borrowed(args))
            }
            PrivateLateStaticArm::CallOnThis(class_name) => {
                (class_name.as_str(), method, StaticCallBinding::ForwardedThis, Cow::Borrowed(args))
            }
            PrivateLateStaticArm::CallStaticMagic(class_name) => (
                class_name.as_str(), "__callStatic", StaticCallBinding::Forwarded,
                Cow::Owned(magic_call_args(method, args, span)),
            ),
            PrivateLateStaticArm::CallMagicOnThis(class_name) => (
                class_name.as_str(), "__call", StaticCallBinding::ForwardedThis,
                Cow::Owned(magic_call_args(method, args, span)),
            ),
        };
        let target = static_receiver_for_class_name(target_class);
        // The checker bound the arguments against the private method only. A redeclaration
        // with other parameters throws its own binding error, and only when a class that
        // selects it is the called class.
        let signature = arm_signature(ctx, &arm, method).cloned();
        if let Some(signature) = signature.as_ref() {
            if super::selected_method_arguments::reject_invalid_at_call_site(
                ctx, target_class, target_method, signature, &call_args, span,
            ) {
                continue;
            }
        }
        let arm_type = arm_result_type(ctx, &arm, method).unwrap_or_else(|| result_type.clone());
        if reference_assignment {
            ctx.reference_call_context.as_mut().unwrap().adopted = false;
        }
        let value = lower_static_method_call_bound(
            ctx, &target, target_method, &call_args, expr, None, binding,
        );
        finish_private_late_static_arm(
            ctx, temp_name.as_deref(), &result_type, value, merge,
            &mut reachable_initialized, reference_assignment, &arm_type, span,
        );
    }
    ctx.builder.position_at_end(merge);
    let mut merged = reachable_initialized.into_iter();
    let mut initialized = merged.next().unwrap_or_else(|| split_initialized.clone());
    for arm_initialized in merged {
        initialized.retain(|slot| arm_initialized.contains(slot));
    }
    ctx.restore_initialized_slots(initialized);
    if reference_assignment {
        // Every arm that reaches the merge adopted a cell into the assignment's staging.
        ctx.reference_call_context.as_mut().unwrap().adopted = true;
    }
    match temp_name {
        Some(temp_name) => take_owned_temp(ctx, &temp_name, span),
        None => lower_null(ctx, expr),
    }
}

/// Packs a call php-src hands to `__call()`/`__callStatic()` into the method name and the
/// argument array. Named arguments keep their names as string keys after the positional ones,
/// as php-src passes them.
fn magic_call_args(method: &str, args: &[Expr], span: Span) -> Vec<Expr> {
    let named = args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }));
    let spread = args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    let arguments = if named && !spread {
        let mut position = 0;
        let pairs = args
            .iter()
            .map(|arg| match &arg.kind {
                ExprKind::NamedArg { name, value } => (
                    Expr::new(ExprKind::StringLiteral(name.clone()), arg.span),
                    (**value).clone(),
                ),
                _ => {
                    position += 1;
                    (Expr::new(ExprKind::IntLiteral(position - 1), arg.span), arg.clone())
                }
            })
            .collect();
        Expr::new(ExprKind::ArrayLiteralAssoc(pairs), span)
    } else {
        Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span)
    };
    vec![Expr::new(ExprKind::StringLiteral(method.to_string()), span), arguments]
}

/// Stores one arm's call result into the merge temporary (or, under `$r =& static::m()`, gives a
/// by-value result its own reference cell) and joins the merge block.
#[allow(clippy::too_many_arguments)]
fn finish_private_late_static_arm(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: Option<&str>,
    result_type: &PhpType,
    value: LoweredValue,
    merge: BlockId,
    reachable_initialized: &mut Vec<HashSet<LocalSlotId>>,
    reference_assignment: bool,
    arm_type: &PhpType,
    span: Span,
) {
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let by_value_reference =
        reference_assignment && !ctx.reference_call_context.as_ref().unwrap().adopted;
    super::enum_interface_calls::complete_reference_branch(
        ctx, value, reference_assignment, arm_type, span,
    );
    if by_value_reference {
        // php-src binds a by-value result to a fresh reference and notices it. The cell is
        // already in the assignment's published staging, so a handler that throws releases it.
        let message = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(
            "Notice: Only variables should be assigned by reference\n".into()), span));
        ctx.emit_void(Op::RuntimeCall, vec![message.value],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::WriteDiagnostic)),
            crate::ir_lower::effects_lookup::runtime_effects(), Some(span));
    }
    if let Some(temp_name) = temp_name {
        let value = if ctx.builder.value_php_type(value.value) == PhpType::Void {
            lower_expr(ctx, &Expr::new(ExprKind::Null, span))
        } else {
            value
        };
        store_value_into_temp(ctx, temp_name, result_type.clone(), value, span);
    }
    reachable_initialized.push(ctx.initialized_slots_snapshot());
    branch_to(ctx, merge);
}
