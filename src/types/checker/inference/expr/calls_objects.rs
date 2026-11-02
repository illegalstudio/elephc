//! Purpose:
//! Infers callable, object, property, scoped, yield, and assignment expression types.
//!
//! Called from:
//! - "super::Checker::infer_type()" for call and object-oriented expression families.
//!
//! Key details:
//! - Retains constructor, visibility, eval-barrier, callable, and late-static-binding checks.

use super::null_coalesce::{
    null_coalesce_buffer_merge_type, null_coalesce_result_type, operand_is_never_null,
};
use super::{body_must_not_use_this, Checker};
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::pcntl_constants::pcntl_int_constants;
use crate::types::{PhpType, TypeEnv};

impl Checker {
    /// Infers types for calls, closures, objects, property access, assignments, and generators.
    pub(super) fn infer_call_or_object_expr_type(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::FunctionCall { name, args } => {
                let name = name.as_str().to_string();
                let args = args.clone();
                self.record_method_exists_capability_guard(&name, &args);
                if name == crate::names::DYNAMIC_INCLUDE_FUNCTION {
                    if args.len() != 3 {
                        return Err(CompileError::new(
                            expr.span,
                            "Internal dynamic include expects path, once, and required operands",
                        ));
                    }
                    self.infer_type(&args[0], env)?;
                    for flag in &args[1..] {
                        let flag_ty = self.infer_type(flag, env)?;
                        if !matches!(flag_ty, PhpType::Bool | PhpType::False) {
                            return Err(CompileError::new(
                                flag.span,
                                "Internal dynamic include flags must be boolean literals",
                            ));
                        }
                    }
                    return Ok(PhpType::Mixed);
                }
                let call_key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
                let references_pcntl_constant = matches!(call_key.as_str(), "defined" | "constant")
                    && args.first().is_some_and(|arg| {
                        let value = match &arg.kind {
                            ExprKind::StringLiteral(value) => Some(value.as_str()),
                            ExprKind::NamedArg { value, .. } => match &value.kind {
                                ExprKind::StringLiteral(value) => Some(value.as_str()),
                                _ => None,
                            },
                            _ => None,
                        };
                        value.is_some_and(|value| {
                            let value = value.trim_start_matches('\\');
                            pcntl_int_constants(self.target)
                                .iter()
                                .any(|(constant, _)| *constant == value)
                        })
                    });
                if references_pcntl_constant {
                    self.require_builtin_library("elephc_pcntl");
                }
                if self.extern_functions.contains_key(name.as_str()) {
                    return self.check_extern_function_call(name.as_str(), &args, expr.span, env);
                }
                if let Some(ty) = self.check_builtin(name.as_str(), &args, expr.span, env)? {
                    self.builtin_call_types.insert(
                        (self.current_loop_storage_scope.clone(), expr.span),
                        ty.clone(),
                    );
                    return Ok(ty);
                }
                self.check_function_call(name.as_str(), &args, expr.span, env)
            }
            ExprKind::BufferNew { element_type, len } => {
                let len_ty = self.infer_type(len, env)?;
                // A length takes what a buffer index takes. Checked int arithmetic such as
                // `$n + 1` infers `mixed` (PHP promotes an overflowing result to float), so a
                // `mixed` or `int|float` length is converted at runtime, where a value that is
                // not a whole number in range fails the allocation's length validation.
                if !matches!(len_ty, PhpType::Int | PhpType::Mixed)
                    && !len_ty.is_int_float_union()
                {
                    return Err(CompileError::new(
                        expr.span,
                        "buffer_new<T>() length must be integer",
                    ));
                }
                let elem_ty = self.resolve_buffer_element_type(
                    element_type,
                    expr.span,
                    "buffer_new<T>() requires a POD scalar, pointer, or packed class element type",
                )?;
                Ok(PhpType::Buffer(Box::new(elem_ty)))
            }
            ExprKind::BitNot(inner) => {
                let ty = self.infer_type(inner, env)?;
                if let Some(class) = crate::types::object_operators::operator_class(&ty) {
                    return Ok(PhpType::Object(class.name.to_string()));
                }
                // php complements a string byte by byte (brick/math's `BigInteger::fromBytes()`).
                if ty == PhpType::Str {
                    return Ok(PhpType::Str);
                }
                if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
                    return Ok(PhpType::Mixed);
                }
                if !matches!(ty, PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void) {
                    return Err(CompileError::new(
                        expr.span,
                        "Bitwise NOT requires integer operand",
                    ));
                }
                Ok(PhpType::Int)
            }
            ExprKind::NullCoalesce { value, default } => {
                // `??` is a null probe: PHP evaluates `$neverDefined ?? $d` to `$d` without an
                // undefined-variable warning, so a never-declared chain root reads as `null`
                // here. The default operand keeps ordinary inference.
                let probed =
                    crate::types::checker::null_probe::null_probe_env(self, value, env);
                let probed_env = probed.clone();
                let operand_env = probed_env.as_ref().unwrap_or(env);
                let vt = self.infer_null_probe_operand(value, operand_env)?;
                let value_is_never_null = operand_is_never_null(self, value, &vt, operand_env);
                let dt = self.infer_type(default, env)?;
                if let Some(buffer) = null_coalesce_buffer_merge_type(
                    self,
                    value,
                    &vt,
                    value_is_never_null,
                    default,
                    &dt,
                    expr.span,
                )? {
                    return Ok(buffer);
                }
                Ok(null_coalesce_result_type(self, value_is_never_null, vt, dt))
            }
            ExprKind::Pipe { value, callable } => {
                self.infer_pipe_type(value, callable, expr, env)
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                let mut scoped_env = env.clone();
                self.check_assignment_expression(
                    target,
                    value,
                    result_target.as_deref(),
                    prelude,
                    expr.span,
                    &mut scoped_env,
                )
            }
            ExprKind::ConstRef(name) => {
                let constant_name = name.as_str().trim_start_matches('\\');
                if constant_name == "MT_RAND_PHP" {
                    self.warnings.push(crate::errors::CompileWarning::deprecation(
                        expr.span,
                        "Constant MT_RAND_PHP is deprecated since 8.3, as it uses a biased non-standard variant of Mt19937",
                    ));
                }
                if pcntl_int_constants(self.target)
                    .iter()
                    .any(|(constant, _)| *constant == constant_name)
                {
                    self.require_builtin_library("elephc_pcntl");
                }
                // php's fallback for an unqualified name in a namespace is the global constant.
                if let Some(ty) = self.constants.get(name.as_str()).or_else(|| {
                    name.function_fallback()
                        .and_then(|global| self.constants.get(global))
                }) {
                    return Ok(ty.clone());
                }
                // A name nothing in the closed world defines is not a compile error: php looks it
                // up when the expression executes. A file the program loads at run time may define
                // it (a Composer `files` polyfill handed to the runtime defines `IDNA_DEFAULT`
                // behind `if (!defined(...))`), and otherwise the read throws `Error: Undefined
                // constant`. Lowering picks the runtime fetch or the throwing lookup, by the
                // global form it looks the name up under.
                if !self.eval_barrier_active {
                    let looked_up = name
                        .function_fallback()
                        .map(|global| global.trim_start_matches('\\'))
                        .unwrap_or(constant_name);
                    self.record_undefined_constant_read(looked_up, expr.span);
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::FirstClassCallable(target) => {
                self.infer_first_class_callable_target(target, expr.span, env)?;
                Ok(PhpType::Callable)
            }
            ExprKind::Closure {
                params,
                variadic,
                variadic_by_ref,
                variadic_type,
                return_type,
                body,
                is_arrow,
                is_static,
                captures,
                capture_refs,
                by_ref_return: _,
            } => {
                if *is_static {
                    body_must_not_use_this(body, expr.span)?;
                }
                let captures = &crate::types::checker::callables::closure_captures_for_check(
                    *is_arrow, captures, capture_refs, env,
                );
                self.infer_closure_type(
                    params,
                    variadic,
                    *variadic_by_ref,
                    variadic_type,
                    return_type,
                    body,
                    captures,
                    capture_refs,
                    expr,
                    env,
                )
            }
            ExprKind::Spread(inner) => {
                self.infer_spread_element_type(inner, expr.span, env, false)
            }
            ExprKind::NamedArg { value, .. } => self.infer_type(value, env),
            ExprKind::ClosureCall { var, args } => {
                self.infer_closure_call_type(var, args, expr, env)
            }
            ExprKind::ExprCall { callee, args } => {
                self.infer_expr_call_type(callee, args, expr, env)
            }
            ExprKind::BinaryOp { left, op, right } => {
                self.infer_binary_op_type(left, op, right, expr, env)
            }
            ExprKind::InstanceOf { value, target } => {
                self.infer_instanceof_type(value, target, expr, env)
            }
            ExprKind::NewObject { class_name, args } => {
                self.infer_new_object_type(class_name.as_str(), args, expr, env)
            }
            // `generics::classes` rewrites every generic construction to a `NewObject` naming
            // the instantiated class, so one arriving here means no template answered it — the
            // named class declares no type parameters. Reported rather than widened: a
            // template has no representation, and inferring `Plain` for `new Plain<int>()`
            // would compile a program whose type arguments meant nothing.
            ExprKind::NewGeneric { class_type, args } => {
                if let Some(instantiated) =
                    self.infer_written_generic_construction(class_type, args, expr, env)?
                {
                    return Ok(instantiated);
                }
                Err(CompileError::new(
                expr.span,
                &format!(
                    "'{}' is written with type arguments but declares no type parameters",
                    match class_type {
                        crate::parser::ast::TypeExpr::GenericClass { name, .. } =>
                            name.as_str().to_string(),
                        other => crate::generics::describe_type(other),
                    }
                ),
                ))
            }
            ExprKind::Clone(inner) => {
                // A non-public `__clone` is php's RUNTIME Error ("Call to private method
                // C::__clone() from global scope"), catchable like any other; the backend's clone
                // hook dispatch raises it. Refusing the program here made that catch unwritable.
                let ty = self.infer_type(inner, env)?;
                match &ty {
                    PhpType::Object(_) => Ok(ty),
                    // A Closure value is a callable descriptor here; `clone $closure` is a
                    // fresh Closure that keeps `$this`, scope and captures (lowered as a
                    // null-receiver rebind).
                    PhpType::Callable => Ok(PhpType::Callable),
                    PhpType::Union(_)
                        if crate::types::checker::type_compat::type_is_gradual_object_family(
                            &ty,
                        ) =>
                    {
                        Ok(ty)
                    }
                    PhpType::Mixed => Ok(PhpType::Mixed),
                    other if matches!(other.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) => {
                        Ok(PhpType::Mixed)
                    }
                    _ => Err(CompileError::new(expr.span, "clone requires an object value")),
                }
            }
            ExprKind::NewDynamic { name_expr, args } => {
                // The class is named at runtime; without a literal class
                // we can't typecheck constructor args or the resulting
                // object's type. Infer the name expression for its side
                // effects + warnings, type-check the args generically, and
                // return Mixed.
                // The unpack-after-named shape is syntactic in
                // PHP, so it is still rejected without a known constructor.
                self.require_no_spread_after_named_args(args, "Dynamic constructor")?;
                // The runtime class is unknown, so an array-element argument binds by value, as php
                // does for every constructor that does not take that position by reference.
                // Refusing the whole file because SOME unrelated class of the closed world has a
                // by-reference constructor parameter (Symfony's RedisTrait `new $sentinelClass(
                // $host, $port, $params['timeout'])` meets `SessionBagProxy::__construct`) deferred
                // it to the runtime autoloader. An element handed to a constructor that really
                // takes a reference is the only shape that loses its alias.
                // No constructor signature means no parameter binding modes either, so every
                // argument is conservatively reference-aliased — see
                // `Checker::record_unresolved_callee_argument_aliases`.
                self.record_unresolved_callee_argument_aliases(args);
                self.infer_type(name_expr, env)?;
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::NewDynamicObject {
                class_name,
                fallback_class,
                args,
                ..
            } => {
                let class_ty = self.infer_type(class_name, env)?;
                if class_ty != PhpType::Str {
                    return Err(CompileError::new(
                        class_name.span,
                        "Dynamic object factory class must be a string",
                    ));
                }
                self.infer_new_object_type(fallback_class.as_str(), args, expr, env)?;
                self.require_phar_archive_libraries();
                Ok(PhpType::Object(fallback_class.as_str().to_string()))
            }
            ExprKind::PropertyAccess { object, property } => {
                self.infer_property_access_type(object, property, expr, env)
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                self.infer_dynamic_property_access_type(object, property, expr, env, false)
            }
            ExprKind::NullsafePropertyAccess { object, property } => {
                self.infer_nullsafe_property_access_type(object, property, expr, env)
            }
            ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.infer_dynamic_property_access_type(object, property, expr, env, true)
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                self.infer_static_property_access_type(receiver, property, expr, env)
            }
            ExprKind::DynamicStaticPropertyAccess { receiver, property } => {
                self.infer_dynamic_static_property_access_type(receiver, property, expr, env)
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => self.infer_method_call_type(object, method, args, expr, env),
            ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => self.infer_nullsafe_method_call_type(object, method, args, expr, env),
            ExprKind::NullsafeDynamicMethodCall {
                object,
                method,
                args,
            } => self.infer_nullsafe_dynamic_method_call_type(object, method, args, expr, env),
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => self.infer_static_method_call_type(receiver, method, args, expr, env),
            ExprKind::This => self.infer_this_type(expr, env),
            ExprKind::PtrCast {
                target_type,
                expr: inner,
            } => self.infer_ptr_cast_type(target_type, inner, expr, env),
            ExprKind::ClassConstant { receiver } => {
                self.validate_class_constant_receiver(receiver, expr.span)?;
                Ok(PhpType::Str)
            }
            ExprKind::ObjectClassName { object } => {
                let object_type = self.infer_type(object, env)?;
                // A closure is an object of class `Closure`: `$closure::class` is "Closure".
                let object_only = match &object_type {
                    PhpType::Object(_) | PhpType::Mixed | PhpType::Callable => true,
                    PhpType::Union(members) => {
                        !members.is_empty()
                            && members.iter().any(|member| {
                                matches!(
                                    member,
                                    PhpType::Object(_) | PhpType::Mixed | PhpType::Callable
                                )
                            })
                    }
                    _ => false,
                };
                if !object_only {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Cannot use \"::class\" on {}", object_type),
                    ));
                }
                Ok(PhpType::Str)
            }
            ExprKind::ScopedConstantAccess { receiver, name } => {
                self.infer_scoped_constant_access(receiver, name, expr)
            }
            ExprKind::DynamicScopedConstantAccess { receiver, name } => {
                self.infer_dynamic_scoped_constant_access(receiver, name, expr, env)
            }
            ExprKind::NewScopedObject { receiver, args } => {
                // A receiver written `Box<int>::of()` names `Box` until instantiation renames
                // it, and this pass can run on a generic function's template body — which is
                // walked and then stripped, never instantiated.
                let receiver = &receiver.written_class_receiver();
                let class_name = match receiver {
                    // A generic receiver is instantiated into an ordinary named one before type checking;
                    // a template has no class to reach through.
                    crate::parser::ast::StaticReceiver::Generic(_) => unreachable!(
                        "StaticReceiver::Generic must be instantiated by generics::classes"
                    ),
                    crate::parser::ast::StaticReceiver::Self_ => {
                        self.current_class.clone().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use 'new self()' outside a class context",
                            )
                        })?
                    }
                    crate::parser::ast::StaticReceiver::Static => {
                        let class_name = self.current_class.clone().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use 'new static()' outside a class context",
                            )
                        })?;
                        self.validate_late_bound_constructor_targets(&class_name, args, expr, env)?;
                        return Ok(PhpType::Object(class_name));
                    }
                    crate::parser::ast::StaticReceiver::Parent => {
                        let current = self.current_class.as_ref().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use 'new parent()' outside a class context",
                            )
                        })?;
                        self.classes
                            .get(current)
                            .and_then(|info| info.parent.clone())
                            .ok_or_else(|| {
                                CompileError::new(
                                    expr.span,
                                    &format!("Class '{}' has no parent class", current),
                                )
                            })?
                    }
                    crate::parser::ast::StaticReceiver::Named(name) => name.as_canonical(),
                };
                self.infer_new_object_type(&class_name, args, expr, env)
            }
            ExprKind::Yield { key, value } => {
                if let Some(k) = key {
                    self.infer_type(k, env)?;
                }
                if let Some(v) = value {
                    self.infer_type(v, env)?;
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::YieldFrom(inner) => {
                let inner_ty = self.infer_type(inner, env)?;
                // Arrays, mixed-key literals and non-Generator Traversables are re-yielded through
                // the ordinary iterator protocol. A concrete Generator keeps its dedicated
                // delegation path so sent values and its return value are forwarded exactly as
                // PHP requires.
                let supported = matches!(inner.kind, ExprKind::ArrayLiteralMixed(_))
                    || self.yield_from_source_supported(&inner_ty);
                if !supported {
                    return Err(CompileError::new(
                        inner.span,
                        &format!(
                            "yield from expects an array or Traversable, got {}",
                            inner_ty
                        ),
                    ));
                }
                Ok(PhpType::Mixed)
            }
            ExprKind::MagicConstant(_) => {
                unreachable!("MagicConstant must be lowered before type inference")
            }
            _ => unreachable!("basic expression routed to call/object inference"),
        }
    }

    /// Records a read of a constant nothing defines (`name` without a leading `\`).
    ///
    /// php compiles the read and throws `Error: Undefined constant "NAME"` when it runs, which a
    /// `defined('NAME') && NAME` guard never reaches; lowering emits that runtime lookup. The
    /// warning (once per name) keeps a misspelt constant visible at build time.
    pub(crate) fn record_undefined_constant_read(&mut self, name: &str, span: crate::span::Span) {
        if self.undefined_constant_reads.insert(name.to_string()) {
            self.warnings.push(crate::errors::CompileWarning::new(
                span,
                &format!("Undefined constant: {} (reading it throws Error at runtime)", name),
            ));
        }
    }

    /// Returns whether a static or gradual value can carry a valid `yield from` source.
    fn yield_from_source_supported(&self, ty: &PhpType) -> bool {
        match ty {
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable | PhpType::Mixed => {
                true
            }
            PhpType::Union(members) => members
                .iter()
                .any(|member| self.yield_from_source_supported(member)),
            _ => {
                self.type_accepts(&PhpType::Object("Generator".to_string()), ty)
                    || self.type_accepts(&PhpType::Object("Traversable".to_string()), ty)
            }
        }
    }

    /// Infers one argument on a callable-descriptor path.
    ///
    /// Descriptor invokers own a runtime iterator walk, so they can accept Traversable spread
    /// sources. Generic user calls deliberately use ordinary `infer_type` and remain array-only
    /// until their lowering gains the same walk.
    pub(crate) fn infer_descriptor_call_arg_type(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_runtime_iterated_call_arg_type(expr, env)
    }

    /// Infers one argument for a call surface that owns runtime array or Traversable iteration.
    ///
    /// Besides descriptor invokers, `array_push` uses this for its associative receiver
    /// coordinator, which stages every unpacked value before mutating the receiver.
    pub(crate) fn infer_runtime_iterated_call_arg_type(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        self.infer_runtime_iterated_call_arg_types(expr, env)
            .map(|(_, element)| element)
    }

    /// Infers both the source and element types for a runtime-iterated call argument.
    ///
    /// The source type lets builtin checks distinguish a concrete typed array from a
    /// Traversable or boxed array whose element layout remains unknown to native storage.
    pub(crate) fn infer_runtime_iterated_call_arg_types(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<(PhpType, PhpType), CompileError> {
        match &expr.kind {
            ExprKind::Spread(inner) => {
                let source = self.infer_type(inner, env)?;
                let element = self.spread_source_element_type(
                    source.clone(),
                    expr.span,
                    true,
                )?;
                Ok((source, element))
            }
            _ => self.infer_type(expr, env).map(|ty| (ty.clone(), ty)),
        }
    }

    /// Returns the element type supplied by a spread source under the selected lowering policy.
    fn infer_spread_element_type(
        &mut self,
        inner: &Expr,
        span: crate::span::Span,
        env: &TypeEnv,
        allow_traversable: bool,
    ) -> Result<PhpType, CompileError> {
        let ty = self.infer_type(inner, env)?;
        self.spread_source_element_type(ty, span, allow_traversable)
    }

    /// Returns the element type a spread source of the already inferred type `ty` supplies, or
    /// the "requires an array" diagnostic at `span`, the span of the `...` node itself.
    ///
    /// Split from [`Checker::infer_spread_element_type`] so the indexed-literal typing, which
    /// types its spread sources once in its hash-spread scan, does not type them a second time.
    pub(super) fn spread_source_element_type(
        &self,
        ty: PhpType,
        span: crate::span::Span,
        allow_traversable: bool,
    ) -> Result<PhpType, CompileError> {
        match ty {
            PhpType::Array(elem_ty) => Ok(*elem_ty),
            PhpType::AssocArray { value, .. } => Ok(*value),
            PhpType::Mixed => Ok(PhpType::Mixed),
            ty if ty.is_php_array() => Ok(PhpType::Mixed),
            // Gradual and iterable sources are unpacked through the runtime iterator protocol.
            PhpType::Iterable | PhpType::Union(_) => Ok(PhpType::Mixed),
            // `callable` is a PREDICATE, not a storage shape: the value behind it may be the
            // two-element `[$object, 'method']` array PHP unpacks. EIR lowering rebuilds both
            // elements from the descriptor, so the element type is gradual.
            PhpType::Callable => Ok(PhpType::Mixed),
            PhpType::Object(name) if self.object_type_implements_iterable(&name) => {
                Ok(PhpType::Mixed)
            }
            ty if allow_traversable && self.descriptor_spread_source_may_be_traversable(&ty) => {
                Ok(PhpType::Mixed)
            }
            _ => Err(CompileError::new(span, "Spread operator requires an array")),
        }
    }

    /// Returns whether descriptor runtime iteration can validate this non-array source type.
    ///
    /// Every object is a possible `Traversable` at runtime. Keeping acceptance tied to the
    /// descriptor path is important: its iterator setup owns the runtime interface check, while
    /// ordinary calls still only know how to unpack array storage. Known concrete classes and
    /// interface hints therefore do not need a second, brittle nominal proof here.
    pub(crate) fn descriptor_spread_source_may_be_traversable(&self, ty: &PhpType) -> bool {
        match ty {
            PhpType::Iterable => true,
            PhpType::Object(_) => true,
            PhpType::Union(members) => members
                .iter()
                .all(|member| match member {
                    PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed => true,
                    other => self.descriptor_spread_source_may_be_traversable(other),
                }),
            _ => false,
        }
    }

    /// Returns whether a descriptor call contains a runtime Traversable spread source.
    pub(crate) fn descriptor_call_has_traversable_spread(
        &mut self,
        args: &[Expr],
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        for arg in args {
            let ExprKind::Spread(inner) = &arg.kind else {
                continue;
            };
            let ty = self.infer_type(inner, env)?;
            if self.descriptor_spread_source_may_be_traversable(&ty) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
