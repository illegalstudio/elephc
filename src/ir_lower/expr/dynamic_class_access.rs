//! Purpose:
//! Lowers dynamic-receiver class member reads — `$class::NAME` and `$class::$prop` — that the
//! parser desugared into the internal `__elephc_dynamic_class_constant` /
//! `__elephc_dynamic_static_property` primitives.
//!
//! Called from:
//! - `crate::ir_lower::expr::function_calls::lower_function_call()`, before registry lowering.
//!
//! Key details:
//! - The receiver is evaluated exactly once into a synthetic PHP local, then reduced to its
//!   runtime class name: `get_class()` for an object, `ltrim($name, '\\')` for a string, and a
//!   runtime `is_object()` split when the static type is `mixed` or a union.
//! - The name is matched case-insensitively (`strcasecmp`, like PHP class lookup) against every
//!   AOT class-like that resolves the member, own or inherited. Each arm lowers the ordinary
//!   named-receiver node (`C::NAME` / `C::$prop`), so values, enum cases, inheritance and
//!   interface constants come from the same code path as statically named access.
//! - A name that matches no candidate throws a catchable `Error`. PHP distinguishes an unknown
//!   class (`Class "X" not found`) from a missing member; this path reports the member form for
//!   both because a per-site table of every known class would cost code size on the hot path.
//! - Member visibility is checked against the lowering scope (`member_read_access_error`): an arm whose
//!   member the scope cannot see throws PHP's visibility `Error` instead of reading it.

use super::*;
use crate::parser::ast::{ArrayEntry, Visibility};

/// Which member family a dynamic-receiver read targets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DynamicMember {
    Constant,
    StaticProperty,
}

/// Expands a direct call to one of the dynamic class-member primitives, or returns `None` when
/// `name` is any other function so the caller continues with ordinary call lowering.
pub(super) fn lower_dynamic_class_member_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let kind = match php_symbol_key(name.as_str().trim_start_matches('\\')).as_str() {
        "__elephc_dynamic_class_constant" => DynamicMember::Constant,
        "__elephc_dynamic_static_property" => DynamicMember::StaticProperty,
        "__elephc_dynamic_static_call" => return lower_dynamic_static_call(ctx, args, expr),
        _ => return None,
    };
    let [receiver, member] = args else {
        return None;
    };
    let ExprKind::StringLiteral(member) = &member.kind else {
        return None;
    };
    Some(lower_dynamic_class_member(ctx, kind, receiver, member, expr, false))
}

/// Lowers `$class::$prop` as the operand of a silent probe (`??`, `isset()`, `empty()`), or
/// returns `None` when `expr` is not a dynamic static-property read.
///
/// PHP reads the property in `BP_VAR_IS` mode there: an uninitialized typed property, an
/// undeclared name or one the scope cannot see yields `null` instead of raising. Known
/// limitation: an unknown class also yields `null`, where PHP throws `Class "X" not found`.
pub(super) fn lower_dynamic_static_property_probe(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "__elephc_dynamic_static_property" {
        return None;
    }
    let [receiver, member] = args.as_slice() else {
        return None;
    };
    let ExprKind::StringLiteral(member) = &member.kind else {
        return None;
    };
    Some(lower_dynamic_class_member(
        ctx,
        DynamicMember::StaticProperty,
        receiver,
        member,
        expr,
        true,
    ))
}

/// Expands one dynamic-receiver member read; `silent` selects the probe semantics described on
/// [`lower_dynamic_static_property_probe`].
fn lower_dynamic_class_member(
    ctx: &mut LoweringContext<'_, '_>,
    kind: DynamicMember,
    receiver: &Expr,
    member: &str,
    expr: &Expr,
    silent: bool,
) -> LoweredValue {
    let member = &member.to_string();
    let span = expr.span;

    // Evaluate the receiver once; later reads go through the synthetic local.
    let receiver_value = lower_expr(ctx, receiver);
    let receiver_type = normalize_value_php_type(ctx.builder.value_php_type(receiver_value.value));
    let receiver_local = ctx.declare_synthetic_php_local(receiver_type.clone());
    store_value_into_temp(ctx, &receiver_local, receiver_type.clone(), receiver_value, span);
    let receiver_var = Expr::new(ExprKind::Variable(receiver_local), span);

    let class_name_expr = runtime_class_name_expr(&receiver_type, receiver_var, span);
    let name_local = ctx.declare_synthetic_php_local(PhpType::Str);
    store_expr_into_temp(ctx, &name_local, PhpType::Str, &class_name_expr, span);

    let candidates = dynamic_member_candidates(ctx, kind, member);
    // The checker types the primitive as its declared `mixed`; the lowered value must agree,
    // or consumers that trust the checked type misread it (see the design notes).
    let result_type = PhpType::Mixed;
    let result_temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let merge = ctx.builder.create_named_block("dyn_class_member.merge", Vec::new());

    let mut arms = Vec::new();
    for class_name in candidates {
        let arm = ctx.builder.create_named_block("dyn_class_member.arm", Vec::new());
        let skip = ctx.builder.create_named_block("dyn_class_member.skip", Vec::new());
        let matches = lower_expr(ctx, &class_name_matches_expr(&name_local, &class_name, span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: matches.value,
            then_target: arm,
            then_args: Vec::new(),
            else_target: skip,
            else_args: Vec::new(),
        });
        arms.push((arm, class_name));
        ctx.builder.position_at_end(skip);
    }

    if silent {
        // No class declares the property: a silent probe answers null. PHP still throws
        // `Class "X" not found` for an unknown class; telling the two apart needs a runtime
        // class table at this site, the code-size cost the non-silent message also avoids.
        store_expr_into_temp(ctx, &result_temp, result_type.clone(), &Expr::new(ExprKind::Null, span), span);
        branch_to(ctx, merge);
    } else {
        // No candidate matched: PHP raises a catchable Error.
        let error = Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("Error"),
                args: vec![missing_member_message_expr(kind, &name_local, member, span)],
            },
            span,
        );
        let error = lower_expr(ctx, &error);
        // Expression-position throw, exactly like `lower_throw_access_error_expr`: the opcode
        // unwinds, and the null store below only keeps the (unreachable) fallthrough well formed.
        ctx.emit_void(
            Op::ThrowException,
            vec![error.value],
            None,
            Op::ThrowException.default_effects(),
            Some(span),
        );
        store_expr_into_temp(ctx, &result_temp, result_type.clone(), &Expr::new(ExprKind::Null, span), span);
        branch_to(ctx, merge);
    }

    for (arm, class_name) in arms {
        ctx.builder.position_at_end(arm);
        ctx.restore_initialized_slots(split_initialized.clone());
        if let Some(message) = member_read_access_error(ctx, kind, &class_name, member) {
            if silent {
                // A property the scope cannot see is simply "not set" to a silent probe.
                store_expr_into_temp(ctx, &result_temp, result_type.clone(), &Expr::new(ExprKind::Null, span), span);
                branch_to(ctx, merge);
            } else {
                throw_error_and_merge(ctx, "Error", literal(&message, span), &result_temp, merge, span);
            }
            continue;
        }
        let receiver = StaticReceiver::Named(Name::from(class_name));
        if silent && super::property_access::static_property_can_be_uninitialized(ctx, &receiver, member) {
            // The ordinary read raises on an uninitialized typed slot; the probe answers null.
            let value = super::property_access::lower_initialized_static_property_value(
                ctx, &receiver, member, expr,
            );
            store_value_into_temp(ctx, &result_temp, result_type.clone(), value, span);
            branch_to(ctx, merge);
            continue;
        }
        let read = match kind {
            DynamicMember::Constant => Expr::new(
                ExprKind::ScopedConstantAccess { receiver, name: member.clone() },
                span,
            ),
            DynamicMember::StaticProperty => Expr::new(
                ExprKind::StaticPropertyAccess { receiver, property: member.clone() },
                span,
            ),
        };
        store_expr_into_temp(ctx, &result_temp, result_type.clone(), &read, span);
        branch_to(ctx, merge);
    }
    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &result_temp, span)
}


/// Builds the expression that yields the receiver's runtime class name as a string.
///
/// PHP strips exactly one leading `\` from a class-name string (`'\Foo'::X` works, `'\\Foo'`
/// does not); `get_class()` never returns one. A `mixed`/union receiver splits on `is_object()` at runtime.
fn runtime_class_name_expr(receiver_type: &PhpType, receiver: Expr, span: Span) -> Expr {
    let call = |name: &str, args: Vec<Expr>| {
        Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, span)
    };
    match receiver_type.codegen_repr() {
        PhpType::Object(_) => call("get_class", vec![receiver]),
        PhpType::Str => strip_one_leading_backslash(receiver, span),
        _ => Expr::new(
            ExprKind::Ternary {
                condition: Box::new(call("is_object", vec![receiver.clone()])),
                then_expr: Box::new(call("get_class", vec![receiver.clone()])),
                else_expr: Box::new(strip_one_leading_backslash(
                    Expr::new(
                        ExprKind::Cast { target: CastType::String, expr: Box::new(receiver) },
                        span,
                    ),
                    span,
                )),
            },
            span,
        ),
    }
}

/// Builds `strcasecmp($name, 'Class') === 0`.
pub(super) fn class_name_matches_expr(name_local: &str, class_name: &str, span: Span) -> Expr {
    let compare = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strcasecmp"),
            args: vec![
                Expr::new(ExprKind::Variable(name_local.to_string()), span),
                Expr::new(ExprKind::StringLiteral(class_name.to_string()), span),
            ],
        },
        span,
    );
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(compare),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
        },
        span,
    )
}

/// Builds PHP's missing-member message with the runtime class name spliced in.
fn missing_member_message_expr(
    kind: DynamicMember,
    name_local: &str,
    member: &str,
    span: Span,
) -> Expr {
    let (prefix, suffix) = match kind {
        DynamicMember::Constant => ("Undefined constant ", format!("::{member}")),
        DynamicMember::StaticProperty => {
            ("Access to undeclared static property ", format!("::${member}"))
        }
    };
    let concat = |left: Expr, right: Expr| {
        Expr::new(
            ExprKind::BinaryOp { left: Box::new(left), op: BinOp::Concat, right: Box::new(right) },
            span,
        )
    };
    concat(
        concat(
            Expr::new(ExprKind::StringLiteral(prefix.to_string()), span),
            Expr::new(ExprKind::Variable(name_local.to_string()), span),
        ),
        Expr::new(ExprKind::StringLiteral(suffix), span),
    )
}

/// Returns, in a deterministic order, every AOT class-like that resolves `member`.
///
/// Constants: classes (own, inherited, or from an implemented interface), interfaces, and
/// enums (cases or constants). Static properties: classes whose static-property table (own
/// or inherited) names `member`.
fn dynamic_member_candidates(
    ctx: &LoweringContext<'_, '_>,
    kind: DynamicMember,
    member: &str,
) -> Vec<String> {
    let mut candidates: Vec<String> = match kind {
        DynamicMember::Constant => ctx
            .classes
            .keys()
            .chain(ctx.interfaces.keys())
            .filter(|class_name| ctx.scoped_constant_value(class_name, member).is_some())
            .chain(ctx.enums.iter().filter_map(|(enum_name, info)| {
                info.cases.iter().any(|case| case.name == member).then_some(enum_name)
            }))
            .cloned()
            .collect(),
        DynamicMember::StaticProperty => ctx
            .classes
            .iter()
            .filter(|(_, info)| info.static_properties.iter().any(|(name, _)| name == member))
            .map(|(class_name, _)| class_name.clone())
            .collect(),
    };
    candidates.sort();
    candidates.dedup();
    candidates
}


/// Expands `__elephc_dynamic_static_call($recv, 'm', ...$args)` — PHP's `$recv::m(...)`.
///
/// Like the member reads, the receiver is evaluated once and reduced to its runtime class name,
/// then matched against every AOT class that declares or inherits `m`. Each arm lowers the
/// ordinary call, after the class is resolved, as PHP orders it (the arguments are evaluated
/// inside the matched arm only):
/// - a static `m` becomes `C::m(...)`, so late static binding sees `C`;
/// - an instance `m` is PHP's "call statically": it runs on `$this` when the current method has
///   one that is an instance of `C` (exact for a `$this` receiver; for another receiver only when
///   no subclass of `C` overrides `m`, otherwise a catchable "not supported" `Error`), and
///   throws PHP's `Non-static method C::m() cannot be called statically` `Error` otherwise.
/// A class with `__callStatic` answers a name it lacks, or one this scope cannot reach, through
/// `C::__callStatic('m', [args])`; any other unmatched name throws `Call to undefined method
/// X::m()`.
fn lower_dynamic_static_call(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let [receiver, method, call_args @ ..] = args else {
        return None;
    };
    {
        // `self::$m()` / `static::{expr}()` / `C::$m()`: the class is named in the source and
        // only the method name is dynamic. The parser marks the receiver with `<Receiver>::class`;
        // constant folding has already turned a named `C::class` into the literal `'C'`, and may
        // have folded the name itself (`parent::{'a' . 'b'}()`), which must still forward
        // late static binding like `parent::ab()`.
        let literal_method = matches!(method.kind, ExprKind::StringLiteral(_));
        let scoped = match &receiver.kind {
            ExprKind::ClassConstant { receiver } => Some(receiver.clone()),
            ExprKind::StringLiteral(class_name) if !literal_method => {
                Some(StaticReceiver::Named(Name::from(class_name.clone())))
            }
            _ => None,
        };
        if let Some(scoped) = scoped {
            return super::scoped_dynamic_calls::lower_scoped_dynamic_call(
                ctx, &scoped, method, call_args, expr,
            );
        }
    }
    let ExprKind::StringLiteral(method) = &method.kind else {
        return None;
    };
    let call_args = &flatten_literal_spreads(call_args);
    let span = expr.span;
    let receiver_is_this = matches!(receiver.kind, ExprKind::This);
    let receiver_value = lower_expr(ctx, receiver);
    let receiver_type = normalize_value_php_type(ctx.builder.value_php_type(receiver_value.value));
    let receiver_local = ctx.declare_synthetic_php_local(receiver_type.clone());
    store_value_into_temp(ctx, &receiver_local, receiver_type.clone(), receiver_value, span);
    let receiver_var = Expr::new(ExprKind::Variable(receiver_local), span);
    let class_name_expr = runtime_class_name_expr(&receiver_type, receiver_var, span);
    let name_local = ctx.declare_synthetic_php_local(PhpType::Str);
    store_expr_into_temp(ctx, &name_local, PhpType::Str, &class_name_expr, span);

    let method_key = php_symbol_key(method);
    let passed_args = call_args.len();
    let first_spread = call_args.iter().position(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    let mut candidates: Vec<MethodCandidate> = ctx
        .classes
        .iter()
        .filter_map(|(class_name, info)| {
            let (sig, is_static, visibility, declaring) = match (
                info.static_methods.get(&method_key),
                info.methods.get(&method_key),
            ) {
                (Some(sig), _) => (
                    sig,
                    true,
                    info.static_method_visibilities.get(&method_key).cloned(),
                    info.static_method_declaring_classes.get(&method_key).cloned(),
                ),
                (None, Some(sig)) => (
                    sig,
                    false,
                    info.method_visibilities.get(&method_key).cloned(),
                    info.method_declaring_classes.get(&method_key).cloned(),
                ),
                // A private method of an ancestor is not in the child's tables, yet PHP finds it
                // (and allows it from the declaring scope): `A::callSec(new B)` → `$o::sec()`.
                (None, None) => match inherited_private_static_method(ctx, info, &method_key) {
                    Some(found) => found,
                    // No such method: a class with `__callStatic` still answers the call.
                    None if class_has_call_static(info) => {
                        return Some(MethodCandidate {
                            class_name: class_name.clone(),
                            is_static: true,
                            visibility: None,
                            declaring: class_name.clone(),
                            required: 0,
                            max_args: None,
                            unsupported: None,
                            magic: true,
                        });
                    }
                    None => return None,
                },
            };
            let late_static_unsupported = (declaring.as_deref() != Some(class_name.as_str())
                && matches!(visibility, Some(Visibility::Private))
                && !info.static_methods.contains_key(&method_key))
                .then(|| {
                    let declaring = declaring.clone().unwrap_or_default();
                    inherited_private_late_static_blocker(ctx, &declaring, &method_key)
                })
                .flatten();
            // A by-reference parameter only matters when this call site passes that argument:
            // the checker saw the by-value primitive, so the write-back cannot happen.
            // A non-literal spread may fill every parameter from its position on.
            let reached = first_spread.map_or(passed_args, |_| usize::MAX);
            let by_ref_unsupported = sig
                .ref_params
                .iter()
                .take(reached)
                .any(|by_ref| *by_ref)
                .then(|| {
                    format!(
                        "elephc: calling {class_name}::{method}() through a dynamic class (`$class::{method}()`) with an argument for a by-reference parameter is not supported; name the class"
                    )
                });
            let variadic_slots = usize::from(sig.variadic.is_some());
            Some(MethodCandidate {
                class_name: class_name.clone(),
                is_static,
                visibility,
                declaring: declaring.unwrap_or_else(|| class_name.clone()),
                // The variadic parameter is never required.
                required: sig
                    .params
                    .iter()
                    .zip(sig.defaults.iter())
                    .take(sig.params.len() - variadic_slots.min(sig.params.len()))
                    .filter(|(_, default)| default.is_none())
                    .count(),
                max_args: sig.variadic.is_none().then_some(sig.params.len()),
                unsupported: late_static_unsupported.or(by_ref_unsupported),
                magic: false,
            })
        })
        .collect();
    candidates.sort_by(|left, right| left.class_name.cmp(&right.class_name));
    let has_this = ctx.has_local_slot("this");
    let has_spread = call_args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    let scope_label = match &ctx.current_class {
        Some(scope) => format!("scope {scope}"),
        None => "global scope".to_string(),
    };

    let result_temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let split_initialized = ctx.initialized_slots_snapshot();
    let merge = ctx.builder.create_named_block("dyn_static_call.merge", Vec::new());
    let mut arms = Vec::new();
    for candidate in candidates {
        let arm = ctx.builder.create_named_block("dyn_static_call.arm", Vec::new());
        let skip = ctx.builder.create_named_block("dyn_static_call.skip", Vec::new());
        let matches =
            lower_expr(ctx, &class_name_matches_expr(&name_local, &candidate.class_name, span));
        conditional_branch(ctx, matches.value, arm, skip);
        arms.push((arm, candidate));
        ctx.builder.position_at_end(skip);
    }
    let undefined = string_concat_expr(
        vec![
            literal("Call to undefined method ", span),
            Expr::new(ExprKind::Variable(name_local.clone()), span),
            literal(&format!("::{method}()"), span),
        ],
        span,
    );
    throw_error_and_merge(ctx, "Error", undefined, &result_temp, merge, span);

    for (arm, candidate) in arms {
        ctx.builder.position_at_end(arm);
        ctx.restore_initialized_slots(split_initialized.clone());
        let MethodCandidate {
            class_name,
            is_static,
            visibility,
            declaring,
            required,
            max_args,
            unsupported,
            magic,
        } = candidate;
        // PHP hands an unknown method, and one this scope cannot reach, to `__callStatic`.
        let has_magic = ctx.classes.get(class_name.as_str()).is_some_and(class_has_call_static);
        if magic || (has_magic && !scope_can_access(ctx, visibility.as_ref(), &declaring)) {
            let call = call_static_magic_expr(&class_name, method, call_args, span);
            store_expr_into_temp(ctx, &result_temp, PhpType::Mixed, &call, span);
            branch_to(ctx, merge);
            continue;
        }
        if !scope_can_access(ctx, visibility.as_ref(), &declaring) {
            let message = format!(
                "Call to {} method {declaring}::{method}() from {scope_label}",
                visibility_word(visibility.as_ref())
            );
            throw_error_and_merge(ctx, "Error", literal(&message, span), &result_temp, merge, span);
            continue;
        }
        if let Some(message) = unsupported {
            // Only this arm is unsupported: other receivers at the same site still compile.
            throw_error_and_merge(ctx, "Error", literal(&message, span), &result_temp, merge, span);
            continue;
        }
        if !has_spread && call_args.len() < required {
            let bound = if max_args == Some(required) { "exactly" } else { "at least" };
            let message = format!(
                "Too few arguments to function {declaring}::{method}(), {} passed and {bound} {required} expected",
                call_args.len()
            );
            // PHP evaluates the supplied arguments before the callee raises the arity error.
            evaluate_args_for_effects(ctx, call_args, span);
            throw_error_and_merge(
                ctx,
                "ArgumentCountError",
                literal(&message, span),
                &result_temp,
                merge,
                span,
            );
            continue;
        }
        if !is_static {
            let non_static = literal(
                &format!("Non-static method {class_name}::{method}() cannot be called statically"),
                span,
            );
            let related = ctx.current_class.as_deref().is_some_and(|scope| {
                super::property_access::class_extends_class(ctx, scope, &class_name)
                    || super::property_access::class_extends_class(ctx, &class_name, scope)
            });
            if !has_this || !related {
                // `$this` (if any) can never be an instance of `C`: PHP refuses the static call.
                throw_error_and_merge(ctx, "Error", non_static, &result_temp, merge, span);
                continue;
            }
            if !receiver_is_this && subclass_overrides_method(ctx, &class_name, &method_key) {
                // PHP would run `C::m()` non-virtually on `$this`; `$this->m()` could dispatch to
                // an override instead, so refuse rather than call the wrong body.
                crate::ir_lower::diagnostics::refuse(
                    span,
                    &format!(
                        "Calling instance method `{class_name}::{method}()` through `$object::{method}()` from a related class is not supported because a subclass overrides it; call it on the object"
                    ),
                );
            }
            // `$this instanceof C`: PHP runs `C::m()` on `$this`.
            let this_is_c = Expr::new(
                ExprKind::InstanceOf {
                    value: Box::new(Expr::new(ExprKind::This, span)),
                    target: InstanceOfTarget::Name(Name::from(class_name.clone())),
                },
                span,
            );
            let on_this = ctx.builder.create_named_block("dyn_static_call.this", Vec::new());
            let not_this = ctx.builder.create_named_block("dyn_static_call.no_this", Vec::new());
            let this_is_c = lower_expr(ctx, &this_is_c);
            conditional_branch(ctx, this_is_c.value, on_this, not_this);
            ctx.builder.position_at_end(not_this);
            throw_error_and_merge(ctx, "Error", non_static, &result_temp, merge, span);
            ctx.builder.position_at_end(on_this);
        }
        let args = arm_call_args(ctx, call_args, if has_spread { None } else { max_args }, span);
        let call = if is_static {
            // An inherited private method lives only on its declaring class.
            let target = if ctx
                .classes
                .get(class_name.as_str())
                .is_some_and(|info| info.static_methods.contains_key(&method_key))
            {
                class_name
            } else {
                declaring
            };
            Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: StaticReceiver::Named(Name::from(target)),
                    method: method.clone(),
                    args,
                },
                span,
            )
        } else {
            Expr::new(
                ExprKind::MethodCall {
                    object: Box::new(Expr::new(ExprKind::This, span)),
                    method: method.clone(),
                    args,
                },
                span,
            )
        };
        store_expr_into_temp(ctx, &result_temp, PhpType::Mixed, &call, span);
        branch_to(ctx, merge);
    }
    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &result_temp, span))
}

/// Returns whether any subclass of `class_name` declares its own `method_key`.
fn subclass_overrides_method(ctx: &LoweringContext<'_, '_>, class_name: &str, method_key: &str) -> bool {
    let declaring = ctx
        .classes
        .get(class_name)
        .and_then(|info| info.method_declaring_classes.get(method_key));
    ctx.classes.iter().any(|(other, info)| {
        other != class_name
            && super::property_access::class_extends_class(ctx, other, class_name)
            && info.method_declaring_classes.get(method_key) != declaring
    })
}

/// Builds a string literal expression.
pub(super) fn literal(text: &str, span: Span) -> Expr {
    Expr::new(ExprKind::StringLiteral(text.to_string()), span)
}

/// Folds `parts` into one left-associated `.` concatenation.
pub(super) fn string_concat_expr(parts: Vec<Expr>, span: Span) -> Expr {
    parts
        .into_iter()
        .reduce(|left, right| {
            Expr::new(
                ExprKind::BinaryOp { left: Box::new(left), op: BinOp::Concat, right: Box::new(right) },
                span,
            )
        })
        .expect("at least one part")
}

/// Terminates the current block with a `CondBr` on `cond`.
pub(super) fn conditional_branch(ctx: &mut LoweringContext<'_, '_>, cond: ValueId, then_target: BlockId, else_target: BlockId) {
    ctx.builder.terminate(Terminator::CondBr {
        cond,
        then_target,
        then_args: Vec::new(),
        else_target,
        else_args: Vec::new(),
    });
}

/// Throws `new Error($message)` and keeps the unreachable fallthrough well formed.
pub(super) fn throw_error_and_merge(
    ctx: &mut LoweringContext<'_, '_>,
    error_class: &str,
    message: Expr,
    result_temp: &str,
    merge: BlockId,
    span: Span,
) {
    let error = Expr::new(
        ExprKind::NewObject { class_name: Name::unqualified(error_class), args: vec![message] },
        span,
    );
    let error = lower_expr(ctx, &error);
    ctx.emit_void(
        Op::ThrowException,
        vec![error.value],
        None,
        Op::ThrowException.default_effects(),
        Some(span),
    );
    store_expr_into_temp(ctx, result_temp, PhpType::Mixed, &Expr::new(ExprKind::Null, span), span);
    branch_to(ctx, merge);
}

/// Builds `str_starts_with($n, "\\") ? substr($n, 1) : $n` — PHP drops ONE leading backslash.
fn strip_one_leading_backslash(name: Expr, span: Span) -> Expr {
    let call = |function: &str, args: Vec<Expr>| {
        Expr::new(ExprKind::FunctionCall { name: Name::unqualified(function), args }, span)
    };
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(call("str_starts_with", vec![name.clone(), literal("\\", span)])),
            then_expr: Box::new(call(
                "substr",
                vec![name.clone(), Expr::new(ExprKind::IntLiteral(1), span)],
            )),
            else_expr: Box::new(name),
        },
        span,
    )
}

/// Returns whether the lowering scope (the class whose code is being lowered, if any) may see a
/// member of `visibility` declared on `declaring`, by PHP's rules.
pub(super) fn scope_can_access(
    ctx: &LoweringContext<'_, '_>,
    visibility: Option<&Visibility>,
    declaring: &str,
) -> bool {
    let scope = ctx.current_class.as_deref();
    match visibility {
        None | Some(Visibility::Public) => true,
        Some(Visibility::Private) => {
            scope.is_some_and(|scope| php_symbol_key(scope) == php_symbol_key(declaring))
        }
        Some(Visibility::Protected) => scope.is_some_and(|scope| {
            super::property_access::class_extends_class(ctx, scope, declaring)
                || super::property_access::class_extends_class(ctx, declaring, scope)
        }),
    }
}

/// PHP's spelling of a non-public visibility in access errors.
pub(super) fn visibility_word(visibility: Option<&Visibility>) -> &'static str {
    match visibility {
        Some(Visibility::Private) => "private",
        _ => "protected",
    }
}

/// Returns PHP's access `Error` message when reading `member` of `class_name` is not allowed
/// from the lowering scope, or `None` when the ordinary named read applies.
fn member_read_access_error(
    ctx: &LoweringContext<'_, '_>,
    kind: DynamicMember,
    class_name: &str,
    member: &str,
) -> Option<String> {
    match kind {
        DynamicMember::Constant => {
            // Walk to the class that declares the constant; interface and enum members are public.
            let mut current = Some(class_name.to_string());
            while let Some(name) = current {
                let info = ctx.classes.get(name.as_str())?;
                if info.constants.contains_key(member) {
                    let visibility = info.constant_visibilities.get(member);
                    if matches!(visibility, Some(Visibility::Private)) && name != class_name {
                        // Private constants are not inherited.
                        return Some(format!("Undefined constant {class_name}::{member}"));
                    }
                    if scope_can_access(ctx, visibility, &name) {
                        return None;
                    }
                    return Some(format!(
                        "Cannot access {} constant {class_name}::{member}",
                        visibility_word(visibility)
                    ));
                }
                current = info.parent.clone();
            }
            None
        }
        DynamicMember::StaticProperty => {
            let info = ctx.classes.get(class_name)?;
            let visibility = info.static_property_visibilities.get(member);
            let declaring = info
                .static_property_declaring_classes
                .get(member)
                .map(String::as_str)
                .unwrap_or(class_name);
            if scope_can_access(ctx, visibility, declaring) {
                return None;
            }
            Some(format!(
                "Cannot access {} property {class_name}::${member}",
                visibility_word(visibility)
            ))
        }
    }
}

/// One class that declares or inherits the dynamically called method.
struct MethodCandidate {
    class_name: String,
    is_static: bool,
    visibility: Option<Visibility>,
    declaring: String,
    /// Parameters without a default.
    required: usize,
    /// `None` when the method is variadic.
    max_args: Option<usize>,
    /// Why this arm cannot be lowered faithfully; the arm then throws instead.
    unsupported: Option<String>,
    /// The class has no such method and answers through its `__callStatic`.
    magic: bool,
}

/// Whether the class declares or inherits a `__callStatic` handler.
fn class_has_call_static(info: &crate::types::ClassInfo) -> bool {
    info.static_methods.contains_key(&php_symbol_key("__callStatic"))
}

/// Builds `C::__callStatic('m', [args])`: PHP's fallback for `$class::m(...)` when `C` has no
/// accessible `m`. Named arguments keep their names as string keys and a spread is unpacked
/// into the list, as PHP collects them for the handler.
fn call_static_magic_expr(class_name: &str, method: &str, call_args: &[Expr], span: Span) -> Expr {
    let entries = call_args
        .iter()
        .map(|arg| match &arg.kind {
            ExprKind::Spread(inner) => ArrayEntry::Spread((**inner).clone()),
            ExprKind::NamedArg { name, value } => ArrayEntry::Keyed(
                Expr::new(ExprKind::StringLiteral(name.clone()), arg.span),
                (**value).clone(),
            ),
            _ => ArrayEntry::Value(arg.clone()),
        })
        .collect();
    Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(Name::from(class_name.to_string())),
            method: "__callStatic".to_string(),
            args: vec![
                literal(method, span),
                Expr::new(ExprKind::ArrayLiteralMixed(entries), span),
            ],
        },
        span,
    )
}

/// Evaluates every supplied argument, in order, only for its side effects, keeping each value
/// in a synthetic local so its ownership is released like any other temporary.
///
/// Used by arms that raise `ArgumentCountError` for too few arguments: PHP evaluates the
/// arguments the caller supplied before the callee rejects the call.
pub(super) fn evaluate_args_for_effects(
    ctx: &mut LoweringContext<'_, '_>,
    call_args: &[Expr],
    span: Span,
) {
    for arg in call_args {
        let value = lower_expr(ctx, arg);
        let value_type = normalize_value_php_type(ctx.builder.value_php_type(value.value));
        let local = ctx.declare_synthetic_php_local(value_type.clone());
        store_value_into_temp(ctx, &local, value_type, value, span);
    }
}

/// Returns the arguments one arm passes. PHP evaluates surplus arguments to a non-variadic user
/// method and then ignores them; the call node cannot carry them, so every argument is first
/// evaluated, in order, into a temporary and only the first `max` are passed.
pub(super) fn arm_call_args(
    ctx: &mut LoweringContext<'_, '_>,
    call_args: &[Expr],
    max: Option<usize>,
    span: Span,
) -> Vec<Expr> {
    let Some(max) = max.filter(|max| call_args.len() > *max) else {
        return call_args.to_vec();
    };
    let mut passed = Vec::with_capacity(max);
    for (index, arg) in call_args.iter().enumerate() {
        let value = lower_expr(ctx, arg);
        let value_type = normalize_value_php_type(ctx.builder.value_php_type(value.value));
        let local = ctx.declare_synthetic_php_local(value_type.clone());
        store_value_into_temp(ctx, &local, value_type, value, span);
        if index < max {
            passed.push(Expr::new(ExprKind::Variable(local), arg.span));
        }
    }
    passed
}

/// Finds a private static method an ancestor of `info` declares, which PHP resolves for
/// `$child::m()` although the child's method table does not list it.
///
/// The arm calls it on the declaring class; `inherited_private_late_static_blocker` decides
/// whether late static binding inside it still behaves as PHP's would.
fn inherited_private_static_method<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    info: &'a crate::types::ClassInfo,
    method_key: &str,
) -> Option<(&'a crate::types::FunctionSig, bool, Option<Visibility>, Option<String>)> {
    let mut parent = info.parent.clone();
    while let Some(name) = parent {
        let ancestor = ctx.classes.get(name.as_str())?;
        if let Some(sig) = ancestor.static_methods.get(method_key) {
            let visibility = ancestor.static_method_visibilities.get(method_key).cloned();
            if !matches!(visibility, Some(Visibility::Private)) {
                return None;
            }
            return Some((sig, true, visibility, Some(name)));
        }
        parent = ancestor.parent.clone();
    }
    None
}

/// Expands `...[a, b]` (a literal list) into plain arguments so arity can be checked per arm.
pub(super) fn flatten_literal_spreads(args: &[Expr]) -> Vec<Expr> {
    let mut flat = Vec::with_capacity(args.len());
    for arg in args {
        match &arg.kind {
            ExprKind::Spread(inner) => match &inner.kind {
                ExprKind::ArrayLiteral(items)
                    if items.iter().all(|item| !matches!(item.kind, ExprKind::Spread(_))) =>
                {
                    flat.extend(items.iter().cloned());
                }
                _ => flat.push(arg.clone()),
            },
            _ => flat.push(arg.clone()),
        }
    }
    flat
}

/// Returns why calling the private static `declaring::method_key()` on the declaring class
/// (instead of with `static` bound to the receiver's subclass) could differ from PHP, or `None`
/// when its body provably never observes the called class (`lsb_taint`, a typed AST walk).
fn inherited_private_late_static_blocker(
    ctx: &LoweringContext<'_, '_>,
    declaring: &str,
    method_key: &str,
) -> Option<String> {
    let info = ctx.classes.get(declaring)?;
    let decl = info.method_decls.iter().find(|decl| php_symbol_key(&decl.name) == method_key)?;
    super::lsb_taint::body_uses_late_static_binding(&decl.body).then(|| {
        format!(
            "elephc: calling private static method {declaring}::{method_key}() through a subclass with `$class::{method_key}()` is not supported because it depends on late static binding"
        )
    })
}
