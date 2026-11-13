//! Purpose:
//! Infers expression class refs forms for the checker.
//! Handles type facts and diagnostics for expression shapes that need more than scalar/operator inference.
//!
//! Called from:
//! - `crate::types::checker::inference::expr`
//!
//! Key details:
//! - Expression inference shares environments with statement checking, so variable and effect updates must stay synchronized.

use crate::errors::{CompileError, RuntimeFault};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    /// Validates `new` expressions on late-bound static constructor targets by inferring
    /// the object type for every concrete class that descends from `base_class`.
    ///
    /// Used when `$obj::new(...)` or similar late-bound constructor syntax is used,
    /// to ensure each possible runtime-instantiable class variant is well-typed. Abstract bases
    /// and descendants cannot be the late-bound runtime class of a successful construction.
    pub(super) fn validate_late_bound_constructor_targets(
        &mut self,
        base_class: &str,
        args: &[Expr],
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<(), CompileError> {
        // Every registered class at or below `base_class`, from the children index rather than a
        // parent-chain walk from every class in the table (`new static` sits in hot factories).
        let mut class_names: Vec<String> = self.class_and_descendants(base_class);
        class_names.sort();

        for class_name in class_names {
            // Abstract receivers fail at runtime. Their concrete descendants still need
            // constructor validation, but an abstract candidate is never instantiated here.
            if self.classes.get(&class_name).is_some_and(|class| class.is_abstract)
                // A descendant that is not a construction candidate for these arguments is a
                // separate call site php decides at run time; see `late_bound_target_cannot_bind`.
                || (class_name != base_class
                    && self.late_bound_target_cannot_bind(&class_name, args, env)?)
            {
                continue;
            }
            let constructor = self
                .classes
                .get(&class_name)
                .and_then(|class| class.methods.get("__construct"))
                .cloned();
            if let Some(constructor) = constructor {
                for (arg_index, arg) in args.iter().enumerate() {
                    let (param_index, value) = match &arg.kind {
                        ExprKind::NamedArg { name, value } => (
                            constructor
                                .params
                                .iter()
                                .position(|(param, _)| param == name)
                                .unwrap_or(arg_index),
                            value.as_ref(),
                        ),
                        _ => (arg_index, arg),
                    };
                    if constructor
                        .ref_params
                        .get(param_index)
                        .copied()
                        .unwrap_or(false)
                        && matches!(value.kind, ExprKind::ArrayAccess { .. })
                    {
                        return Err(CompileError::new(
                            value.span,
                            "new static() cannot bind an array element by reference because the runtime constructor target is late-bound",
                        ));
                    }
                }
            }
            self.infer_new_object_type(&class_name, args, expr, env)?;
        }

        Ok(())
    }

    /// Returns whether this late-bound descendant is not a construction candidate for these
    /// arguments, and so has nothing to say about whether the program is well typed.
    ///
    /// `new static` binds to the class the call was made ON, so a descendant is a SEPARATE call
    /// site that PHP decides at run time. Two shapes matter, and Symfony's
    /// `HttpException::fromStatusCode()` — which ends in
    /// `new static($statusCode, $message, $previous, $headers, $code)` — hits both:
    ///
    /// - SURPLUS: `AccessDeniedHttpException` declares four parameters. PHP discards the surplus
    ///   and runs, so this is not an error at all; the lowering reproduces it by dropping the
    ///   extra arguments (`static_new_droppable_surplus_args`).
    /// - STORAGE CONFLICT: `MethodNotAllowedHttpException` takes `array $allow` where the factory
    ///   passes an int. PHP raises a TypeError, but only for a program that actually calls
    ///   `MethodNotAllowedHttpException::fromStatusCode()`; the lowering drops the class from its
    ///   candidate set (`static_new_args_match_param_storage`) and the branch raises if reached.
    ///
    /// - TOO FEW: Laravel's `Collection::newInstance()` is `new static($items)`, and
    ///   `SortedMiddleware extends Collection` declares `__construct(array $priorityMap,
    ///   $middlewares)`. PHP raises `ArgumentCountError` only if a `SortedMiddleware` receiver
    ///   reaches that method, which Laravel never does; reporting it at compile time refused the
    ///   whole `Collection` class, and the deferred class then failed to declare at run time, so
    ///   every request died. The lowering already leaves such a class out of its candidate set
    ///   (`dynamic_new_args_lower_to_exact_arity`).
    fn late_bound_target_cannot_bind(
        &mut self,
        class_name: &str,
        args: &[Expr],
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let Some(sig) = self
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.methods.get("__construct"))
            .cloned()
        else {
            return Ok(false);
        };
        // Arguments before the first spread or named one bind by position whatever follows, so
        // their storage still rules a descendant out; only the arity checks need the full list.
        let positional = args
            .iter()
            .position(|arg| {
                matches!(
                    arg.kind,
                    crate::parser::ast::ExprKind::Spread(_)
                        | crate::parser::ast::ExprKind::NamedArg { .. }
                )
            })
            .unwrap_or(args.len());
        if positional == args.len() {
            if !crate::func_args::sig_collects_surplus_args(&sig) && args.len() > sig.params.len() {
                return Ok(true);
            }
            if args.len() < crate::types::checker::schema::validation::required_param_count(&sig) {
                return Ok(true);
            }
        }
        for (index, arg) in args[..positional].iter().enumerate() {
            let Some((_, param_ty)) = sig.params.get(index) else {
                break;
            };
            if crate::types::late_static_dispatch::literal_string_cannot_bind_numeric(param_ty, arg) {
                return Ok(true);
            }
            let arg_ty = self.infer_type(arg, env)?;
            if late_bound_param_storage_conflicts(
                &param_ty.late_bound_param_storage(),
                &arg_ty.codegen_repr(),
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Infers the type of a class constant or enum case accessed via scope resolution
    /// (e.g., `MyClass::CONSTANT` or `Color::Red`).
    ///
    /// Searches the class/interface hierarchy for the named constant, preferring enum cases
    /// when the receiver is an enum. Falls back to interface constants and finally returns
    /// an error if the constant is not found.
    pub(crate) fn infer_scoped_constant_access(
        &mut self,
        receiver: &StaticReceiver,
        name: &str,
        expr: &Expr,
    ) -> Result<PhpType, CompileError> {
        let class_name = self.resolve_static_receiver_class(receiver, expr.span)?;
        if !self.scoped_constant_receiver_is_known(&class_name) {
            // PHP resolves classes lazily when this expression executes. Keep an unknown
            // receiver gradual so optional integrations in unexecuted paths remain compilable;
            // the EIR backend emits the runtime class-not-found fatal for an executed read.
            return Ok(PhpType::Mixed);
        }
        // First: enum case access (`Color::Red`). Enums shadow classes for
        // this syntax in PHP since 8.1. A name that is not a declared case is an enum *constant*
        // (`Scale::FACTOR`), which is resolved through the class-constant table below.
        if let Some(enum_info) = self.enums.get(&class_name) {
            if enum_info.cases.iter().any(|case| case.name == name) {
                return self.infer_enum_case_type(&class_name, name, expr);
            }
        }
        // Walk parent chain to find a class constant.
        let mut current_class = Some(class_name.clone());
        while let Some(cn) = current_class.as_deref() {
            if let Some(info) = self.classes.get(cn) {
                if let Some(type_expr) = info.constant_types.get(name).cloned() {
                    return self.resolve_type_expr(&type_expr, expr.span);
                }
                if let Some(value_expr) = info.constants.get(name).cloned() {
                    if let Some(reason) = info.constant_deprecations.get(name).cloned() {
                        let message = if reason.is_empty() {
                            format!("Use of deprecated class constant: {}::{}", cn, name)
                        } else {
                            format!(
                                "Use of deprecated class constant: {}::{} — {}",
                                cn, name, reason
                            )
                        };
                        self.warnings
                            .push(crate::errors::CompileWarning::new(expr.span, &message));
                    }
                    return self.infer_type(&value_expr, &TypeEnv::default());
                }
            }
            current_class = self.classes.get(cn).and_then(|i| i.parent.clone());
        }
        // Fallback: search implemented interfaces (and parent interfaces).
        // Only the interface list is needed: cloning the whole class record for it was a top
        // checker frame on Symfony (every interface-constant read through a class name).
        let interfaces = self.classes.get(&class_name).map(|info| info.interfaces.clone());
        if let Some(interfaces) = interfaces {
            for iface_name in &interfaces {
                if let Some((value, type_expr)) = self.lookup_interface_constant(iface_name, name) {
                    if let Some(type_expr) = type_expr {
                        return self.resolve_type_expr(&type_expr, expr.span);
                    }
                    return self.infer_type(&value, &TypeEnv::default());
                }
            }
        }
        // Direct interface receiver (`Limits::MAX`).
        if let Some((value, type_expr)) = self.lookup_interface_constant(&class_name, name) {
            if let Some(type_expr) = type_expr {
                return self.resolve_type_expr(&type_expr, expr.span);
            }
            return self.infer_type(&value, &TypeEnv::default());
        }
        // On an enum, a `::name` that is neither a declared case nor a constant is an undefined
        // case — report that rather than the generic class-constant message.
        if self.enums.contains_key(&class_name) {
            return Err(CompileError::new(
                expr.span,
                &format!("Undefined enum case: {}::{}", class_name, name),
            ));
        }
        // php resolves the constant when the read runs: `Error: Undefined constant X::C`.
        Err(CompileError::new(
            expr.span,
            &format!("Undefined class constant: {}::{}", class_name, name),
        )
        .with_runtime_fault(RuntimeFault::UndefinedClassConstant {
            class: class_name.clone(),
            constant: name.to_string(),
        }))
    }

    /// Infers a class constant read whose receiver is an object-valued expression.
    pub(crate) fn infer_dynamic_scoped_constant_access(
        &mut self,
        receiver: &Expr,
        name: &str,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let receiver_type = self.infer_type(receiver, env)?;
        match receiver_type.codegen_repr() {
            PhpType::Object(class_name) => self.infer_scoped_constant_access(
                &StaticReceiver::Named(crate::names::Name::from(class_name)),
                name,
                expr,
            ),
            // PHP permits `$className::CONSTANT` with a runtime class-string. The concrete
            // class and constant are selected by the closed-world registry during lowering;
            // until then its value must remain gradual.
            PhpType::Str | PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
            other => Err(CompileError::new(
                receiver.span,
                &format!(
                    "Dynamic class constant receiver must be an object or class-string, got {}",
                    other
                ),
            )),
        }
    }

    /// Returns whether a scoped-constant receiver is known in static class-like metadata.
    fn scoped_constant_receiver_is_known(&self, class_name: &str) -> bool {
        self.classes.contains_key(class_name)
            || self.interfaces.contains_key(class_name)
            || self.declared_traits.contains(class_name)
            || self.enums.contains_key(class_name)
    }

    /// Looks up a constant by name on an interface, traversing parent interfaces breadth-first
    /// to find it. Returns its value expression and optional declared type.
    fn lookup_interface_constant(
        &self,
        interface_name: &str,
        const_name: &str,
    ) -> Option<(crate::parser::ast::Expr, Option<crate::parser::ast::TypeExpr>)> {
        let mut visited = crate::fast_hash::FastSet::default();
        let mut queue: Vec<String> = vec![interface_name.to_string()];
        while let Some(name) = queue.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            if let Some(iface) = self.interfaces.get(&name) {
                if let Some(value) = iface.constants.get(const_name) {
                    return Some((
                        value.clone(),
                        iface.constant_types.get(const_name).cloned(),
                    ));
                }
                queue.extend(iface.parents.iter().cloned());
            }
        }
        None
    }

    /// Resolves a `StaticReceiver` to its canonical class name string.
    ///
    /// - `Named` returns the class name directly.
    /// - `Self_` / `Static` return the current class, or error if not inside a class.
    /// - `Parent` returns the parent of the current class, or error if there is no parent.
    fn resolve_static_receiver_class(
        &self,
        receiver: &StaticReceiver,
        span: Span,
    ) -> Result<String, CompileError> {
        // A receiver written `Box<int>::of()` names `Box` until instantiation renames
        // it, and this pass can run on a generic function's template body — which is
        // walked and then stripped, never instantiated.
        let receiver = &receiver.written_class_receiver();
        match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            StaticReceiver::Named(name) => Ok(name.as_canonical()),
            StaticReceiver::Self_ | StaticReceiver::Static => self
                .current_class
                .clone()
                .ok_or_else(|| CompileError::new(span, "Cannot use self:: outside a class context")),
            StaticReceiver::Parent => {
                let current = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(span, "Cannot use parent:: outside a class context")
                })?;
                self.classes
                    .get(current)
                    .and_then(|info| info.parent.clone())
                    .ok_or_else(|| {
                        CompileError::new(
                            span,
                            &format!("Class '{}' has no parent class", current),
                        )
                    })
            }
        }
    }

    /// Validates that `self::class`, `static::class`, or `parent::class` is used in an
    /// appropriate class context. Returns an error for invalid scope (e.g., outside a class
    /// or on a class with no parent for `parent::class`).
    pub(super) fn validate_class_constant_receiver(
        &self,
        receiver: &StaticReceiver,
        span: Span,
    ) -> Result<(), CompileError> {
        // A receiver written `Box<int>::of()` names `Box` until instantiation renames
        // it, and this pass can run on a generic function's template body — which is
        // walked and then stripped, never instantiated.
        let receiver = &receiver.written_class_receiver();
        match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            StaticReceiver::Named(_) => Ok(()),
            StaticReceiver::Self_ | StaticReceiver::Static => {
                if self.current_class.is_some() {
                    Ok(())
                } else {
                    Err(CompileError::new(
                        span,
                        "Cannot use self::class or static::class outside a class context",
                    ))
                }
            }
            StaticReceiver::Parent => {
                let current = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(
                        span,
                        "Cannot use parent::class outside a class context",
                    )
                })?;
                if self
                    .classes
                    .get(current)
                    .and_then(|info| info.parent.as_ref())
                    .is_some()
                {
                    Ok(())
                } else {
                    Err(CompileError::new(
                        span,
                        &format!("Class '{}' has no parent class", current),
                    ))
                }
            }
        }
    }
}

/// Returns whether two concrete representations can never hold one another's values.
///
/// Mirrors `crate::ir_lower::expr::static_new_param_storage_conflicts`, so the checker and the
/// lowering agree on which late-bound descendants are construction candidates.
fn late_bound_param_storage_conflicts(param: &PhpType, arg: &PhpType) -> bool {
    let array_like = |ty: &PhpType| matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. });
    let scalar_like = |ty: &PhpType| {
        matches!(
            ty,
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
        )
    };
    let object_like = |ty: &PhpType| matches!(ty, PhpType::Object(_));
    (array_like(param) && (scalar_like(arg) || object_like(arg)))
        || ((scalar_like(param) || object_like(param)) && array_like(arg))
}
