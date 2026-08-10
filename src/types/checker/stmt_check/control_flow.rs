//! Purpose:
//! Validates statement control flow behavior.
//! Keeps control-flow and assignment effects synchronized with expression inference and return analysis.
//!
//! Called from:
//! - `crate::types::checker::stmt_check`
//!
//! Key details:
//! - Branch and loop handling must preserve PHP execution order and conservative type environments.
//! - A `foreach` over a PHP-visible non-iterable (`int`, `string`, `bool`, `null`, `float`,
//!   `resource`) is a WARNING, not an error: php-src raises
//!   `foreach() argument must be of type array|object, <type> given` at runtime and keeps
//!   going, so rejecting it would make elephc refuse a program PHP runs. Codegen emits the
//!   matching runtime warning and skips the loop
//!   (`IteratorSourceKind::NonIterable` in `crate::codegen::lower_inst::iterators`).
//!   Compiler-internal types with no PHP spelling stay a hard error.

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

const FS_CURRENT_AS_SELF: i64 = 16;
const FS_CURRENT_AS_PATHNAME: i64 = 32;
const FS_CURRENT_MODE_MASK: i64 = 240;
const FS_SKIP_DOTS: i64 = 4096;

/// Computes and records fixed-point array storage contracts before checking a loop body.
///
/// The shared analysis iterates over rebinds and growth sites with an evolving environment, so
/// cascading promotions, non-literal RHSs, and raw-to-raw element changes converge before any
/// header/body read is checked. EIR lowering later consumes the recorded contract for the same
/// loop span rather than repeating expression inference.
fn stabilize_loop_storage(
    checker: &mut Checker,
    loop_span: crate::span::Span,
    body: &[Stmt],
    update: Option<&Stmt>,
    env: &mut TypeEnv,
) {
    let key = (checker.current_loop_storage_scope.clone(), loop_span);
    if let Some(recorded) = checker.loop_storage_types.get(&key).cloned() {
        for (name, storage_type) in recorded {
            env.insert(name, storage_type);
        }
        return;
    }
    let snapshot = env.clone();
    let mut call_types: std::collections::HashMap<crate::span::Span, PhpType> =
        std::collections::HashMap::new();
    let contracts = crate::types::checker::loop_carried_storage_types(
        body,
        update,
        &snapshot,
        &mut |expr, analysis_env| {
            let is_call = matches!(
                expr.kind,
                ExprKind::FunctionCall { .. }
                    | ExprKind::MethodCall { .. }
                    | ExprKind::StaticMethodCall { .. }
                    | ExprKind::ClosureCall { .. }
                    | ExprKind::ExprCall { .. }
            );
            // The memo is keyed by span, so it may only answer for a span that names one call.
            // Under `Span::dummy()` every method / static / closure call in a prelude loop body
            // shared a single entry and the FIRST call's inferred type was handed to all of
            // them — 42 times while checking one `new PDO("sqlite::memory:")` program.
            let memoizable = is_call && expr.span.identifies_a_node();
            if memoizable {
                if let Some(cached) = call_types.get(&expr.span) {
                    return Some(cached.clone());
                }
            }
            let inferred = checker.infer_type(expr, analysis_env).ok()?;
            if memoizable {
                call_types.insert(expr.span, inferred.clone());
            }
            Some(inferred)
        },
    );
    let recorded = checker.loop_storage_types.entry(key).or_default();
    for (name, storage_type) in contracts {
        recorded.insert(name.clone(), storage_type.clone());
        env.insert(name, storage_type);
    }
}

/// Restores a narrowed variable in the environment to its previously saved type after a guarded
/// branch, removing it when it had no prior type. Used to keep `if`/`else` type narrowing scoped
/// to its branch.
fn restore_narrowed_var(env: &mut TypeEnv, var: &str, saved: &Option<PhpType>) {
    match saved {
        Some(ty) => {
            env.insert(var.to_string(), ty.clone());
        }
        None => {
            env.remove(var);
        }
    }
}

/// Names a `foreach` source that PHP accepts but can never iterate, or `None` when the type
/// has no PHP-visible spelling and must stay a hard compile error.
///
/// The names match what php-src prints in `foreach() argument must be of type array|object,
/// <name> given` (captured from PHP 8.5.6): `int`, `float`, `string`, `null`, `resource`, and
/// `true`/`false` for booleans. Only `PhpType::False` pins the boolean value at compile time;
/// a general `bool` is reported as `bool` here, while the RUNTIME warning emitted by
/// `__rt_warn_foreach_non_iterable` always prints the real `true`/`false`.
fn non_iterable_foreach_argument_name(ty: &PhpType) -> Option<&'static str> {
    match ty {
        PhpType::Int => Some("int"),
        PhpType::Float => Some("float"),
        PhpType::Str => Some("string"),
        PhpType::False => Some("false"),
        PhpType::Bool => Some("bool"),
        PhpType::Void => Some("null"),
        PhpType::Resource(_) => Some("resource"),
        _ => None,
    }
}

/// Returns the synthetic constructor default flags for filesystem iterators.
fn filesystem_iterator_default_flags(class_name: &str) -> Option<i64> {
    match class_name {
        "FilesystemIterator" => Some(FS_SKIP_DOTS),
        "GlobIterator" | "RecursiveDirectoryIterator" => Some(0),
        _ => None,
    }
}

impl Checker {
    /// Validates control-flow statements and updates the type environment for their assignment effects.
    ///
    /// Dispatches to specific handlers for `foreach`, `switch`, `if`, `do-while`, `while`, `for`,
    /// `throw`, and `try` constructs. Each handler infers expression types, binds loop/scoped
    /// variables to their PHP-determined types, tracks `break`/`continue` depth, and accumulates
    /// errors for malformed or incompatible constructs. Returns `Ok(())` only when all checks pass.
    pub(crate) fn check_control_flow_stmt(
        &mut self,
        stmt: &crate::parser::ast::Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body,
            } => {
                let arr_ty = self.infer_type_with_assignment_effects(array, env)?;
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if let Some(k) = key_var {
                        // A genuinely packed array has int keys; an UNKNOWN-element array (an
                        // `array`-hinted param/property, elements known only to phpdoc) may be
                        // associative at runtime, so its keys are Mixed (ward-http's
                        // `foreach ($headers as $name => $values)` with string keys).
                        let key_ty = if matches!(elem_ty.as_ref(), PhpType::Mixed) {
                            PhpType::Mixed
                        } else {
                            PhpType::Int
                        };
                        env.insert(k.clone(), key_ty);
                        self.clear_foreach_callable_metadata(k);
                    }
                    let value_ty = *elem_ty.clone();
                    env.insert(value_var.clone(), value_ty.clone());
                    self.update_foreach_callable_metadata(value_var, array, &value_ty);
                } else if let PhpType::AssocArray { key, value } = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), *key.clone());
                        self.clear_foreach_callable_metadata(k);
                    }
                    let value_ty = *value.clone();
                    env.insert(value_var.clone(), value_ty.clone());
                    self.update_foreach_callable_metadata(value_var, array, &value_ty);
                } else if let PhpType::Object(class_name) = &arr_ty {
                    let is_iter = self.class_implements_interface(class_name, "Iterator")
                        || self.interface_extends_interface(class_name, "Iterator");
                    let is_iter_agg = self
                        .class_implements_interface(class_name, "IteratorAggregate")
                        || self.interface_extends_interface(class_name, "IteratorAggregate");
                    if !is_iter && !is_iter_agg {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "foreach over object requires {} to implement Iterator or IteratorAggregate",
                                class_name
                            ),
                        ));
                    }
                    let (key_ty, value_ty) =
                        self.foreach_object_key_value_types(class_name, array);
                    if let Some(k) = key_var {
                        env.insert(k.clone(), key_ty);
                        self.clear_foreach_callable_metadata(k);
                    }
                    env.insert(value_var.clone(), value_ty);
                    self.clear_foreach_callable_metadata(value_var);
                } else if matches!(
                    arr_ty,
                    PhpType::Iterable | PhpType::Mixed | PhpType::Union(_)
                ) {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Mixed);
                        self.clear_foreach_callable_metadata(k);
                    }
                    env.insert(value_var.clone(), PhpType::Mixed);
                    self.clear_foreach_callable_metadata(value_var);
                } else if let Some(type_name) = non_iterable_foreach_argument_name(&arr_ty) {
                    // php-src does NOT reject this: `ZEND_FE_RESET_R` raises
                    // `foreach() argument must be of type array|object, <type> given`
                    // (E_WARNING), skips the loop body, and execution continues. Mirroring
                    // that as a hard error would make elephc reject a program PHP runs, so
                    // the diagnostic is a compile warning and codegen emits the same
                    // runtime warning (`IteratorSourceKind::NonIterable` in
                    // `src/codegen/lower_inst/iterators.rs`). Compiler-internal types
                    // (`Packed`, `Pointer`, `Buffer`, `Never`, `Callable`, `TaggedScalar`)
                    // have no PHP-visible spelling and stay a hard error below.
                    self.warnings.push(crate::errors::CompileWarning::new(
                        stmt.span,
                        &format!(
                            "foreach() argument must be of type array|object, {} given; the loop body will never run",
                            type_name
                        ),
                    ));
                    // The body is still type-checked, so bind both loop variables the way
                    // the `Mixed` source path does.
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Mixed);
                        self.clear_foreach_callable_metadata(k);
                    }
                    env.insert(value_var.clone(), PhpType::Mixed);
                    self.clear_foreach_callable_metadata(value_var);
                } else {
                    return Err(CompileError::new(
                        stmt.span,
                        "foreach requires an array, iterable, or an object implementing Iterator/IteratorAggregate",
                    ));
                }
                // A foreach key is a boxed `Mixed` cell at runtime regardless of
                // the source array's key type, so record the bound name so that a
                // `$dst[$k] = $v` write under it defers to `Op::ArraySetMixedKey`
                // instead of promoting the destination to `AssocArray`.
                if let Some(k) = key_var {
                    self.foreach_key_locals.insert(k.clone());
                }
                if *value_by_ref && matches!(arr_ty, PhpType::Object(_) | PhpType::Iterable) {
                    return Err(CompileError::new(
                        stmt.span,
                        "by-reference foreach over Iterator/IteratorAggregate objects or iterable-typed values is not supported; use an array source or remove &",
                    ));
                }
                // Widen after the key/value bindings are in the environment so a push of
                // the foreach value variable joins with its real element type.
                stabilize_loop_storage(self, stmt.span, body, None, env);
                let errors = self.check_break_continue_target_body(body, env);
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.infer_type_with_assignment_effects(subject, env)?;
                let mut errors = Vec::new();
                for (values, _) in cases {
                    for v in values {
                        self.infer_type_with_assignment_effects(v, env)?;
                    }
                }
                self.break_continue_depth += 1;
                for (_, body) in cases {
                    errors.extend(self.check_body(body, env));
                }
                if let Some(body) = default {
                    errors.extend(self.check_body(body, env));
                }
                self.break_continue_depth -= 1;
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                let mut errors = Vec::new();

                // Flow-sensitive type narrowing across the if / elseif* / else chain.
                //
                // Each recognized guard narrows its variable to the guarded type while checking
                // that branch's body. The fallthrough env for the remaining clauses (and the final
                // else) accumulates the complement, which is sound because reaching a later clause
                // means every earlier condition was false.
                //
                // After the whole construct we restore every variable we narrowed, so code after
                // the `if` sees the joined view. The single exception is an exhaustively divergent
                // chain (no else and *every* clause body diverges): there the only way to fall
                // through is with all conditions false, so the accumulated complement is sound for
                // the statements after the `if`.
                let mut clauses: Vec<(&Expr, &Vec<Stmt>)> = vec![(condition, then_body)];
                clauses.extend(elseif_clauses.iter().map(|(c, b)| (c, b)));

                // Pre-`if` type of every variable we narrow, captured the first time we touch it,
                // so each one can be restored after the construct.
                let mut saved_vars: Vec<(String, Option<PhpType>)> = Vec::new();
                let mut applied_any_guard = false;
                // Single-clause join state: the guarded key and the type it has where the
                // then-branch falls out of the construct. `None` means "no usable fact" (the
                // branch diverged, or a call inside it purged the narrowing).
                let mut join_key: Option<String> = None;
                let mut then_exit_ty: Option<PhpType> = None;
                let single_clause = clauses.len() == 1;

                for (cond, body) in &clauses {
                    self.infer_type_with_assignment_effects(cond, env)?;

                    if let Some(guard) = self.guard_narrowing(cond, env)? {
                        applied_any_guard = true;
                        // Remember the variable's pre-`if` type the first time we narrow it.
                        if !saved_vars.iter().any(|(v, _)| v == &guard.var) {
                            saved_vars.push((guard.var.clone(), env.get(&guard.var).cloned()));
                        }

                        // Check the guarded body with the "then" type.
                        let saved = env.get(&guard.var).cloned();
                        env.insert(guard.var.clone(), guard.then_ty.clone());
                        for s in *body {
                            if let Err(error) = self.check_stmt(s, env) {
                                errors.extend(error.flatten());
                            }
                        }
                        // Join only when the then-branch WROTE the guarded place, i.e. when the
                        // fact at branch exit is no longer the guard's own `then` type. That is
                        // exactly the lazy-initialization shape; joining unconditionally would
                        // instead publish a guard's narrowing (e.g. `instanceof`) to the code
                        // after the `if`, where it does not hold.
                        let branch_exit = env.get(&guard.var);
                        if single_clause
                            && Self::narrowed_place_key_is_property(&guard.var)
                            && !self.body_cannot_fall_through(body)
                            && branch_exit.is_some_and(|ty| *ty != guard.then_ty)
                        {
                            join_key = Some(guard.var.clone());
                            then_exit_ty = branch_exit.cloned();
                        }
                        restore_narrowed_var(env, &guard.var, &saved);

                        // The fallthrough env for the rest of the chain (next elseif or else)
                        // sees the complement.
                        env.insert(guard.var.clone(), guard.else_ty.clone());
                    } else {
                        // No narrowing for this clause — check the body with the current env.
                        for s in *body {
                            if let Err(error) = self.check_stmt(s, env) {
                                errors.extend(error.flatten());
                            }
                        }
                    }
                }

                // Final else body (if present) is checked with the accumulated complement.
                // `None` = the else path cannot reach the code after the `if`. `Some(None)` = it
                // can, but the guarded fact was lost there. `Some(Some(ty))` = it can and the
                // fact is `ty`.
                let mut else_exit_ty: Option<Option<PhpType>> = None;
                let mut else_falls_through = else_body.is_none();
                if let Some(body) = else_body {
                    for s in body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                    else_falls_through = !self.body_cannot_fall_through(body);
                }
                if let Some(key) = &join_key {
                    if else_falls_through {
                        else_exit_ty = Some(env.get(key).cloned());
                    }
                }

                // Keep the accumulated complement for the statements after the `if` only when no
                // guarded clause can fall through: there is no else and every clause ends in a
                // non-fallthrough statement, so reaching the following code implies all conditions
                // were false. Otherwise restore every narrowed variable to its pre-`if` type.
                let keep_complement_after_if = applied_any_guard
                    && else_body.is_none()
                    && clauses
                        .iter()
                        .all(|(_, body)| self.body_cannot_fall_through(body));
                // A single guarded clause whose then-branch also falls through joins the two
                // exit facts instead of discarding both. `if (X === null) { X = new S(); }`
                // leaves `S` on the then path (the write recorded it) and `S` on the else path
                // (the guard complement), so the union is `S` — the singleton pattern.
                let joined = join_key.as_ref().and_then(|key| {
                    let then_ty = then_exit_ty.clone()?;
                    let joined = match &else_exit_ty {
                        None => then_ty,
                        Some(Some(else_ty)) => {
                            self.normalize_union_type(vec![then_ty, else_ty.clone()])
                        }
                        Some(None) => return None,
                    };
                    Some((key.clone(), joined))
                });
                if !keep_complement_after_if {
                    for (var, original) in &saved_vars {
                        restore_narrowed_var(env, var, original);
                    }
                    if let Some((key, joined)) = joined {
                        env.insert(key, joined);
                    }
                }

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::DoWhile { body, condition } => {
                stabilize_loop_storage(self, stmt.span, body, None, env);
                let errors = self.check_break_continue_target_body(body, env);
                self.infer_type_with_assignment_effects(condition, env)?;
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::While { condition, body } => {
                stabilize_loop_storage(self, stmt.span, body, None, env);
                self.infer_type_with_assignment_effects(condition, env)?;
                // The condition is re-evaluated before every iteration, so a guard on it
                // holds on entry to each one: `while (($row = fgetcsv($h)) !== false)`
                // leaves `$row` an array inside the body. The narrowing is dropped again
                // afterwards, because the loop exits precisely when the guard is false.
                let guard = self.guard_narrowing(condition, env)?;
                let saved = guard
                    .as_ref()
                    .map(|g| (g.var.clone(), env.get(&g.var).cloned()));
                if let Some(g) = &guard {
                    env.insert(g.var.clone(), g.then_ty.clone());
                }
                let errors = self.check_break_continue_target_body(body, env);
                if let Some((var, previous)) = saved {
                    match previous {
                        Some(ty) => {
                            env.insert(var, ty);
                        }
                        None => {
                            env.remove(&var);
                        }
                    }
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(s) = init {
                    self.check_stmt(s, env)?;
                }
                stabilize_loop_storage(self, stmt.span, body, update.as_deref(), env);
                if let Some(c) = condition {
                    self.infer_type_with_assignment_effects(c, env)?;
                }
                if let Some(s) = update {
                    self.check_stmt(s, env)?;
                }
                let errors = self.check_break_continue_target_body(body, env);
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            StmtKind::Throw(expr) => {
                let thrown_ty = self.infer_type_with_assignment_effects(expr, env)?;
                match thrown_ty {
                    PhpType::Object(type_name)
                        if self.object_type_implements_throwable(&type_name) =>
                    {
                        Ok(())
                    }
                    PhpType::Object(_) => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object implementing Throwable",
                    )),
                    _ => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object value",
                    )),
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let mut errors = Vec::new();
                for s in try_body {
                    if let Err(error) = self.check_stmt(s, env) {
                        errors.extend(error.flatten());
                    }
                }
                for catch_clause in catches {
                    let mut resolved_types = Vec::new();
                    for raw_exception_type in &catch_clause.exception_types {
                        let exception_type =
                            self.resolve_catch_type_name(raw_exception_type, stmt.span)?;
                        if !self.classes.contains_key(&exception_type)
                            && !self.interfaces.contains_key(&exception_type)
                        {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined class: {}", exception_type),
                            ));
                        }
                        if !self.object_type_implements_throwable(&exception_type) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Catch type must extend or implement Throwable: {}",
                                    exception_type
                                ),
                            ));
                        }
                        resolved_types.push(exception_type);
                    }
                    if let Some(variable) = &catch_clause.variable {
                        env.insert(
                            variable.clone(),
                            PhpType::Object(self.common_catch_type_name(&resolved_types)),
                        );
                    }
                    for s in &catch_clause.body {
                        if let Err(error) = self.check_stmt(s, env) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Some(body) = finally_body {
                    self.finally_break_continue_bases
                        .push(self.break_continue_depth);
                    errors.extend(self.check_body(body, env));
                    self.finally_break_continue_bases.pop();
                }
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(CompileError::from_many(errors))
                }
            }
            _ => unreachable!("non-control-flow statement routed to control-flow checker"),
        }
    }

    /// Returns the static key and value types exposed by foreach over an object iterator.
    ///
    /// Concrete `Iterator` implementations can narrow `key()`/`current()` from the
    /// interface's `mixed` contract, so foreach should expose those narrower types inside
    /// the loop. IteratorAggregate sources are resolved through their `getIterator()`
    /// return type when that type is statically known.
    fn foreach_object_key_value_types(
        &self,
        class_name: &str,
        source: &Expr,
    ) -> (PhpType, PhpType) {
        let value_override = self.foreach_object_value_type_override(class_name, source);
        if self.class_implements_interface(class_name, "Iterator")
            || self.interface_extends_interface(class_name, "Iterator")
        {
            return (
                self.iterator_method_return_type(class_name, "key"),
                value_override.unwrap_or_else(|| {
                    self.iterator_method_return_type(class_name, "current")
                }),
            );
        }

        let get_iterator_ty = self.iterator_method_return_type(class_name, "getIterator");
        if let PhpType::Object(iterator_name) = get_iterator_ty {
            return (
                self.iterator_method_return_type(&iterator_name, "key"),
                value_override.unwrap_or_else(|| {
                    self.iterator_method_return_type(&iterator_name, "current")
                }),
            );
        }

        (PhpType::Mixed, PhpType::Mixed)
    }

    /// Returns a narrower foreach value type for SPL filesystem iterators when flags are static.
    fn foreach_object_value_type_override(
        &self,
        class_name: &str,
        source: &Expr,
    ) -> Option<PhpType> {
        if class_name == "DirectoryIterator" {
            return Some(PhpType::Object("DirectoryIterator".to_string()));
        }
        let flags = self.filesystem_iterator_source_flags(class_name, source)?;
        match flags & FS_CURRENT_MODE_MASK {
            FS_CURRENT_AS_PATHNAME => None,
            FS_CURRENT_AS_SELF => Some(PhpType::Object(class_name.to_string())),
            _ => Some(PhpType::Object("SplFileInfo".to_string())),
        }
    }

    /// Returns constructor flags for statically constructed filesystem iterators.
    fn filesystem_iterator_source_flags(&self, class_name: &str, source: &Expr) -> Option<i64> {
        if !matches!(
            class_name,
            "FilesystemIterator" | "GlobIterator" | "RecursiveDirectoryIterator"
        ) {
            return None;
        }
        let ExprKind::NewObject {
            class_name: source_class,
            args,
        } = &source.kind
        else {
            return None;
        };
        if source_class.as_str() != class_name {
            return None;
        }
        args.get(1)
            .and_then(|expr| self.eval_static_int_expr(expr))
            .or_else(|| filesystem_iterator_default_flags(class_name))
    }

    /// Evaluates a side-effect-free integer expression used for SPL flag constants.
    fn eval_static_int_expr(&self, expr: &Expr) -> Option<i64> {
        match &expr.kind {
            ExprKind::IntLiteral(value) => Some(*value),
            ExprKind::Negate(inner) => self.eval_static_int_expr(inner).map(|value| -value),
            ExprKind::BitNot(inner) => self.eval_static_int_expr(inner).map(|value| !value),
            ExprKind::BinaryOp { left, op, right } => {
                let left = self.eval_static_int_expr(left)?;
                let right = self.eval_static_int_expr(right)?;
                match op {
                    BinOp::BitOr => Some(left | right),
                    BinOp::BitAnd => Some(left & right),
                    BinOp::BitXor => Some(left ^ right),
                    BinOp::Add => Some(left + right),
                    BinOp::Sub => Some(left - right),
                    _ => None,
                }
            }
            ExprKind::ScopedConstantAccess { receiver, name } => {
                self.class_constant_int_value(receiver, name)
            }
            _ => None,
        }
    }

    /// Resolves a class constant integer value from checker metadata.
    fn class_constant_int_value(&self, receiver: &StaticReceiver, name: &str) -> Option<i64> {
        let StaticReceiver::Named(class_name) = receiver else {
            return None;
        };
        self.classes
            .get(class_name.as_str())
            .and_then(|class_info| class_info.constants.get(name))
            .and_then(|expr| self.eval_static_int_expr(expr))
    }

    /// Looks up an iterator-related method return type on either a class or an interface.
    ///
    /// Missing metadata falls back to `mixed`, matching PHP's loose iterator contracts and
    /// preserving the previous conservative behavior for dynamic or unknown iterator shapes.
    fn iterator_method_return_type(&self, type_name: &str, method: &str) -> PhpType {
        let method_key = crate::names::php_symbol_key(method);
        if type_name == "DirectoryIterator" && method_key == "current" {
            return PhpType::Object("DirectoryIterator".to_string());
        }
        if let Some(class_info) = self.classes.get(type_name) {
            return class_info
                .methods
                .get(&method_key)
                .map(|sig| sig.return_type.clone())
                .unwrap_or(PhpType::Mixed);
        }
        self.interfaces
            .get(type_name)
            .and_then(|interface_info| interface_info.methods.get(&method_key))
            .map(|sig| sig.return_type.clone())
            .unwrap_or(PhpType::Mixed)
    }

    /// Checks a loop body with `break`/`continue` target tracking.
    ///
    /// Increments `break_continue_depth` before checking the body and decrements it after,
    /// so that `break`/`continue` validation knows the correct nesting level. Returns all
    /// errors accumulated while checking the body; the caller decides whether to propagate them.
    fn check_break_continue_target_body(
        &mut self,
        body: &[Stmt],
        env: &mut TypeEnv,
    ) -> Vec<CompileError> {
        // A loop body may observe a property write made by an earlier iteration (the write site
        // is *after* the read site in source order), so property narrowings from outside the
        // loop cannot be trusted inside it.
        Self::purge_property_narrowings(env);
        self.break_continue_depth += 1;
        let errors = self.check_body(body, env);
        self.break_continue_depth -= 1;
        errors
    }

    /// Updates callable metadata for a foreach value variable.
    ///
    /// Homogeneous arrays that store callable descriptors keep their signature
    /// and capture metadata under the source array variable name. A foreach value
    /// binding from that array must expose the same metadata to calls emitted in
    /// the loop body.
    fn update_foreach_callable_metadata(
        &mut self,
        dest: &str,
        source_array: &Expr,
        value_ty: &PhpType,
    ) {
        if value_ty != &PhpType::Callable {
            self.clear_foreach_callable_metadata(dest);
            return;
        }
        if let ExprKind::Variable(src_name) = &source_array.kind {
            self.copy_foreach_callable_metadata(dest, src_name);
        } else {
            self.clear_foreach_callable_metadata(dest);
        }
    }

    /// Copies callable signature, capture, first-class target, and callable-array metadata.
    fn copy_foreach_callable_metadata(&mut self, dest: &str, src: &str) {
        if let Some(return_ty) = self.closure_return_types.get(src).cloned() {
            self.closure_return_types.insert(dest.to_string(), return_ty);
        } else {
            self.closure_return_types.remove(dest);
        }
        if let Some(sig) = self.callable_sigs.get(src).cloned() {
            self.callable_sigs.insert(dest.to_string(), sig);
        } else {
            self.callable_sigs.remove(dest);
        }
        if let Some(captures) = self.callable_captures.get(src).cloned() {
            self.callable_captures.insert(dest.to_string(), captures);
        } else {
            self.callable_captures.remove(dest);
        }
        if let Some(target) = self.callable_array_targets.get(src).cloned() {
            self.callable_array_targets
                .insert(dest.to_string(), target);
        } else {
            self.callable_array_targets.remove(dest);
        }
        if let Some(target) = self.first_class_callable_targets.get(src).cloned() {
            self.first_class_callable_targets
                .insert(dest.to_string(), target);
        } else {
            self.first_class_callable_targets.remove(dest);
        }
    }

    /// Clears callable metadata for a foreach key or value binding.
    fn clear_foreach_callable_metadata(&mut self, dest: &str) {
        self.closure_return_types.remove(dest);
        self.callable_sigs.remove(dest);
        self.callable_captures.remove(dest);
        self.callable_array_targets.remove(dest);
        self.first_class_callable_targets.remove(dest);
    }

    /// Checks each statement in a body sequentially, collecting all errors.
    ///
    /// Unlike `check_break_continue_target_body`, this does not update `break_continue_depth`.
    /// Used for `switch` cases, `if` branches, `try` blocks, and other bodies where the
    /// break/continue level is managed at a higher level.
    fn check_body(&mut self, body: &[Stmt], env: &mut TypeEnv) -> Vec<CompileError> {
        let mut errors = Vec::new();
        for stmt in body {
            if let Err(error) = self.check_stmt(stmt, env) {
                errors.extend(error.flatten());
            }
        }
        errors
    }
}
