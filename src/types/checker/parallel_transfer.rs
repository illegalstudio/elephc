//! Purpose:
//! Enforces the static transfer boundary of `Elephc\Parallel\TaskGroup::spawn()`.
//!
//! Called from:
//! - Instance-method inference after ordinary signature and argument validation.
//!
//! Key details:
//! - The task target must remain statically resolvable, including tracked closure variables.
//! - Captures, arguments, and return values use the shared recursive `PhpType` classifier.
//! - PHP by-reference captures are rejected except for the named Cancellation token.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::optimize::ParallelCallableSafety;
use crate::parser::ast::{CallableTarget, Expr, ExprKind};
use crate::span::Span;
use crate::types::parallel_transfer::{
    is_parallel_cancellation, parallel_transfer_rejection,
    parallel_transfer_return_rejection,
};
use crate::types::{PhpType, TypeEnv};

use super::Checker;

impl Checker {
    /// Rejects statically provable attempts to return the root group beyond `Parallel::run`.
    ///
    /// A `Future` deliberately has a settled post-scope representation, but a `TaskGroup` owns
    /// scope-local cancellation and child bookkeeping. The compiler still emits a lifecycle guard
    /// for dynamic/erased flows; this check keeps ordinary typed paths fail-closed at the API edge.
    pub(super) fn check_parallel_run_task_group_escape(
        &mut self,
        args: &[Expr],
        span: Span,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let Some(body) = args.first() else {
            return Ok(());
        };

        if let ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            variadic_type,
            body: closure_body,
            captures,
            ..
        } = &body.kind
        {
            // `run()` supplies this capability even when a user omitted the parameter hint. The
            // contextual environment therefore catches `fn ($group): mixed => $group` as well
            // as the explicitly typed spelling.
            let closure = self.prepare_closure_signature_context_with_param_hints(
                params,
                variadic,
                *variadic_by_ref,
                variadic_type,
                captures,
                body.span,
                env,
                &[PhpType::Object("Elephc\\Parallel\\TaskGroup".to_string())],
            )?;
            let mut returns = Vec::new();
            for stmt in closure_body {
                self.collect_return_infos(stmt, &closure.env, &mut returns);
            }
            if returns
                .iter()
                .any(|returned| type_contains_parallel_task_group(&returned.ty))
            {
                return Err(parallel_task_group_scope_escape_error(span));
            }
            return Ok(());
        }

        if self
            .resolve_expr_callable_sig(body, env)?
            .is_some_and(|signature| type_contains_parallel_task_group(&signature.return_type))
        {
            return Err(parallel_task_group_scope_escape_error(span));
        }
        Ok(())
    }

    /// Records known Async closure/function tasks so later blocking Parallel calls can be warned on.
    pub(super) fn record_async_task_callable(&mut self, args: &[Expr]) {
        let Some(task) = args.first() else {
            return;
        };
        match &task.kind {
            ExprKind::Closure { .. } => {
                self.async_task_closure_spans.insert(task.span);
            }
            ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
                if let Some(canonical) = self.canonical_function_name_folded(name.as_str()) {
                    self.async_task_functions.insert(canonical);
                }
            }
            _ => {}
        }
    }

    /// Warns when tracked Async task bodies perform a blocking Parallel join or root-scope drain.
    pub(super) fn emit_parallel_async_join_warnings(&mut self) {
        let join_message = "Elephc\\Parallel\\Future::join() inside an Elephc Async task blocks the whole cooperative scheduler thread";
        for (join_site, closure_span) in &self.parallel_future_join_closure_sites {
            if self.async_task_closure_spans.contains(closure_span) {
                self.warnings
                    .push(crate::errors::CompileWarning::new(*join_site, join_message));
            }
        }
        for (join_site, function) in &self.parallel_future_join_function_sites {
            if self.async_task_functions.contains(function) {
                self.warnings
                    .push(crate::errors::CompileWarning::new(*join_site, join_message));
            }
        }

        let run_message = "Elephc\\Parallel\\run() inside an Elephc Async task blocks the whole cooperative scheduler thread until the Parallel scope drains";
        for (run_site, closure_span) in &self.parallel_run_closure_sites {
            if self.async_task_closure_spans.contains(closure_span) {
                self.warnings
                    .push(crate::errors::CompileWarning::new(*run_site, run_message));
            }
        }
        for (run_site, function) in &self.parallel_run_function_sites {
            if self.async_task_functions.contains(function) {
                self.warnings
                    .push(crate::errors::CompileWarning::new(*run_site, run_message));
            }
        }
    }

    /// Deduplicates a Parallel root call and associates it with its enclosing closure or function.
    pub(super) fn record_parallel_run_site(&mut self, span: Span) {
        if self.parallel_run_sites.contains(&span) {
            return;
        }
        self.parallel_run_sites.push(span);
        if let Some(closure_span) = self.current_closure_span {
            self.parallel_run_closure_sites.push((span, closure_span));
        } else if let Some(function) = self.current_function.clone() {
            self.parallel_run_function_sites.push((span, function));
        }
    }

    /// Enforces the v1 worker target, capture, effect, argument and return-transfer restrictions.
    pub(super) fn check_parallel_spawn_transfer(
        &mut self,
        args: &[Expr],
        span: Span,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let Some(task) = args.first() else {
            return Ok(());
        };
        // Named/spread normalization owns cloned AST nodes. Check this exact closure body
        // before consulting address-keyed return-site proofs; the original argument's
        // earlier check cannot establish ownership/alias facts for its normalized copy.
        if matches!(task.kind, ExprKind::Closure { .. }) {
            self.infer_type(task, env)?;
        }
        if matches!(
            task.kind,
            ExprKind::FirstClassCallable(crate::parser::ast::CallableTarget::StaticMethod { .. })
        ) {
            return Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): static-method Closure targets are not supported by the v1 worker ABI",
            ));
        }
        let signature = self.resolve_expr_callable_sig(task, env)?.ok_or_else(|| {
            CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): task Closure target must be statically resolvable",
            )
        })?;
        if signature.by_ref_return {
            return Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): task cannot return by reference across an isolated runtime context",
            ));
        }
        if signature.variadic.is_some() {
            return Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): variadic task Closures are not supported by the v1 worker ABI",
            ));
        }

        let captures = self.parallel_task_captures(task, env)?;
        for (name, ty, by_ref) in captures {
            if by_ref
                && !matches!(&ty, PhpType::Object(class) if is_parallel_cancellation(class))
            {
                return Err(CompileError::new(
                    task.span,
                    &format!(
                        "Elephc\\Parallel\\TaskGroup::spawn(): capture &${name} cannot cross an isolated runtime context; only Elephc\\Async\\Cancellation may be shared by reference"
                    ),
                ));
            }
            if !by_ref && self.is_reference_aliased_array(&name, &ty) {
                return Err(CompileError::new(
                    task.span,
                    &format!(
                        "Elephc\\Parallel\\TaskGroup::spawn(): capture ${name} cannot cross an isolated runtime context: reference-aliased arrays may contain cycles"
                    ),
                ));
            }
            Self::require_parallel_transferable(
                &ty,
                task.span,
                &format!("capture ${name}"),
                true,
            )?;
        }

        let safety = self
            .parallel_callable_safety_for_expr(task)
            .unwrap_or(ParallelCallableSafety {
                uses_process_global_storage: true,
                uses_process_global_runtime_state: false,
                enters_parallel_scope: false,
                enters_async_scope: false,
            });
        if safety.enters_parallel_scope {
            return Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): task may enter Elephc\\Parallel\\run(); nested Parallel scopes are not supported inside a Parallel worker in v1",
            ));
        }
        if safety.enters_async_scope {
            return Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): task may enter Elephc\\Async\\run(); Async root scopes inside Parallel workers are not supported in v1",
            ));
        }
        if safety.uses_process_global_storage {
            return Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): task may access process-global PHP storage (global variables, static locals, or static properties), which is not isolated per worker in v1",
            ));
        }
        if safety.uses_process_global_runtime_state {
            return Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): task may access mutable process-global runtime state or an unclassified external function, which is not isolated per worker in v1",
            ));
        }

        let task_args = &args[1..];
        self.check_known_callable_call(
            &signature,
            task_args,
            span,
            env,
            "Elephc\\Parallel task",
        )?;
        for (index, argument) in task_args.iter().enumerate() {
            self.require_parallel_expr_transferable(
                argument,
                env,
                &format!("argument #{}", index + 1),
                true,
            )?;
        }
        self.require_parallel_task_return_transferable(task, &signature, env)
    }

    /// Resolves worker-safety summaries through tracked aliases, captures and classified externs.
    pub(crate) fn parallel_callable_safety_for_expr(
        &self,
        callable: &Expr,
    ) -> Option<ParallelCallableSafety> {
        match &callable.kind {
            ExprKind::FirstClassCallable(CallableTarget::Function(name))
                if let Some(extern_name) =
                    self.canonical_extern_function_name_folded(name.as_str()) =>
            {
                let has_callback_parameter = self
                    .extern_functions
                    .get(&extern_name)
                    .is_some_and(|signature| {
                        signature
                            .params
                            .iter()
                            .any(|(_, ty)| ty.codegen_repr() == PhpType::Callable)
                    });
                Some(ParallelCallableSafety {
                    uses_process_global_storage: has_callback_parameter,
                    uses_process_global_runtime_state: !has_callback_parameter
                        && !crate::optimize::parallel_extern_is_worker_safe(&extern_name),
                    ..ParallelCallableSafety::default()
                })
            }
            ExprKind::Variable(name) => self.parallel_callable_safety.get(name).copied(),
            ExprKind::ArrayAccess { array, .. } => match &array.kind {
                ExprKind::Variable(name) => self.parallel_callable_safety.get(name).copied(),
                _ => None,
            },
            ExprKind::Assignment { value, .. } => {
                self.parallel_callable_safety_for_expr(value)
            }
            ExprKind::Closure { captures, .. } => {
                let captured_aliases = captures
                    .iter()
                    .filter_map(|name| {
                        self.parallel_callable_safety
                            .get(name)
                            .copied()
                            .map(|safety| (name.clone(), safety))
                    })
                    .collect();
                self.parallel_safety_analysis.as_ref().map(|analysis| {
                    analysis.callable_safety_with_aliases(callable, &captured_aliases)
                })
            }
            _ => self
                .parallel_safety_analysis
                .as_ref()
                .map(|analysis| analysis.callable_safety(callable)),
        }
    }

    /// Collects typed captures from resolvable tasks and rejects implicit object-bound closures.
    fn parallel_task_captures(
        &self,
        task: &Expr,
        env: &TypeEnv,
    ) -> Result<Vec<(String, PhpType, bool)>, CompileError> {
        match &task.kind {
            ExprKind::Closure {
                captures,
                capture_refs,
                is_static,
                ..
            } => {
                if self.current_class.is_some() && !is_static {
                    return Err(CompileError::new(
                        task.span,
                        "Elephc\\Parallel\\TaskGroup::spawn(): a Closure created in object scope must be static because its implicit $this binding cannot cross an isolated runtime context",
                    ));
                }
                Ok(captures
                    .iter()
                    .map(|name| {
                        (
                            name.clone(),
                            env.get(name).cloned().unwrap_or(PhpType::Mixed),
                            capture_refs.iter().any(|captured| captured == name),
                        )
                    })
                    .collect())
            }
            ExprKind::FirstClassCallable(target) => {
                Self::parallel_first_class_captures(target, task)
            }
            ExprKind::Variable(name) if self.callable_captures.contains_key(name) => {
                Ok(self.callable_captures.get(name).cloned().unwrap_or_default())
            }
            ExprKind::Variable(name) => self
                .first_class_callable_targets
                .get(name)
                .ok_or_else(|| {
                    CompileError::new(
                        task.span,
                        "Elephc\\Parallel\\TaskGroup::spawn(): task Closure target must be statically resolvable",
                    )
                })
                .and_then(|target| Self::parallel_first_class_captures(target, task)),
            ExprKind::Assignment { value, .. } => self.parallel_task_captures(value, env),
            _ => Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): task Closure target must be statically resolvable",
            )),
        }
    }

    /// Accepts receiver-free first-class targets and rejects a captured instance receiver.
    fn parallel_first_class_captures(
        target: &CallableTarget,
        task: &Expr,
    ) -> Result<Vec<(String, PhpType, bool)>, CompileError> {
        match target {
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => Ok(Vec::new()),
            CallableTarget::Method { .. } => Err(CompileError::new(
                task.span,
                "Elephc\\Parallel\\TaskGroup::spawn(): receiver-bound Closure captures an object and cannot cross an isolated runtime context",
            )),
        }
    }

    /// Maps the shared recursive transfer classifier's first rejection to a subject-specific error.
    fn require_parallel_transferable(
        ty: &PhpType,
        span: Span,
        subject: &str,
        allow_direct_cancellation: bool,
    ) -> Result<(), CompileError> {
        let rejection = if allow_direct_cancellation {
            parallel_transfer_rejection(ty)
        } else {
            parallel_transfer_return_rejection(ty)
        };
        match rejection {
            Some(reason) => Err(CompileError::new(
                span,
                &format!(
                    "Elephc\\Parallel\\TaskGroup::spawn(): {subject} cannot cross an isolated runtime context: {reason}"
                ),
            )),
            None => Ok(()),
        }
    }

    /// Checks inferred shapes, proving heterogeneous array literals recursively and rejecting aliases.
    fn require_parallel_expr_transferable(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
        subject: &str,
        allow_direct_cancellation: bool,
    ) -> Result<(), CompileError> {
        if let ExprKind::Variable(name) = &expr.kind {
            if env
                .get(name)
                .is_some_and(|ty| self.is_reference_aliased_array(name, ty))
            {
                return Err(CompileError::new(
                    expr.span,
                    &format!(
                        "Elephc\\Parallel\\TaskGroup::spawn(): {subject} cannot cross an isolated runtime context: reference-aliased arrays may contain cycles"
                    ),
                ));
            }
        }
        let ty = self.infer_type(expr, env)?;
        let rejection = if allow_direct_cancellation {
            parallel_transfer_rejection(&ty)
        } else {
            parallel_transfer_return_rejection(&ty)
        };
        match rejection {
            None => Ok(()),
            Some(crate::types::parallel_transfer::ParallelTransferRejection::Mixed) => {
                match &expr.kind {
                    ExprKind::ArrayLiteral(values) => {
                        for value in values {
                            self.require_parallel_expr_transferable(
                                value,
                                env,
                                &format!("{subject} array element"),
                                false,
                            )?;
                        }
                        Ok(())
                    }
                    ExprKind::ArrayLiteralAssoc(entries) => {
                        for (key, value) in entries {
                            self.require_parallel_expr_transferable(
                                key,
                                env,
                                &format!("{subject} array key"),
                                false,
                            )?;
                            self.require_parallel_expr_transferable(
                                value,
                                env,
                                &format!("{subject} array value"),
                                false,
                            )?;
                        }
                        Ok(())
                    }
                    _ => Self::require_parallel_transferable(
                        &ty,
                        expr.span,
                        subject,
                        allow_direct_cancellation,
                    ),
                }
            }
            Some(_) => Self::require_parallel_transferable(
                &ty,
                expr.span,
                subject,
                allow_direct_cancellation,
            ),
        }
    }

    /// Validates declared or individually proven closure returns without allowing Cancellation output.
    fn require_parallel_task_return_transferable(
        &mut self,
        task: &Expr,
        signature: &crate::types::FunctionSig,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        let tracked_closure = matches!(&task.kind,
            ExprKind::Variable(name) if self.callable_captures.contains_key(name));
        if parallel_transfer_return_rejection(&signature.return_type).is_none()
            && !matches!(task.kind, ExprKind::Closure { .. })
            && !tracked_closure
        {
            return Ok(());
        }
        if let ExprKind::Variable(name) = &task.kind {
            if self.parallel_transfer_safe_callable_returns.contains(name) {
                return Ok(());
            }
            if tracked_closure {
                // The cached proof also covers hidden object/array shapes. A failed proof
                // remains the shared unknown-shape diagnostic, not a new alias-only claim.
                return Self::require_parallel_transferable(
                    &PhpType::Mixed,
                    task.span,
                    "task return value",
                    false,
                );
            }
        }
        let ExprKind::Closure { body, .. } = &task.kind else {
            return Self::require_parallel_transferable(
                &signature.return_type,
                task.span,
                "task return value",
                false,
            );
        };
        let mut task_env = env.clone();
        for (name, ty) in &signature.params {
            task_env.insert(name.clone(), ty.clone());
        }
        let mut returns = Vec::new();
        for statement in body {
            self.collect_return_infos(statement, &task_env, &mut returns);
        }
        // A checked scalar/array return contract can narrow an erased expression at the
        // callee's own return boundary. Inspect local alias risk first, but do not re-infer
        // that expression in its parent's storage/callable context afterwards.
        if let Some(returned) = returns.iter().find(|returned| returned.reference_aliased_array) {
            return Err(CompileError::new(
                returned.expr.as_ref().map_or(task.span, |expr| expr.span),
                "Elephc\\Parallel\\TaskGroup::spawn(): task return value cannot cross an isolated runtime context: reference-aliased arrays may contain cycles",
            ));
        }
        if parallel_transfer_return_rejection(&signature.return_type).is_none() {
            return Ok(());
        }
        for returned in returns {
            if let Some(expr) = returned.expr {
                if parallel_transfer_return_rejection(&returned.ty).is_none() {
                    // This type belongs to the checked return site, not the parent
                    // environment. Worker locals must not be re-read in that parent.
                    continue;
                }
                self.require_parallel_expr_transferable(
                    &expr,
                    &task_env,
                    "task return value",
                    false,
                )?;
            }
        }
        Ok(())
    }

    /// Records whether a resolvable callable's result passes the worker-to-parent transfer boundary.
    pub(crate) fn parallel_callable_return_is_safe(
        &mut self,
        callable: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let Some(signature) = self.resolve_expr_callable_sig(callable, env)? else {
            return Ok(false);
        };
        Ok(self
            .require_parallel_task_return_transferable(callable, &signature, env)
            .is_ok())
    }

    /// Identifies tracked reference aliases whose array representation can carry cross-context cycles.
    pub(crate) fn is_reference_aliased_array(&self, name: &str, ty: &PhpType) -> bool {
        self.ref_aliased_locals.contains(name)
            && (ty.is_php_array() || matches!(
                ty.codegen_repr(),
                PhpType::Array(_) | PhpType::AssocArray { .. }
            ))
    }
}

/// Produces the diagnostic for returning a scope-owned TaskGroup beyond its Parallel root.
fn parallel_task_group_scope_escape_error(span: Span) -> CompileError {
    CompileError::new(
        span,
        "Elephc\\Parallel\\run(): TaskGroup cannot escape its owning scope",
    )
}

/// Returns whether `ty` contains the scope-bound Parallel capability, including within a typed
/// array or union. Other object graphs are intentionally handled by the runtime lifecycle guard:
/// their internals are not statically inspectable from the PHP type model.
fn type_contains_parallel_task_group(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class) => {
            php_symbol_key(class.trim_start_matches('\\')) == "elephc\\parallel\\taskgroup"
        }
        PhpType::Array(value) | PhpType::Buffer(value) => type_contains_parallel_task_group(value),
        PhpType::AssocArray { key, value } => {
            type_contains_parallel_task_group(key) || type_contains_parallel_task_group(value)
        }
        PhpType::Union(members) => members.iter().any(type_contains_parallel_task_group),
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Callable
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => false,
    }
}
