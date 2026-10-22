//! Purpose:
//! Lowers static-style calls whose METHOD NAME is only known at run time on a class receiver
//! named in the source: `self::$method(...)`, `static::{'is' . $kind}(...)`, `parent::{$m}(...)`
//! and `C::$method(...)`.
//!
//! Called from:
//! - `crate::ir_lower::expr::dynamic_class_access::lower_dynamic_class_member_call()`, for a
//!   `__elephc_dynamic_static_call(<Receiver>::class, $name, ...$args)` whose first argument is
//!   the `ClassConstant` marker the parser emits for these forms.
//!
//! Key details:
//! - The name is evaluated once (PHP throws `Error: Method name must be a string` for a
//!   non-string) and matched case-insensitively against every method the receiver class
//!   declares or inherits. Each arm lowers the ordinary `Receiver::m(...)` with the same
//!   receiver, so late static binding, `self`/`parent` forwarding and `$this` passing for an
//!   instance method are exactly those of the statically named call.
//! - Arms PHP would reject at run time throw the same `Error` instead of reaching the
//!   compile-time checks of the named call: inaccessible visibility, an instance method
//!   without a compatible `$this`, too few arguments. An arm whose call cannot be lowered
//!   faithfully throws an explicit elephc `Error` (a by-reference parameter the checker never
//!   saw through the primitive).
//! - Under `static::`, a name only subclasses declare dispatches on the called class: the
//!   deepest declaring subclass `S` it descends from runs `S::m()`, exact when the body never
//!   observes the called class (`lsb_taint`) or the called class is `S`; otherwise, and for an
//!   instance method found this way, the arm throws an explicit elephc `Error`.
//! - No match falls back like PHP: `__call` on `$this` when the class has it and `$this` is an
//!   instance of it, then `__callStatic`, and otherwise `Call to undefined method C::m()`. An
//!   inaccessible method takes the same magic fallback before its visibility `Error`.
//!   A name that is itself `__call`/`__callStatic` calls that method directly.

use super::dynamic_class_access::{
    arm_call_args, class_name_matches_expr, conditional_branch, evaluate_args_for_effects,
    flatten_literal_spreads, literal,
    scope_can_access, string_concat_expr, throw_error_and_merge, visibility_word,
};
use super::*;
use crate::parser::ast::Visibility;

/// One method of the receiver class the runtime name may select.
struct ScopedCandidate {
    /// The method name as declared (PHP's spelling in messages and the arm call).
    method: String,
    is_static: bool,
    visibility: Option<Visibility>,
    declaring: String,
    required: usize,
    max_args: Option<usize>,
    by_ref_reached: bool,
}

/// Expands `__elephc_dynamic_static_call(<Receiver>::class, $name, ...$args)`.
pub(super) fn lower_scoped_dynamic_call(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    method: &Expr,
    call_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let span = expr.span;
    let Some(class_name) = scoped_receiver_class(ctx, receiver, span) else {
        return Some(lower_expr(ctx, &Expr::new(ExprKind::Null, span)));
    };
    let call_args = &flatten_literal_spreads(call_args);
    let has_spread = call_args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    let passed_args = call_args.len();

    // The method name, once, as a string (PHP refuses any other type).
    let name_value = lower_expr(ctx, method);
    let name_type = normalize_value_php_type(ctx.builder.value_php_type(name_value.value));
    let raw_name_local = ctx.declare_synthetic_php_local(name_type.clone());
    store_value_into_temp(ctx, &raw_name_local, name_type.clone(), name_value, span);
    let raw_name = Expr::new(ExprKind::Variable(raw_name_local), span);
    let result_temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let merge = ctx.builder.create_named_block("scoped_dyn_call.merge", Vec::new());
    if name_type.codegen_repr() != PhpType::Str {
        let is_string = lower_expr(
            ctx,
            &Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("is_string"),
                    args: vec![raw_name.clone()],
                },
                span,
            ),
        );
        let named = ctx.builder.create_named_block("scoped_dyn_call.named", Vec::new());
        let not_named = ctx.builder.create_named_block("scoped_dyn_call.not_named", Vec::new());
        conditional_branch(ctx, is_string.value, named, not_named);
        ctx.builder.position_at_end(not_named);
        throw_error_and_merge(
            ctx,
            "Error",
            literal("Method name must be a string", span),
            &result_temp,
            merge,
            span,
        );
        ctx.builder.position_at_end(named);
    }
    let name_local = ctx.declare_synthetic_php_local(PhpType::Str);
    store_expr_into_temp(
        ctx,
        &name_local,
        PhpType::Str,
        &Expr::new(ExprKind::Cast { target: CastType::String, expr: Box::new(raw_name) }, span),
        span,
    );
    let name_var = Expr::new(ExprKind::Variable(name_local.clone()), span);

    let first_spread = call_args.iter().position(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    let reached = first_spread.map_or(passed_args, |_| usize::MAX);
    let candidates = scoped_candidates(ctx, &class_name, reached);
    let subclass_only = if matches!(receiver, StaticReceiver::Static) {
        subclass_only_methods(ctx, &class_name, &candidates, reached)
    } else {
        Vec::new()
    };
    let has_this = ctx.has_local_slot("this");
    let scope_label = match &ctx.current_class {
        Some(scope) => format!("scope {scope}"),
        None => "global scope".to_string(),
    };
    let receiver_class_expr = Expr::new(ExprKind::ClassConstant { receiver: receiver.clone() }, span);
    let magic = MagicFallback::new(ctx, receiver, &class_name, has_this);

    let split_initialized = ctx.initialized_slots_snapshot();
    let mut arms = Vec::new();
    for candidate in candidates {
        let arm = ctx.builder.create_named_block("scoped_dyn_call.arm", Vec::new());
        let skip = ctx.builder.create_named_block("scoped_dyn_call.skip", Vec::new());
        let matches = lower_expr(ctx, &class_name_matches_expr(&name_local, &candidate.method, span));
        conditional_branch(ctx, matches.value, arm, skip);
        arms.push((arm, candidate));
        ctx.builder.position_at_end(skip);
    }
    for (method, declarers) in &subclass_only {
        let arm = ctx.builder.create_named_block("scoped_dyn_call.subclass", Vec::new());
        let skip = ctx.builder.create_named_block("scoped_dyn_call.skip", Vec::new());
        let matches = lower_expr(ctx, &class_name_matches_expr(&name_local, method, span));
        conditional_branch(ctx, matches.value, arm, skip);
        ctx.builder.position_at_end(arm);
        ctx.restore_initialized_slots(split_initialized.clone());
        lower_subclass_static_arms(
            ctx,
            method,
            declarers,
            call_args,
            has_spread,
            &magic,
            &name_var,
            &receiver_class_expr,
            &result_temp,
            merge,
            span,
        );
        ctx.builder.position_at_end(skip);
    }
    // No declared method matched: PHP's magic fallback, then `Call to undefined method`.
    ctx.restore_initialized_slots(split_initialized.clone());
    let undefined = string_concat_expr(
        vec![
            literal("Call to undefined method ", span),
            receiver_class_expr.clone(),
            literal("::", span),
            name_var.clone(),
            literal("()", span),
        ],
        span,
    );
    magic.call_or_throw(ctx, &name_var, call_args, "Error", undefined, &result_temp, merge, span);

    for (arm, candidate) in arms {
        ctx.builder.position_at_end(arm);
        ctx.restore_initialized_slots(split_initialized.clone());
        let ScopedCandidate {
            method,
            is_static,
            visibility,
            declaring,
            required,
            max_args,
            by_ref_reached,
        } = candidate;
        if !scope_can_access(ctx, visibility.as_ref(), &declaring) {
            let message = format!(
                "Call to {} method {declaring}::{method}() from {scope_label}",
                visibility_word(visibility.as_ref())
            );
            magic.call_or_throw(
                ctx,
                &name_var,
                call_args,
                "Error",
                literal(&message, span),
                &result_temp,
                merge,
                span,
            );
            continue;
        }
        if by_ref_reached {
            // The checker typed the by-value primitive, so the write-back cannot happen.
            let message = format!(
                "elephc: calling {declaring}::{method}() with a runtime method name (`{}::$method()`) and an argument for a by-reference parameter is not supported; name the method",
                receiver_spelling(receiver, &class_name)
            );
            throw_error_and_merge(ctx, "Error", literal(&message, span), &result_temp, merge, span);
            continue;
        }
        if !has_spread && passed_args < required {
            let bound = if max_args == Some(required) { "exactly" } else { "at least" };
            let message = format!(
                "Too few arguments to function {declaring}::{method}(), {passed_args} passed and {bound} {required} expected"
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
                &format!("Non-static method {declaring}::{method}() cannot be called statically"),
                span,
            );
            if !has_this {
                throw_error_and_merge(ctx, "Error", non_static, &result_temp, merge, span);
                continue;
            }
            // `self::`/`parent::`/`static::` name a class `$this` always is an instance of; a
            // named `C::m()` only runs on `$this` when it is a `C`.
            if let StaticReceiver::Named(_) = receiver {
                let related = ctx.current_class.as_deref().is_some_and(|scope| {
                    super::property_access::class_extends_class(ctx, scope, &class_name)
                        || super::property_access::class_extends_class(ctx, &class_name, scope)
                });
                if !related {
                    throw_error_and_merge(ctx, "Error", non_static, &result_temp, merge, span);
                    continue;
                }
                let this_is_c = Expr::new(
                    ExprKind::InstanceOf {
                        value: Box::new(Expr::new(ExprKind::This, span)),
                        target: InstanceOfTarget::Name(Name::from(class_name.clone())),
                    },
                    span,
                );
                let on_this = ctx.builder.create_named_block("scoped_dyn_call.this", Vec::new());
                let not_this = ctx.builder.create_named_block("scoped_dyn_call.no_this", Vec::new());
                let this_is_c = lower_expr(ctx, &this_is_c);
                conditional_branch(ctx, this_is_c.value, on_this, not_this);
                ctx.builder.position_at_end(not_this);
                throw_error_and_merge(ctx, "Error", non_static, &result_temp, merge, span);
                ctx.builder.position_at_end(on_this);
            }
        }
        let args = arm_call_args(ctx, call_args, if has_spread { None } else { max_args }, span);
        let call = if !is_static && matches!(receiver, StaticReceiver::Static) {
            // `static::m()` for an instance method resolves `m` in the called class, which is
            // `$this`'s class, and runs it on `$this`: exactly `$this->m()` (a private `m` of the
            // scope class still binds non-virtually there).
            Expr::new(
                ExprKind::MethodCall {
                    object: Box::new(Expr::new(ExprKind::This, span)),
                    method,
                    args,
                },
                span,
            )
        } else {
            Expr::new(ExprKind::StaticMethodCall { receiver: receiver.clone(), method, args }, span)
        };
        store_expr_into_temp(ctx, &result_temp, PhpType::Mixed, &call, span);
        branch_to(ctx, merge);
    }
    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &result_temp, span))
}

/// Resolves the class whose method table the runtime name is matched against: the lowering
/// class for `self`/`static`, its parent for `parent`, the named class otherwise. Refuses (and
/// returns `None`) where PHP itself has no such class.
fn scoped_receiver_class(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    span: Span,
) -> Option<String> {
    let refuse = |message: &str| {
        crate::ir_lower::diagnostics::refuse(span, message);
        None
    };
    match receiver {
        StaticReceiver::Self_ | StaticReceiver::Static => match ctx.current_class.clone() {
            Some(class_name) => Some(class_name),
            None => refuse("Cannot use \"self\"/\"static\" when no class scope is active"),
        },
        StaticReceiver::Parent => {
            match ctx.current_class.as_deref().and_then(|scope| ctx.classes.get(scope)) {
                Some(info) => match info.parent.clone() {
                    Some(parent) => Some(parent),
                    None => refuse("Cannot use \"parent\" when current class scope has no parent"),
                },
                None => refuse("Cannot use \"parent\" when no class scope is active"),
            }
        }
        StaticReceiver::Named(name) => {
            let key = name.as_str().trim_start_matches('\\');
            match ctx
                .classes
                .keys()
                .find(|class_name| php_symbol_key(class_name) == php_symbol_key(key))
            {
                Some(class_name) => Some(class_name.clone()),
                None => refuse(&format!(
                    "Class \"{key}\" is not known to the compiler; a static call with a runtime method name needs a declared class"
                )),
            }
        }
        StaticReceiver::Generic(_) => {
            refuse("A static call with a runtime method name on a generic class receiver is not supported")
        }
    }
}

/// The receiver as written, for diagnostics.
fn receiver_spelling(receiver: &StaticReceiver, class_name: &str) -> String {
    match receiver {
        StaticReceiver::Self_ => "self".to_string(),
        StaticReceiver::Static => "static".to_string(),
        StaticReceiver::Parent => "parent".to_string(),
        _ => class_name.to_string(),
    }
}

/// Every method `class_name` declares or inherits (static and instance), with what an arm needs.
/// `__call` and `__callStatic` are ordinary candidates too: a runtime name that IS
/// `'__callStatic'` calls it directly with the supplied arguments, exactly like
/// `C::__callStatic('given', [])`; only a name that matches nothing takes the magic fallback.
fn scoped_candidates(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    reached: usize,
) -> Vec<ScopedCandidate> {
    let Some(info) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    let mut push = |key: &String, sig: &crate::types::FunctionSig, is_static: bool| {
        let (visibility, declaring) = if is_static {
            (
                info.static_method_visibilities.get(key).cloned(),
                info.static_method_declaring_classes.get(key).cloned(),
            )
        } else {
            (
                info.method_visibilities.get(key).cloned(),
                info.method_declaring_classes.get(key).cloned(),
            )
        };
        let declaring = declaring.unwrap_or_else(|| class_name.to_string());
        let method = declared_method_name(ctx, &declaring, key).unwrap_or_else(|| key.clone());
        let variadic_slots = usize::from(sig.variadic.is_some());
        candidates.push(ScopedCandidate {
            method,
            is_static,
            visibility,
            declaring,
            required: sig
                .params
                .iter()
                .zip(sig.defaults.iter())
                .take(sig.params.len() - variadic_slots.min(sig.params.len()))
                .filter(|(_, default)| default.is_none())
                .count(),
            max_args: sig.variadic.is_none().then_some(sig.params.len()),
            by_ref_reached: sig.ref_params.iter().take(reached).any(|by_ref| *by_ref),
        });
    };
    for (key, sig) in &info.static_methods {
        push(key, sig, true);
    }
    for (key, sig) in &info.methods {
        if !info.static_methods.contains_key(key) {
            push(key, sig, false);
        }
    }
    candidates.sort_by(|left, right| left.method.cmp(&right.method));
    candidates
}

/// The declared spelling of `key` on `declaring` (method tables are keyed case-folded).
fn declared_method_name(ctx: &LoweringContext<'_, '_>, declaring: &str, key: &str) -> Option<String> {
    let info = ctx.classes.get(declaring)?;
    info.method_decls
        .iter()
        .find(|decl| php_symbol_key(&decl.name) == key)
        .map(|decl| decl.name.clone())
}

/// A subclass of the `static::` receiver class that declares a method the class itself lacks.
struct SubclassDeclarer {
    class_name: String,
    is_static: bool,
    visibility: Option<Visibility>,
    required: usize,
    max_args: Option<usize>,
    by_ref_reached: bool,
    /// The body observes the called class (`lsb_taint`), so `S::m()` is exact only when the
    /// called class IS `S`.
    uses_late_static_binding: bool,
}

/// Methods only subclasses of `class_name` declare, each with its declaring subclasses ordered
/// deepest first, so the first declarer the called class descends from is the one PHP resolves.
fn subclass_only_methods(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    candidates: &[ScopedCandidate],
    reached: usize,
) -> Vec<(String, Vec<SubclassDeclarer>)> {
    let known: HashSet<String> =
        candidates.iter().map(|candidate| php_symbol_key(&candidate.method)).collect();
    let depth = |name: &str| {
        let mut depth = 0usize;
        let mut current = ctx.classes.get(name).and_then(|info| info.parent.clone());
        while let Some(parent) = current {
            depth += 1;
            current = ctx.classes.get(parent.as_str()).and_then(|info| info.parent.clone());
        }
        depth
    };
    let mut by_method: std::collections::BTreeMap<String, (String, Vec<SubclassDeclarer>)> =
        std::collections::BTreeMap::new();
    for (other, info) in ctx.classes.iter() {
        if other.as_str() == class_name
            || !super::property_access::class_extends_class(ctx, other, class_name)
        {
            continue;
        }
        for decl in &info.method_decls {
            let key = php_symbol_key(&decl.name);
            if known.contains(&key) {
                continue;
            }
            let (sig, is_static, visibility) = match (info.static_methods.get(&key), info.methods.get(&key)) {
                (Some(sig), _) => (sig, true, info.static_method_visibilities.get(&key).cloned()),
                (None, Some(sig)) => (sig, false, info.method_visibilities.get(&key).cloned()),
                (None, None) => continue,
            };
            let variadic_slots = usize::from(sig.variadic.is_some());
            let entry = by_method.entry(key).or_insert_with(|| (decl.name.clone(), Vec::new()));
            entry.1.push(SubclassDeclarer {
                class_name: other.clone(),
                is_static,
                visibility,
                required: sig
                    .params
                    .iter()
                    .zip(sig.defaults.iter())
                    .take(sig.params.len() - variadic_slots.min(sig.params.len()))
                    .filter(|(_, default)| default.is_none())
                    .count(),
                max_args: sig.variadic.is_none().then_some(sig.params.len()),
                by_ref_reached: sig.ref_params.iter().take(reached).any(|by_ref| *by_ref),
                uses_late_static_binding: super::lsb_taint::body_uses_late_static_binding(&decl.body),
            });
        }
    }
    by_method
        .into_values()
        .map(|(method, mut declarers)| {
            declarers.sort_by(|left, right| {
                depth(&right.class_name)
                    .cmp(&depth(&left.class_name))
                    .then_with(|| left.class_name.cmp(&right.class_name))
            });
            (method, declarers)
        })
        .collect()
}

/// Lowers `static::{$name}()` for a name only subclasses declare: the first declaring subclass
/// the called class is an instance of runs its method. A static method is called on that
/// subclass (`S::m()`), which binds `static` to `S`: exact when the body never observes the
/// called class (`lsb_taint`) or the called class is `S` itself, an explicit `Error` otherwise.
/// An instance method found this way is refused with an explicit `Error`.
#[allow(clippy::too_many_arguments)]
fn lower_subclass_static_arms(
    ctx: &mut LoweringContext<'_, '_>,
    method: &str,
    declarers: &[SubclassDeclarer],
    call_args: &[Expr],
    has_spread: bool,
    magic: &MagicFallback,
    name_var: &Expr,
    receiver_class_expr: &Expr,
    result_temp: &str,
    merge: BlockId,
    span: Span,
) {
    let called_class = Expr::new(ExprKind::ClassConstant { receiver: StaticReceiver::Static }, span);
    let scope_label = match &ctx.current_class {
        Some(scope) => format!("scope {scope}"),
        None => "global scope".to_string(),
    };
    for declarer in declarers {
        // `static::class` is `S` or one of its subclasses: an exact name match per class, since
        // the class table is closed (and `is_a()` is not needed on a class string).
        let mut family: Vec<&String> = ctx
            .classes
            .keys()
            .filter(|other| super::property_access::class_extends_class(ctx, other, &declarer.class_name))
            .collect();
        family.sort();
        let is_declarer_expr = family
            .into_iter()
            .map(|member| class_name_matches_expr_value(called_class.clone(), member, span))
            .reduce(|left, right| {
                Expr::new(
                    ExprKind::BinaryOp { left: Box::new(left), op: BinOp::Or, right: Box::new(right) },
                    span,
                )
            })
            .unwrap_or_else(|| Expr::new(ExprKind::BoolLiteral(false), span));
        let is_declarer = lower_expr(ctx, &is_declarer_expr);
        let arm = ctx.builder.create_named_block("scoped_dyn_call.declarer", Vec::new());
        let next = ctx.builder.create_named_block("scoped_dyn_call.next_declarer", Vec::new());
        conditional_branch(ctx, is_declarer.value, arm, next);
        ctx.builder.position_at_end(arm);
        let class_name = &declarer.class_name;
        if !scope_can_access(ctx, declarer.visibility.as_ref(), class_name) {
            let message = format!(
                "Call to {} method {class_name}::{method}() from {scope_label}",
                visibility_word(declarer.visibility.as_ref())
            );
            magic.call_or_throw(ctx, name_var, call_args, "Error", literal(&message, span), result_temp, merge, span);
        } else if !declarer.is_static || declarer.by_ref_reached {
            let message = format!(
                "elephc: calling {class_name}::{method}() through `static::` with a runtime method name is not supported for {}; name the method",
                if declarer.is_static { "a by-reference parameter" } else { "an instance method only a subclass declares" }
            );
            throw_error_and_merge(ctx, "Error", literal(&message, span), result_temp, merge, span);
        } else if !has_spread && call_args.len() < declarer.required {
            let bound = if declarer.max_args == Some(declarer.required) { "exactly" } else { "at least" };
            let message = format!(
                "Too few arguments to function {class_name}::{method}(), {} passed and {bound} {} expected",
                call_args.len(),
                declarer.required
            );
            evaluate_args_for_effects(ctx, call_args, span);
            throw_error_and_merge(ctx, "ArgumentCountError", literal(&message, span), result_temp, merge, span);
        } else {
            if declarer.uses_late_static_binding {
                // Only exact when `static` already is the declaring subclass.
                let exact = lower_expr(ctx, &class_name_matches_expr_value(called_class.clone(), class_name, span));
                let call_block = ctx.builder.create_named_block("scoped_dyn_call.exact", Vec::new());
                let refuse = ctx.builder.create_named_block("scoped_dyn_call.lsb", Vec::new());
                conditional_branch(ctx, exact.value, call_block, refuse);
                ctx.builder.position_at_end(refuse);
                let message = format!(
                    "elephc: calling {class_name}::{method}() through `static::` with a runtime method name from a subclass of {class_name} is not supported because it depends on late static binding; name the method"
                );
                throw_error_and_merge(ctx, "Error", literal(&message, span), result_temp, merge, span);
                ctx.builder.position_at_end(call_block);
            }
            let args = arm_call_args(ctx, call_args, if has_spread { None } else { declarer.max_args }, span);
            let call = Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: StaticReceiver::Named(Name::from(class_name.clone())),
                    method: method.to_string(),
                    args,
                },
                span,
            );
            store_expr_into_temp(ctx, result_temp, PhpType::Mixed, &call, span);
            branch_to(ctx, merge);
        }
        ctx.builder.position_at_end(next);
    }
    // The called class declares no such method after all.
    let undefined = string_concat_expr(
        vec![
            literal("Call to undefined method ", span),
            receiver_class_expr.clone(),
            literal("::", span),
            name_var.clone(),
            literal("()", span),
        ],
        span,
    );
    magic.call_or_throw(ctx, name_var, call_args, "Error", undefined, result_temp, merge, span);
}

/// Builds `strcasecmp(<value>, 'Class') === 0` for an arbitrary string-valued expression.
fn class_name_matches_expr_value(value: Expr, class_name: &str, span: Span) -> Expr {
    let compare = Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("strcasecmp"),
            args: vec![value, literal(class_name, span)],
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

/// PHP's fallback for a static-style call that selects no accessible method.
struct MagicFallback {
    /// `$this->__call($name, $args)` applies (the class has `__call` and `$this` exists).
    call_on_this: bool,
    /// `Receiver::__callStatic($name, $args)` applies.
    call_static: bool,
    /// For a named receiver, `__call` additionally needs `$this instanceof C`.
    named_class: Option<String>,
    receiver: StaticReceiver,
}

impl MagicFallback {
    /// Decides which magic fallback applies for `receiver` (resolved to `class_name`): `__call`
    /// on `$this` when the class declares it, `$this` exists and (for a named receiver) the
    /// scope is related to the class; `__callStatic` when the class declares it.
    fn new(
        ctx: &LoweringContext<'_, '_>,
        receiver: &StaticReceiver,
        class_name: &str,
        has_this: bool,
    ) -> Self {
        let info = ctx.classes.get(class_name);
        let has_call = info.is_some_and(|info| info.methods.contains_key("__call"));
        let call_static = info.is_some_and(|info| info.static_methods.contains_key("__callstatic"));
        let related = ctx.current_class.as_deref().is_some_and(|scope| {
            super::property_access::class_extends_class(ctx, scope, class_name)
                || super::property_access::class_extends_class(ctx, class_name, scope)
        });
        let named_class = matches!(receiver, StaticReceiver::Named(_)).then(|| class_name.to_string());
        Self {
            call_on_this: has_call && has_this && (named_class.is_none() || related),
            call_static,
            named_class,
            receiver: receiver.clone(),
        }
    }

    /// Lowers the magic call when one applies, otherwise throws `error_class($message)`.
    #[allow(clippy::too_many_arguments)]
    fn call_or_throw(
        &self,
        ctx: &mut LoweringContext<'_, '_>,
        name: &Expr,
        call_args: &[Expr],
        error_class: &str,
        message: Expr,
        result_temp: &str,
        merge: BlockId,
        span: Span,
    ) {
        let packed = || {
            vec![name.clone(), Expr::new(ExprKind::ArrayLiteral(call_args.to_vec()), span)]
        };
        let static_call = |receiver: &StaticReceiver| {
            Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: receiver.clone(),
                    method: "__callStatic".to_string(),
                    args: packed(),
                },
                span,
            )
        };
        if self.call_on_this {
            let on_this_call = Expr::new(
                ExprKind::MethodCall {
                    object: Box::new(Expr::new(ExprKind::This, span)),
                    method: "__call".to_string(),
                    args: packed(),
                },
                span,
            );
            if let Some(class_name) = &self.named_class {
                let this_is_c = lower_expr(
                    ctx,
                    &Expr::new(
                        ExprKind::InstanceOf {
                            value: Box::new(Expr::new(ExprKind::This, span)),
                            target: InstanceOfTarget::Name(Name::from(class_name.clone())),
                        },
                        span,
                    ),
                );
                let on_this = ctx.builder.create_named_block("scoped_dyn_call.magic_this", Vec::new());
                let otherwise = ctx.builder.create_named_block("scoped_dyn_call.magic_else", Vec::new());
                conditional_branch(ctx, this_is_c.value, on_this, otherwise);
                ctx.builder.position_at_end(on_this);
                store_expr_into_temp(ctx, result_temp, PhpType::Mixed, &on_this_call, span);
                branch_to(ctx, merge);
                ctx.builder.position_at_end(otherwise);
                if self.call_static {
                    store_expr_into_temp(ctx, result_temp, PhpType::Mixed, &static_call(&self.receiver), span);
                    branch_to(ctx, merge);
                } else {
                    throw_error_and_merge(ctx, error_class, message, result_temp, merge, span);
                }
                return;
            }
            store_expr_into_temp(ctx, result_temp, PhpType::Mixed, &on_this_call, span);
            branch_to(ctx, merge);
            return;
        }
        if self.call_static {
            store_expr_into_temp(ctx, result_temp, PhpType::Mixed, &static_call(&self.receiver), span);
            branch_to(ctx, merge);
            return;
        }
        throw_error_and_merge(ctx, error_class, message, result_temp, merge, span);
    }
}
