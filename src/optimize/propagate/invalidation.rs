//! Purpose:
//! Targeted local-write invalidation for constant propagation: computes which
//! caller locals a statement/expression can write, replacing the historical
//! "any side effect clears the whole environment" rule.
//!
//! Called from:
//! - `crate::optimize::propagate` statement/expression environment updates and
//!   the loop-safe environment helpers.
//!
//! Key details:
//! - Grounded in PHP's memory model: a callee can write a caller local only
//!   through a by-ref parameter, an exposed reference (all reference-exposure
//!   points mark names volatile, so they never carry facts), or global storage
//!   (which only aliases *top-level* locals — `global`-bound names inside
//!   functions are volatile).
//! - By-ref arguments to user-defined callees are also marked volatile: the
//!   callee may retain the reference (e.g. in a by-ref closure capture) and
//!   write it during any later call. Builtins never retain their arguments.
//! - `Invalidation::All` remains for genuinely unknowable writes: `include`,
//!   `yield`, spreads into by-ref callees, and global-writing (or unknown)
//!   callees invoked from top-level scope.

use super::*;

/// The set of caller locals a construct can write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Invalidation {
    /// Exactly these locals may be written.
    Names(HashSet<String>),
    /// The write set is unknowable — the whole environment must be cleared.
    All,
}

impl Invalidation {
    /// The empty invalidation: no local can be written.
    pub(crate) fn none() -> Self {
        Invalidation::Names(HashSet::new())
    }

    /// Unions another invalidation into this one; `All` absorbs everything.
    pub(crate) fn union(self, other: Self) -> Self {
        match (self, other) {
            (Invalidation::Names(mut left), Invalidation::Names(right)) => {
                left.extend(right);
                Invalidation::Names(left)
            }
            _ => Invalidation::All,
        }
    }

    /// Adds one written name (no-op on `All`).
    pub(crate) fn add(&mut self, name: &str) {
        if let Invalidation::Names(names) = self {
            names.insert(name.to_string());
        }
    }

    /// Removes the written names from `env` (`All` clears it).
    pub(crate) fn apply(&self, env: &mut ConstantEnv) {
        match self {
            Invalidation::Names(names) => {
                for name in names {
                    env.remove(name);
                }
            }
            Invalidation::All => env.clear(),
        }
    }
}

/// Computes the caller locals an expression can write, including through the
/// calls it performs.
pub(crate) fn expr_invalidation(expr: &Expr) -> Invalidation {
    // Fast path: a `Some` from the write collector proves the expression
    // contains no calls, complex `unset`s, pipes, or yields anywhere — the
    // collected names are the exact write set.
    if let Some(writes) = expr_local_writes(expr) {
        return Invalidation::Names(writes);
    }
    expr_invalidation_slow(expr)
}

/// The recursive walk behind `expr_invalidation`, used when the expression
/// contains at least one call-like construct.
fn expr_invalidation_slow(expr: &Expr) -> Invalidation {
    match &expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => unreachable!(
            "ExprKind::IncludeValue must be expanded by the resolver"
        ),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. } => Invalidation::none(),
        // Creating a closure executes nothing, but its by-ref captures alias
        // the outer variables from this point on: any existing fact for them
        // must die here (the volatility ledger only blocks *future* facts).
        ExprKind::Closure { capture_refs, .. } => {
            Invalidation::Names(capture_refs.iter().cloned().collect())
        }
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Method { object, .. } => expr_invalidation(object),
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => {
                Invalidation::none()
            }
        },
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::BufferNew { len: inner, .. }
        | ExprKind::NamedArg { value: inner, .. } => expr_invalidation(inner),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_invalidation(left).union(expr_invalidation(right))
        }
        ExprKind::InstanceOf { value, target } => {
            let target_inv = match target {
                InstanceOfTarget::Name(_) => Invalidation::none(),
                InstanceOfTarget::Expr(expr) => expr_invalidation(expr),
            };
            expr_invalidation(value).union(target_inv)
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_invalidation(value).union(expr_invalidation(default))
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_invalidation(array).union(expr_invalidation(index))
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_invalidation(condition)
            .union(expr_invalidation(then_expr))
            .union(expr_invalidation(else_expr)),
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .fold(Invalidation::none(), |acc, item| {
                acc.union(expr_invalidation(item))
            }),
        ExprKind::ArrayLiteralAssoc(items) => {
            items.iter().fold(Invalidation::none(), |acc, (key, value)| {
                acc.union(expr_invalidation(key))
                    .union(expr_invalidation(value))
            })
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let mut inv = expr_invalidation(subject);
            for (patterns, value) in arms {
                for pattern in patterns {
                    inv = inv.union(expr_invalidation(pattern));
                }
                inv = inv.union(expr_invalidation(value));
            }
            if let Some(default) = default {
                inv = inv.union(expr_invalidation(default));
            }
            inv
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_invalidation(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_invalidation(object).union(expr_invalidation(property))
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => {
            let mut inv = block_invalidation(prelude)
                .union(expr_invalidation(value))
                .union(assignment_target_invalidation(target));
            // `result_target` is the read-back expression for the assignment's
            // value (rhs clone, target, or a prelude-bound temp) — a read, not
            // an extra write target.
            if let Some(result_target) = result_target {
                inv = inv.union(expr_invalidation(result_target));
            }
            if let Some(temp) = conditional_value_temp {
                inv.add(temp);
            }
            inv
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => {
            Invalidation::Names(HashSet::from([name.clone()]))
        }
        ExprKind::FunctionCall { name, args } if name == "unset" => {
            args.iter().fold(Invalidation::none(), |acc, arg| {
                acc.union(unset_target_invalidation(arg))
            })
        }
        ExprKind::FunctionCall { name, args } => {
            // A callback-invoking builtin runs arbitrary user code with the
            // caller's arguments (`call_user_func($cb, $value)` forwards
            // `$value` to a possibly-by-ref parameter), so its arguments are
            // treated like an unknown callee's.
            let callee_inv = if builtin_invokes_callbacks(name.as_str()) {
                call_args_invalidation(None, args, true)
            } else {
                call_args_invalidation(
                    function_by_ref_params(name.as_str()).as_deref(),
                    args,
                    is_user_function(name.as_str()),
                )
            };
            args_invalidation(args)
                .union(callee_inv)
                .union(top_level_globals_guard(function_call_effect(name.as_str())))
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => {
            let callee_inv =
                call_args_invalidation(method_by_ref_params(method).as_deref(), args, true);
            expr_invalidation(object)
                .union(args_invalidation(args))
                .union(callee_inv)
                .union(top_level_globals_guard(private_instance_method_call_effect(
                    object, method,
                )))
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            let callee_inv =
                call_args_invalidation(method_by_ref_params(method).as_deref(), args, true);
            args_invalidation(args)
                .union(callee_inv)
                .union(top_level_globals_guard(static_method_call_effect(
                    receiver, method,
                )))
        }
        ExprKind::ClosureCall { var, args } => args_invalidation(args)
            .union(call_args_invalidation(None, args, true))
            .union(top_level_globals_guard(callable_alias_effect(var))),
        ExprKind::ExprCall { callee, args } => expr_invalidation(callee)
            .union(args_invalidation(args))
            .union(call_args_invalidation(None, args, true))
            .union(top_level_globals_guard(expr_call_effect(callee))),
        ExprKind::Pipe { value, callable } => {
            // `$v |> $f` invokes the callable with the piped value as its
            // single argument; resolve the signature when the callable names a
            // function directly.
            let piped = std::slice::from_ref(value.as_ref());
            let callee_inv = match &callable.kind {
                ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
                    call_args_invalidation(
                        function_by_ref_params(name.as_str()).as_deref(),
                        piped,
                        is_user_function(name.as_str()),
                    )
                }
                _ => call_args_invalidation(None, piped, true),
            };
            expr_invalidation(value)
                .union(expr_invalidation(callable))
                .union(callee_inv)
                .union(top_level_globals_guard(expr_call_effect(callable)))
        }
        ExprKind::NewObject { args, .. }
        | ExprKind::NewDynamic { args, .. }
        | ExprKind::NewDynamicObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            let ctor_inv = if any_ctor_by_ref() {
                // Some constructor can bind an argument by reference (by-ref
                // ctor param or by-ref property); without per-class resolution
                // every lvalue-rooted argument is exposed.
                call_args_invalidation(None, args, true)
            } else {
                Invalidation::none()
            };
            let extra = match &expr.kind {
                ExprKind::NewDynamic { name_expr, .. } => expr_invalidation(name_expr),
                ExprKind::NewDynamicObject { class_name, .. } => expr_invalidation(class_name),
                _ => Invalidation::none(),
            };
            // Constructor bodies have no effect summaries, so a top-level
            // `new` stays as conservative as the blanket rule it replaces.
            let top_level = if in_function_scope() {
                Invalidation::none()
            } else {
                Invalidation::All
            };
            args_invalidation(args).union(ctor_inv).union(extra).union(top_level)
        }
        ExprKind::Yield { .. } | ExprKind::YieldFrom(_) => Invalidation::All,
    }
}

/// Unions the invalidations of each argument's own evaluation (nested calls,
/// increments, inline assignments) — separate from what the callee can write.
fn args_invalidation(args: &[Expr]) -> Invalidation {
    args.iter().fold(Invalidation::none(), |acc, arg| {
        acc.union(expr_invalidation(arg))
    })
}

/// Computes what a callee can write through its by-ref parameters.
///
/// `sig` is the callee's `(param name, is_by_ref)` list; `None` means the
/// callee is unknown, in which case every lvalue-rooted argument is treated as
/// exposed. When `retain` is true (user-defined callees) the exposed roots are
/// also marked volatile: the callee may keep the reference alive past the call.
fn call_args_invalidation(
    sig: Option<&[(String, bool)]>,
    args: &[Expr],
    retain: bool,
) -> Invalidation {
    let mut inv = Invalidation::none();
    let Some(sig) = sig else {
        for arg in args {
            expose_argument_root(arg, retain, &mut inv);
        }
        return inv;
    };
    let has_by_ref = sig.iter().any(|(_, is_ref)| *is_ref);
    let mut spread_seen = false;
    let mut position = 0usize;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                if sig
                    .iter()
                    .any(|(param, is_ref)| param == name && *is_ref)
                {
                    expose_argument_root(value, retain, &mut inv);
                }
            }
            ExprKind::Spread(_) => {
                if has_by_ref {
                    // The spread's runtime length decides which elements land
                    // on by-ref positions; give up precisely tracking it.
                    return Invalidation::All;
                }
                spread_seen = true;
            }
            _ => {
                if spread_seen && has_by_ref {
                    return Invalidation::All;
                }
                if sig.get(position).is_some_and(|(_, is_ref)| *is_ref) {
                    expose_argument_root(arg, retain, &mut inv);
                }
                position += 1;
            }
        }
    }
    inv
}

/// Records an argument's lvalue root as writable (and volatile when the callee
/// can retain the reference). Arguments without a local root — literals,
/// property accesses, call results — expose no caller local.
fn expose_argument_root(arg: &Expr, retain: bool, inv: &mut Invalidation) {
    if let Some(root) = lvalue_root(arg) {
        if retain {
            super::stmt::mark_reference_volatile(root);
        }
        inv.add(root);
    }
}

/// At top level every local is a global, so a callee that can write global
/// storage can write any of them.
fn top_level_globals_guard(effect: Effect) -> Invalidation {
    if !in_function_scope() && effect.writes_globals {
        Invalidation::All
    } else {
        Invalidation::none()
    }
}

/// Computes the locals written by one `unset()` argument: the lvalue root for
/// variables and array-access chains, nothing for property targets (they
/// mutate heap state), `All` for unrecognized shapes.
fn unset_target_invalidation(arg: &Expr) -> Invalidation {
    match &arg.kind {
        ExprKind::Variable(name) => Invalidation::Names(HashSet::from([name.clone()])),
        ExprKind::ArrayAccess { array, index } => {
            let mut inv = expr_invalidation(index);
            match lvalue_root(arg) {
                Some(root) => inv.add(root),
                // No local root (e.g. `unset($obj->list[0])`): the write hits
                // heap state reached through the object expression.
                None => inv = inv.union(expr_invalidation(array)),
            }
            inv
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_invalidation(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_invalidation(object).union(expr_invalidation(property))
        }
        ExprKind::StaticPropertyAccess { .. } => Invalidation::none(),
        _ => Invalidation::All,
    }
}

/// Computes the locals written by an assignment target: the lvalue root plus
/// anything its sub-expressions (indices, objects) can write. Unrecognized
/// target shapes are unknowable.
fn assignment_target_invalidation(target: &Expr) -> Invalidation {
    match &target.kind {
        ExprKind::Variable(name) => Invalidation::Names(HashSet::from([name.clone()])),
        ExprKind::ArrayAccess { array, index } => {
            let mut inv = expr_invalidation(index);
            match lvalue_root(target) {
                Some(root) => inv.add(root),
                None => inv = inv.union(expr_invalidation(array)),
            }
            inv
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_invalidation(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_invalidation(object).union(expr_invalidation(property))
        }
        ExprKind::StaticPropertyAccess { .. } => Invalidation::none(),
        _ => Invalidation::All,
    }
}

/// Computes the caller locals a statement can write, mirroring
/// `stmt_local_writes` with call-aware precision.
pub(crate) fn stmt_invalidation(stmt: &Stmt) -> Invalidation {
    match &stmt.kind {
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            block_invalidation(stmts)
        }
        StmtKind::Echo(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Throw(expr)
        | StmtKind::Return(Some(expr)) => expr_invalidation(expr),
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        | StmtKind::FunctionVariantGroup { .. } => Invalidation::none(),
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let mut inv = expr_invalidation(value);
            inv.add(name);
            inv
        }
        StmtKind::RefAssign { target, source } => {
            let mut inv = expr_invalidation(source);
            inv.add(target);
            if let Some(root) = lvalue_root(source) {
                inv.add(root);
            }
            inv
        }
        StmtKind::ListUnpack { vars, value } => {
            let mut inv = expr_invalidation(value);
            for var in vars {
                inv.add(var);
            }
            inv
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            let mut inv = expr_invalidation(array).union(block_invalidation(body));
            if *value_by_ref {
                if let Some(root) = lvalue_root(array) {
                    inv.add(root);
                }
            }
            inv.add(value_var);
            if let Some(key_var) = key_var {
                inv.add(key_var);
            }
            inv
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_invalidation(condition).union(block_invalidation(body))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let mut inv = block_invalidation(body);
            if let Some(init) = init {
                inv = inv.union(stmt_invalidation(init));
            }
            if let Some(condition) = condition {
                inv = inv.union(expr_invalidation(condition));
            }
            if let Some(update) = update {
                inv = inv.union(stmt_invalidation(update));
            }
            inv
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let mut inv = expr_invalidation(condition).union(block_invalidation(then_body));
            for (elseif_condition, elseif_body) in elseif_clauses {
                inv = inv
                    .union(expr_invalidation(elseif_condition))
                    .union(block_invalidation(elseif_body));
            }
            if let Some(else_body) = else_body {
                inv = inv.union(block_invalidation(else_body));
            }
            inv
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let mut inv = block_invalidation(then_body);
            if let Some(else_body) = else_body {
                inv = inv.union(block_invalidation(else_body));
            }
            inv
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            let mut inv = expr_invalidation(subject);
            for (patterns, body) in cases {
                for pattern in patterns {
                    inv = inv.union(expr_invalidation(pattern));
                }
                inv = inv.union(block_invalidation(body));
            }
            if let Some(default) = default {
                inv = inv.union(block_invalidation(default));
            }
            inv
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut inv = block_invalidation(try_body);
            for catch in catches {
                if let Some(variable) = &catch.variable {
                    inv.add(variable);
                }
                inv = inv.union(block_invalidation(&catch.body));
            }
            if let Some(finally_body) = finally_body {
                inv = inv.union(block_invalidation(finally_body));
            }
            inv
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            let mut inv = expr_invalidation(index).union(expr_invalidation(value));
            inv.add(array);
            inv
        }
        StmtKind::NestedArrayAssign { target, value } => {
            assignment_target_invalidation(target).union(expr_invalidation(value))
        }
        StmtKind::ArrayPush { array, value } => {
            let mut inv = expr_invalidation(value);
            inv.add(array);
            inv
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_invalidation(object).union(expr_invalidation(value))
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_invalidation(object)
            .union(expr_invalidation(index))
            .union(expr_invalidation(value)),
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_invalidation(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_invalidation(index).union(expr_invalidation(value))
        }
        StmtKind::Global { vars } => Invalidation::Names(vars.iter().cloned().collect()),
        StmtKind::StaticVar { name, init } => {
            let mut inv = expr_invalidation(init);
            inv.add(name);
            inv
        }
        // Includes splice statements into the current scope; variant marks swap
        // which function definitions the effect summaries describe.
        StmtKind::Include { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantMark { .. } => Invalidation::All,
        StmtKind::IncludeOnceGuard { body, .. } => block_invalidation(body),
    }
}

/// Unions the invalidations of every statement in a block (no terminal-effect
/// shortcut: loop bodies re-execute, so every statement counts).
pub(crate) fn block_invalidation(body: &[Stmt]) -> Invalidation {
    body.iter().fold(Invalidation::none(), |acc, stmt| {
        acc.union(stmt_invalidation(stmt))
    })
}
