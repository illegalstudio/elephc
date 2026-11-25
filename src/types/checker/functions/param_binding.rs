//! Purpose:
//! Applies PHP's parameter-binding rules when a declared user-defined parameter is handed an
//! argument whose inferred type does not already satisfy it: coercive scalar binding
//! (`string $s` accepting `42`) and callable-name strings (`callable $f` accepting
//! `"strtoupper"`).
//!
//! Called from:
//! - `crate::types::checker::functions::resolution` (user function and method calls)
//!
//! Key details:
//! - The accept/reject decision lives in `crate::types::param_binding`, shared with the
//!   matching EIR argument rewrite. This file only turns that decision into a diagnostic and
//!   registers the callable signature a bound callable string implies.
//! - Coercive binding runs *after* `types_compatible` / `type_accepts` have already failed, so
//!   it can only widen what is accepted, never narrow it.
//! - The `declare(strict_types=1)` rejection runs *before* them instead, because the widenings
//!   PHP drops in strict mode (`bool`→`int`, `int`→`bool`, …) are ones `types_compatible`
//!   already accepts on its own.
//! - A tracked callable-array local or statically resolved two-element literal may bind to
//!   `callable` only when lowering can materialize its target as a real descriptor at the same
//!   parameter boundary.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, TypeExpr};
use crate::span::Span;
use crate::types::param_binding::{
    classify_param_binding, strict_param_binding_rejection, ParamBinding,
};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Validates one declared parameter against an argument, allowing PHP's coercive
    /// parameter binding and callable-name strings before reporting a type mismatch.
    ///
    /// `owner` names the `(function, parameter)` pair used to register the signature of a
    /// callable-name string, so the callee can type-check invocations of that parameter the
    /// same way it does for a first-class callable. Pass `None` where that registration does
    /// not apply (variadic elements, spread-expanded positions).
    ///
    /// `by_ref` must be true for a pass-by-reference parameter. Those arguments retain a shared
    /// caller cell; boxed object references defer value validation to the entry guard.
    ///
    /// When the call site's file declared `strict_types=1`, the strict rejection runs first and
    /// no coercive binding is considered at all.
    ///
    /// # Errors
    /// Returns the standard `<context> expects <expected>, got <actual>` mismatch, extended
    /// with the PHP behaviour elephc cannot reproduce when a binding rule exists but is not
    /// statically decidable.
    pub(crate) fn require_bound_param_arg_type(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        env: &TypeEnv,
        context: &str,
        owner: Option<(&str, &str)>,
        by_ref: bool,
        position: usize,
    ) -> Result<(), CompileError> {
        self.require_strict_types_param_binding(expected, actual, arg.span, context)?;
        // Null-only promoted parameters are checked before constructor ABI materialization.
        // Keep by-reference storage outside this value-binding exception.
        if !by_ref && *expected == PhpType::Void && actual.codegen_repr() == PhpType::Mixed {
            return Ok(());
        }

        if !by_ref && *expected == PhpType::DeclaredCallable {
            return self.require_declared_callable_arg(expected, actual, arg, env, context, owner);
        }
        if !by_ref && expected.declares_callable() {
            // `?callable`, `callable|false` and the other callable unions keep a literal name as
            // the string too, so the runtime dispatch must know it like an exact `callable`.
            self.record_declared_callable_literal_name(arg, env);
        }

        // A property reference cannot be coerced into the parameter's type the way a plain
        // value can, so PHP checks the property's CURRENT type at the call boundary and raises
        // its argument `TypeError` before the callee body runs. Record the guard whenever the
        // declared storage is not exactly the parameter representation, including a union that
        // may hold a compatible member (`int|string` for `int &$x`) and a boxed `mixed` slot.
        let mut property_place = arg;
        while let crate::parser::ast::ExprKind::NamedArg { value, .. }
        | crate::parser::ast::ExprKind::ErrorSuppress(value) = &property_place.kind
        {
            property_place = value;
        }
        if by_ref
            && matches!(
                property_place.kind,
                crate::parser::ast::ExprKind::PropertyAccess { .. }
            )
            && !matches!(expected, PhpType::Mixed)
            && crate::types::param_binding::by_ref_property_writeback_supported(expected)
            && crate::types::param_binding::by_ref_property_writeback_supported(actual)
            && (matches!(actual, PhpType::Union(_) | PhpType::Mixed)
                || !Self::types_compatible(expected, actual)
                || !self.type_accepts(expected, actual))
        {
            let display = context
                .split(" parameter $")
                .next()
                .unwrap_or(context)
                .trim();
            let display = display
                .strip_prefix("Function '")
                .or_else(|| display.strip_prefix("Method '"))
                .or_else(|| display.strip_prefix("Static method "))
                .unwrap_or(display)
                .trim_end_matches('\'');
            let display = if display.starts_with("Closure") {
                "{closure}"
            } else {
                display
            };
            let param_name = context.rsplit(" parameter $").next().unwrap_or("arg");
            // The current value decides the `given` type when the storage can hold a compatible
            // member; a concrete disjoint type always throws and reports its declared type.
            let coerces_weakly = self.by_ref_property_slot_is_untyped(property_place, env)?;
            // An untyped slot's current value decides the weak acceptance, even when its
            // flow-inferred type is one concrete member.
            let conditional = coerces_weakly
                || matches!(actual, PhpType::Union(_) | PhpType::Mixed)
                || actual.codegen_repr() == PhpType::Mixed
                // A declared object may hold a subclass (or the parameter may name a
                // subclass of the property's type): PHP checks the CURRENT class at the
                // call boundary instead of refusing statically.
                || matches!((expected, actual), (PhpType::Object(_), PhpType::Object(_)));
            let given = crate::types::diagnostic_names::php_type_name(actual);
            let prefix = format!(
                "{display}(): Argument #{position} (${param_name}) must be of type {}, ",
                crate::types::diagnostic_names::php_type_name(expected),
            );
            let scope = self.current_loop_storage_scope.clone();
            crate::types::checker::record_throw_access_site(
                &mut self.throw_access_sites,
                &scope,
                property_place.span,
                crate::types::ThrowAccessInfo {
                    span: property_place.span,
                    kind: crate::types::ThrowAccessKind::ReferenceArgumentType {
                        prefix,
                        expected: expected.clone(),
                        conditional,
                        coerces_weakly,
                        given,
                    },
                },
            );
            return Ok(());
        }

        if Self::types_compatible(expected, actual) || self.type_accepts(expected, actual) {
            return Ok(());
        }
        if by_ref && matches!(expected, PhpType::Object(_))
            && actual.codegen_repr() == PhpType::Mixed
            && matches!(arg.kind, crate::parser::ast::ExprKind::Variable(_))
        {
            return Ok(());
        }
        if by_ref {
            // php binds `array &$p` to a `mixed` variable and checks the value at run time.
            // The declared PHP-array union lowers to the same boxed `Mixed` cell the variable
            // already holds, so the reference needs no storage change; only the compile-time
            // contract stood in the way, and it refused a `mixed` return value handed straight
            // to such a parameter.
            //
            // A nullable or union parameter (`?array &$read`, `int|string &$v`) lowers to the
            // same boxed cell too, so a `mixed` variable binds to it on the same terms.
            if (expected.is_php_array() || expected.codegen_repr() == PhpType::Mixed)
                && matches!(actual.codegen_repr(), PhpType::Mixed)
                && matches!(arg.kind, crate::parser::ast::ExprKind::Variable(_))
            {
                return Ok(());
            }
            return self.require_compatible_arg_type(expected, actual, arg.span, context);
        }
        match classify_param_binding(expected, actual, arg) {
            ParamBinding::Identity | ParamBinding::Cast(_) | ParamBinding::Const(_) => Ok(()),
            ParamBinding::Callable(target) => {
                let sig = self
                    .resolve_first_class_callable_sig(&target, arg.span, env)
                    .map_err(|err| {
                        Self::param_binding_error(
                            expected,
                            actual,
                            arg,
                            context,
                            err.message.as_str(),
                        )
                    })?;
                self.register_bound_callable_param_sig(owner, sig);
                Ok(())
            }
            ParamBinding::ObjectGuard => Ok(()),
            ParamBinding::Deprecated(detail)
            | ParamBinding::TypeError(detail)
            | ParamBinding::NeedsRuntimeCheck(detail) => Err(Self::param_binding_error(
                expected, actual, arg, context, &detail,
            )),
            ParamBinding::Rejected => {
                // A callable array keeps ordinary array storage until this exact boundary.
                // Accept only a tracked local or a statically resolvable two-element literal,
                // resolve its signature without invoking it, and let EIR materialize the
                // corresponding descriptor.
                if *expected == PhpType::Callable {
                    let Some(target) = self.callable_array_param_target(arg, env)? else {
                        return self.require_compatible_arg_type(
                            expected, actual, arg.span, context,
                        );
                    };
                    let sig = self
                        .resolve_first_class_callable_sig(&target, arg.span, env)
                        .map_err(|err| {
                            Self::param_binding_error(
                                expected,
                                actual,
                                arg,
                                context,
                                err.message.as_str(),
                            )
                        })?;
                    self.register_bound_callable_param_sig(owner, sig);
                    return Ok(());
                }
                self.require_compatible_arg_type(expected, actual, arg.span, context)
            }
        }
    }

    /// Returns whether a by-reference property argument addresses an UNTYPED declaration.
    ///
    /// PHP coerces an untyped property's current value to a declared scalar by-reference
    /// parameter in weak mode; a declared property, including `mixed`, is checked against its
    /// current value without coercion.
    fn by_ref_property_slot_is_untyped(
        &mut self,
        arg: &Expr,
        env: &TypeEnv,
    ) -> Result<bool, CompileError> {
        let crate::parser::ast::ExprKind::PropertyAccess { object, property } = &arg.kind else {
            return Ok(false);
        };
        let Some((class_name, addressable)) =
            self.ref_property_slot_addressability(object, property, env)?
        else {
            return Ok(false);
        };
        Ok(addressable
            && self
                .classes
                .get(class_name.trim_start_matches('\\'))
                .is_some_and(|info| !info.visible_property_is_declared(property)))
    }

    /// Records an argument whose binding EIR lowering checks at run time, and whether a callee
    /// exception summary can contain it (it sits in a function, method, closure or class body).
    pub(crate) fn record_scalar_guard_arg_site(&mut self, span: Span) {
        self.scalar_guard_arg_sites.insert(span);
        if self.current_function.is_some()
            || self.current_method.is_some()
            || self.current_class.is_some()
            || self.closure_depth > 0
        {
            self.summary_scalar_guard_arg_sites.insert(span);
        }
    }

    /// Records a spread whose elements bind to a declared scalar parameter at or after
    /// `param_idx`: they are checked at run time from their values, so the call stays
    /// observable.
    pub(crate) fn record_spread_scalar_guard_arg_site(
        &mut self,
        sig: &crate::types::FunctionSig,
        param_idx: usize,
        regular_param_count: usize,
        span: Span,
    ) {
        if (param_idx..regular_param_count).any(|index| {
            crate::types::param_binding::runtime_scalar_guard_param(sig, index).is_some()
        }) {
            self.record_scalar_guard_arg_site(span);
        }
    }

    /// Rejects a declared-parameter argument that `declare(strict_types=1)` forbids at this
    /// call site.
    ///
    /// No-op unless the statement being checked came from a file that declared the directive,
    /// and no-op for every conversion PHP still performs in strict mode (the `int`→`float`
    /// widening and every non-scalar declared type). It is called from the coercive binding
    /// path *and* from the call surfaces that never had coercive binding — declared variadic
    /// element types and the sig-based closure/first-class-callable path — so the directive
    /// reaches every declared parameter, not only the coercively bound ones.
    ///
    /// # Errors
    /// Returns the standard `<context> expects <expected>, got <actual>` mismatch extended with
    /// the `TypeError` PHP would throw at run time.
    pub(crate) fn require_strict_types_param_binding(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        span: Span,
        context: &str,
    ) -> Result<(), CompileError> {
        // Every declared parameter binding passes through here, in both modes. EIR lowering
        // checks a `mixed`/union argument bound to a declared scalar at run time; record the
        // argument so the AST optimizer neither drops the call as pure nor prunes a
        // `catch (TypeError)` around it. Nullable and by-reference parameters are filtered
        // by lowering; recording them too only keeps a call observable.
        if matches!(expected, PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool)
            && crate::types::param_binding::runtime_scalar_guard_needed(expected, actual, self.strict_types)
        {
            self.record_scalar_guard_arg_site(span);
        }
        if !self.strict_types {
            return Ok(());
        }
        match strict_param_binding_rejection(expected, actual) {
            Some(detail) => Err(Self::binding_mismatch_error(
                expected, actual, span, context, &detail,
            )),
            None => Ok(()),
        }
    }

    /// Applies `declare(strict_types=1)` to the elements of fully static positional unpacks.
    ///
    /// The planned-argument loops keep `...[0]` as one spread node and skip it, so without this
    /// pass a literal unpack bypassed the strict check that the same value passed directly gets.
    /// Only literal indexed arrays are flattened (`expand_planned_positional_spreads`); dynamic
    /// unpacks keep their runtime binding. Each element is checked against the parameter it
    /// binds, or against the declared variadic element type past the regular parameters.
    ///
    /// # Errors
    /// Returns the strict binding mismatch for the first rejected element.
    pub(crate) fn require_strict_types_static_spread_bindings(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<(), CompileError> {
        if !self.strict_types || !args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
            return Ok(());
        }
        let Some(expanded) = crate::types::call_args::expand_planned_positional_spreads(args) else {
            return Ok(());
        };
        let regular_param_count = crate::types::call_args::regular_param_count(sig);
        let variadic_index = crate::types::signatures::variadic_param_index(sig);
        let variadic_elem_ty = self.variadic_argument_element_type(sig, span, callee_desc)?;
        for (position, arg) in expanded.iter().enumerate() {
            let (index, expected, context) = if position < regular_param_count {
                let (name, ty) = &sig.params[position];
                (position, ty.clone(), format!("{} parameter ${}", callee_desc, name))
            } else if let (Some(index), Some(elem_ty)) = (variadic_index, variadic_elem_ty.as_ref()) {
                let name = &sig.params[index].0;
                (index, elem_ty.clone(), format!("{} variadic parameter ${}", callee_desc, name))
            } else {
                break;
            };
            if !sig.declared_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let actual = self.infer_type(arg, caller_env)?;
            self.require_strict_types_param_binding(&expected, &actual, arg.span, &context)?;
            // The scalar rule above only judges scalar-to-scalar pairs. A scalar, `false` or
            // `null` element unpacked into a declared type with no compatible member (`int` or
            // `false` into `array|string|null`, `null` into `string`) is a TypeError too.
            // Callable slots keep their own string binding rules and are left to the planned
            // binding.
            let scalar_actual = matches!(
                actual,
                PhpType::Int
                    | PhpType::Float
                    | PhpType::Bool
                    | PhpType::False
                    | PhpType::Str
                    | PhpType::Void
            );
            if scalar_actual && !Self::type_has_callable_member(&expected) {
                self.require_compatible_arg_type(&expected, &actual, arg.span, &context)?;
            }
        }
        Ok(())
    }

    /// Returns whether `ty` is `callable` or a union with a `callable` member.
    fn type_has_callable_member(ty: &PhpType) -> bool {
        match ty {
            PhpType::Union(members) => members.iter().any(Self::type_has_callable_member),
            other => other.codegen_repr() == PhpType::Callable,
        }
    }

    /// Returns the strict-call validation signature for prelude functions whose PHP-visible
    /// parameters are deliberately widened to `mixed`.
    ///
    /// The injected Tidy prelude declares `$config` and `$encoding` as `mixed`, and the locale
    /// prelude declares `setlocale()`'s `$locales` and `...$rest` as `mixed`, so their runtime
    /// adapters can reproduce the internal functions' weak scalar coercion and TypeError
    /// ordering. Under `declare(strict_types=1)` PHP's internal parameter parser still rejects
    /// scalars against `array|string|null` and `?string`; restore those contract types for the
    /// checker's strict validation only, leaving weak-mode runtime behavior untouched.
    pub(crate) fn prelude_strict_adapter_signature(
        &self,
        function: Option<&str>,
        owner: Option<&str>,
        callee_desc: Option<&str>,
        sig: &FunctionSig,
    ) -> Option<FunctionSig> {
        if !self.strict_types {
            return None;
        }
        let function_key = function.map(|name| name.trim_start_matches('\\').to_ascii_lowercase());
        let tidy_function = function_key.as_deref().is_some_and(|name| {
            matches!(
                name,
                "tidy_parse_string" | "tidy_parse_file" | "tidy_repair_string" | "tidy_repair_file"
            )
        });
        // A program's own global `setlocale()` replaces the prelude and keeps its declared types.
        let setlocale_function = function_key.as_deref() == Some("setlocale")
            && function
                .and_then(|name| self.fn_decls.get(name))
                .is_some_and(|decl| decl.internal_prelude);
        let tidy_method = owner
            .zip(callee_desc)
            .is_some_and(|(owner, callee_desc)| self.tidy_prelude_declares_method(owner, callee_desc));
        if !tidy_function && !tidy_method && !setlocale_function {
            return None;
        }
        let mut adjusted = sig.clone();
        let mut changed = false;
        for (name, ty) in &mut adjusted.params {
            if *ty != PhpType::Mixed {
                continue;
            }
            let array_string_or_null = if setlocale_function {
                name.eq_ignore_ascii_case("locales")
            } else {
                name.eq_ignore_ascii_case("config")
            };
            if array_string_or_null {
                *ty = self.normalize_union_type(vec![
                    PhpType::php_array(),
                    PhpType::Str,
                    PhpType::Void,
                ]);
                changed = true;
            } else if !setlocale_function && name.eq_ignore_ascii_case("encoding") {
                *ty = self.normalize_union_type(vec![PhpType::Str, PhpType::Void]);
                changed = true;
            }
        }
        // Call validation reads a declared variadic's element type from its source type
        // expression, so `...$rest` is narrowed there rather than in its collector storage type.
        if setlocale_function {
            if let Some(slot) = crate::types::signatures::variadic_param_index(&adjusted)
                .and_then(|index| adjusted.param_type_exprs.get_mut(index))
                .filter(|slot| slot.as_ref().is_some_and(Self::is_mixed_type_expr))
            {
                *slot = Some(TypeExpr::Union(vec![
                    TypeExpr::Named(Name::unqualified("array")),
                    TypeExpr::Str,
                    TypeExpr::Named(Name::unqualified("null")),
                ]));
                changed = true;
            }
        }
        changed.then_some(adjusted)
    }

    /// Returns the strict-call validation signature for a first-class callable whose target
    /// is a prelude function with widened `mixed` parameters, or `None` for any other target.
    pub(crate) fn first_class_prelude_strict_adapter_signature(
        &self,
        target: &CallableTarget,
        sig: &FunctionSig,
    ) -> Option<FunctionSig> {
        let CallableTarget::Function(name) = target else {
            return None;
        };
        let function = self
            .canonical_function_name_folded(name.as_str())
            .unwrap_or_else(|| name.as_str().to_string());
        self.prelude_strict_adapter_signature(Some(&function), None, None, sig)
    }

    /// Returns whether `type_expr` is the bare `mixed` a prelude adapter declares.
    fn is_mixed_type_expr(type_expr: &TypeExpr) -> bool {
        matches!(type_expr, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("mixed"))
    }

    /// Returns whether `callee_desc` names a method declared by the injected `tidy` class.
    ///
    /// A subclass receiver may inherit the prelude adapter or override/add its own methods.
    /// The declaring-class table distinguishes the two: only methods whose implementation
    /// class is `tidy` get the contract-type strict check.
    fn tidy_prelude_declares_method(&self, owner: &str, callee_desc: &str) -> bool {
        let Some((_, method)) = callee_desc.rsplit_once("::") else {
            return false;
        };
        let method = method.trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && character != '_'
        });
        if method.is_empty() {
            return false;
        }
        let Some(class_info) = self.classes.get(owner.trim_start_matches('\\')) else {
            return false;
        };
        let key = crate::names::php_symbol_key(method);
        let declares = |declaring: Option<&String>| {
            declaring.is_some_and(|declaring| declaring.trim_start_matches('\\').eq_ignore_ascii_case("tidy"))
        };
        declares(class_info.method_declaring_classes.get(&key))
            || declares(class_info.static_method_declaring_classes.get(&key))
    }

    /// Runs `operation` with `declare(strict_types=1)` parameter binding suspended.
    ///
    /// PHP invokes a callback handed to an internal function (`array_map`, `usort`,
    /// `array_walk`, `preg_replace_callback`, …) from the engine's own frame, which is never
    /// strict, so the calling file's directive does not reach the callback's parameters —
    /// `array_map('g', [true])` still coerces `true` into `g(int $i)` under `strict_types=1`.
    /// `call_user_func`/`call_user_func_array` are the documented exception: they forward the
    /// caller's frame and therefore stay on the strict path. Verified on PHP 8.4.20.
    pub(crate) fn with_internal_callback_binding<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let outer_strict_types = self.strict_types;
        let outer_internal_callback_binding = self.internal_callback_binding;
        self.strict_types = false;
        self.internal_callback_binding = true;
        let result = operation(self);
        self.strict_types = outer_strict_types;
        self.internal_callback_binding = outer_internal_callback_binding;
        result
    }

    /// Builds the extended parameter mismatch diagnostic, appending the PHP behaviour that
    /// explains why elephc refuses the binding.
    fn param_binding_error(
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        context: &str,
        detail: &str,
    ) -> CompileError {
        Self::binding_mismatch_error(expected, actual, arg.span, context, detail)
    }

    /// Formats one parameter-binding rejection at `span`, shared by the coercive and strict
    /// paths so both diagnostics read identically apart from the explanation.
    fn binding_mismatch_error(
        expected: &PhpType,
        actual: &PhpType,
        span: Span,
        context: &str,
        detail: &str,
    ) -> CompileError {
        CompileError::new(
            span,
            &format!(
                "{} expects {}, got {} — {}",
                context, expected, actual, detail
            ),
        )
    }

    /// Records the signature a bound callable-name string gives a declared `callable`
    /// parameter, so the callee resolves `$f(...)` exactly as it does for a first-class
    /// callable argument.
    /// Validates one argument bound to a declared `callable` parameter.
    ///
    /// The value is stored as PHP passed it, so no argument is rewritten. A literal function or
    /// `Class::method` name and a statically resolvable callable array are still resolved
    /// here: an unknown name is a compile error, and a known target registers its signature
    /// so invocations of the parameter keep their static result type. Every other string,
    /// array, object or boxed value is left to PHP's runtime callable check.
    pub(crate) fn require_declared_callable_arg(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        env: &TypeEnv,
        context: &str,
        owner: Option<(&str, &str)>,
    ) -> Result<(), CompileError> {
        // A spread element bound through a projection (`["ucfirst", ...][0]`) is not a bare
        // literal; record what it may carry, since the projected type can be `mixed`.
        self.record_static_callable_literals(arg);
        if let Some(arms) = selected_value_arms(static_callable_argument(self, arg, env)) {
            // Each arm of `?:`, `??` or `match` is the bound value itself: a literal arm is
            // resolved like a direct literal (an unknown name is a compile error and its
            // libraries are linked). No signature is registered, since the arms may differ.
            for arm in arms {
                // An arm can itself be an assignment or `@` (`$c ? ($n = "ucfirst") : ...`).
                let value = static_callable_argument(self, arm, env);
                if matches!(value.kind, crate::parser::ast::ExprKind::StringLiteral(_))
                    || selected_value_arms(value).is_some()
                {
                    let arm_ty = if matches!(value.kind, crate::parser::ast::ExprKind::StringLiteral(_)) {
                        PhpType::Str
                    } else {
                        actual.clone()
                    };
                    self.require_declared_callable_arg(expected, &arm_ty, arm, env, context, None)?;
                }
            }
        }
        let target = match (actual.codegen_repr(), &static_callable_argument(self, arg, env).kind) {
            (PhpType::Str, crate::parser::ast::ExprKind::StringLiteral(name)) => {
                match crate::types::param_binding::callable_target_from_name(name) {
                    Some(target) => Some(target),
                    None => {
                        return Err(Self::param_binding_error(
                            expected,
                            actual,
                            arg,
                            context,
                            &format!("\"{}\" is not a valid callable name", name.escape_debug()),
                        ))
                    }
                }
            }
            (PhpType::Array(_) | PhpType::AssocArray { .. }, _) => {
                self.callable_array_param_target(arg, env)?
            }
            _ => None,
        };
        if let Some(target) = target {
            let sig = self
                .resolve_first_class_callable_sig(&target, arg.span, env)
                .map_err(|err| {
                    Self::param_binding_error(expected, actual, arg, context, err.message.as_str())
                })?;
            self.register_bound_callable_param_sig(owner, sig);
            if let crate::parser::ast::CallableTarget::Function(name) = &target {
                self.declared_callable_names.insert(name.as_str().to_string());
            }
            return Ok(());
        }
        if Self::types_compatible(expected, actual) || self.type_accepts(expected, actual) {
            return Ok(());
        }
        self.require_compatible_arg_type(expected, actual, arg.span, context)
    }

    /// Records a function name written as a string literal where a declared `callable` keeps
    /// the string: a `callable` return value, or an argument for a callable union parameter.
    ///
    /// The value stays the plain string, so invoking it later resolves the name at run time;
    /// the runtime dispatch only knows builtin names written as literals where a declared
    /// callable keeps the string. An unknown name is left to the runtime `TypeError`, as PHP
    /// raises it.
    pub(crate) fn record_declared_callable_literal_name(&mut self, value: &Expr, env: &TypeEnv) {
        let value = static_callable_argument(self, value, env);
        if let Some(arms) = selected_value_arms(value) {
            // Each arm of a conditional is the value itself, so it is a certain callable.
            for arm in arms {
                self.record_declared_callable_literal_name(arm, env);
            }
            return;
        }
        let crate::parser::ast::ExprKind::StringLiteral(name) = &value.kind else {
            self.record_static_callable_literals(value);
            return;
        };
        if let Some(crate::parser::ast::CallableTarget::Function(function)) =
            crate::types::param_binding::callable_target_from_name(name)
        {
            // The wrapper the dispatch builds for a builtin needs its libraries linked, as
            // `name(...)` links them.
            let key = crate::names::php_symbol_key(function.as_str().trim_start_matches('\\'));
            self.require_first_class_callable_builtin_libraries(&key);
            self.declared_callable_names.insert(function.as_str().to_string());
        }
    }

    /// Validates one argument collected by a declared `callable ...$fs` variadic (or a callable
    /// union element such as `?callable ...$fs`), or one literal element of a static spread, as
    /// a regular parameter of that type binds it.
    ///
    /// An exact `callable` element resolves a literal name or callable array like a regular
    /// `callable` parameter (an unknown literal name is a compile error); a callable union
    /// element records a literal name for the runtime dispatch. A by-reference element binds a
    /// variable, never a literal, so it is left to the entry check.
    pub(crate) fn require_declared_callable_variadic_arg(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        env: &TypeEnv,
        context: &str,
        by_ref: bool,
    ) -> Result<(), CompileError> {
        if by_ref {
            return Ok(());
        }
        if *expected == PhpType::DeclaredCallable {
            return self.require_declared_callable_arg(expected, actual, arg, env, context, None);
        }
        if expected.declares_callable() {
            self.record_declared_callable_literal_name(arg, env);
        }
        Ok(())
    }

    /// Lists the still open regular parameters a spread fills, from `start`, as (declared type,
    /// by-reference). An undeclared parameter reports `Mixed`, which binds no callable literal.
    pub(crate) fn declared_spread_targets(
        sig: &FunctionSig,
        start: usize,
        regular_param_count: usize,
    ) -> Vec<(PhpType, bool)> {
        (start..regular_param_count)
            .map(|index| {
                let declared = sig.declared_params.get(index).copied().unwrap_or(false);
                let ty = sig
                    .params
                    .get(index)
                    .filter(|_| declared)
                    .map_or(PhpType::Mixed, |(_, ty)| ty.clone());
                (ty, sig.ref_params.get(index).copied().unwrap_or(false))
            })
            .collect()
    }

    /// Returns whether any parameter of a call target (`regular`, then the variadic element)
    /// declares `callable`, alone or in a union.
    pub(crate) fn call_declares_callable(regular: &[(PhpType, bool)], variadic: Option<&PhpType>) -> bool {
        regular.iter().any(|(ty, _)| ty.declares_callable())
            || variadic.is_some_and(PhpType::declares_callable)
    }

    /// Records every string literal held by a static array literal behind a spread, a named
    /// argument or an access into that literal (the shape call planning gives a spread element
    /// bound to a regular parameter).
    ///
    /// This over-approximates on purpose: a later runtime key, another spread or a named argument
    /// decides which element binds where, so these names are only possible callables. They are
    /// recorded through [`Self::record_possible_callable_literal_name`], which never adds a
    /// library or prelude requirement. A bare literal argument is resolved by
    /// [`Self::require_declared_callable_arg`] instead.
    pub(crate) fn record_static_callable_literals(&mut self, expr: &Expr) {
        use crate::parser::ast::ExprKind;
        match &expr.kind {
            ExprKind::Spread(inner) => self.record_static_array_literal_elements(inner),
            ExprKind::NamedArg { value, .. } => self.record_static_callable_literals(value),
            ExprKind::ArrayAccess { array, .. } => self.record_static_array_literal_elements(array),
            // Planning binds an optional parameter filled by a spread as
            // `count($spread) > 0 ? $spread[0] : default`.
            ExprKind::Ternary { then_expr, else_expr, .. } => {
                self.record_static_callable_literal_element(then_expr);
                self.record_static_callable_literal_element(else_expr);
            }
            ExprKind::ShortTernary { value, default } => {
                self.record_static_callable_literal_element(value);
                self.record_static_callable_literal_element(default);
            }
            _ => {}
        }
    }

    /// Records the string literals of one static array literal (indexed, keyed or mixed). A
    /// bare array argument is a callable array, not a list of names, so only the spread and
    /// projection forms above reach this.
    fn record_static_array_literal_elements(&mut self, array: &Expr) {
        use crate::parser::ast::{ArrayEntry, ExprKind};
        match &array.kind {
            ExprKind::ArrayLiteral(items) => {
                for item in items {
                    self.record_static_callable_literal_element(item);
                }
            }
            ExprKind::ArrayLiteralAssoc(entries) => {
                for (_, value) in entries {
                    self.record_static_callable_literal_element(value);
                }
            }
            ExprKind::ArrayLiteralMixed(entries) => {
                for entry in entries {
                    match entry {
                        ArrayEntry::Spread(value)
                        | ArrayEntry::Keyed(_, value)
                        | ArrayEntry::Value(value)
                        | ArrayEntry::Reference(value)
                        | ArrayEntry::KeyReference(_, value) => {
                            self.record_static_callable_literal_element(value);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Records one element of a static array literal: a string literal names a callable, and a
    /// nested spread contributes its own elements.
    fn record_static_callable_literal_element(&mut self, element: &Expr) {
        match &element.kind {
            crate::parser::ast::ExprKind::StringLiteral(_) => {
                self.record_possible_callable_literal_name(element);
            }
            crate::parser::ast::ExprKind::Spread(inner) => {
                self.record_static_array_literal_elements(inner);
            }
            _ => self.record_static_callable_literals(element),
        }
    }

    /// Records a string literal that may reach a declared callable, when linking its wrapper
    /// costs nothing beyond the wrapper itself.
    ///
    /// A spread or projection element may just as well be ordinary text for another parameter,
    /// so a builtin that needs a bridge or system library, or whose wrapper calls a prelude
    /// helper (`array_rand`, XML, GMP), is not recorded: its requirement would change what the program links (or fail to build the
    /// wrapper) for a name that may never be called. Such a builtin bound only this way stops
    /// with the runtime-only builtin error, like a name selected at run time.
    fn record_possible_callable_literal_name(&mut self, value: &Expr) {
        let crate::parser::ast::ExprKind::StringLiteral(name) = &value.kind else {
            return;
        };
        let Some(crate::parser::ast::CallableTarget::Function(function)) =
            crate::types::param_binding::callable_target_from_name(name)
        else {
            return;
        };
        let key = crate::names::php_symbol_key(function.as_str().trim_start_matches('\\'));
        if crate::builtins::registry::lookup(&key).is_some_and(|def| {
            !crate::builtins::registry::builtin_requirements_are_empty(def)
                || !crate::builtins::registry::callable_wrapper_prelude_helpers(&key).is_empty()
        }) {
            return;
        }
        self.declared_callable_names.insert(function.as_str().to_string());
    }

    fn register_bound_callable_param_sig(
        &mut self,
        owner: Option<(&str, &str)>,
        sig: FunctionSig,
    ) {
        let Some((owner_name, param_name)) = owner else {
            return;
        };
        let key = (owner_name.to_string(), param_name.to_string());
        if self.callable_param_sigs.get(&key) != Some(&sig) {
            self.callable_param_sigs.insert(key, sig);
        }
    }
}

/// Returns the value an argument binds: through a named argument, an assignment whose result
/// is the assigned value (`apply($name = "strlen")`, `apply($box->cb = "strlen")`) or the `@`
/// operator, which all evaluate to their inner value.
///
/// Call planning wraps a keyed variadic entry as a named argument, so a literal callable name
/// written there sits behind that node too. A string-offset write evaluates to one byte, never
/// to the name it writes, so an element write is looked through only when its base is proven
/// not to be a string (see `assignment_yields_assigned_value`).
fn static_callable_argument<'e>(checker: &Checker, arg: &'e Expr, env: &TypeEnv) -> &'e Expr {
    use crate::parser::ast::ExprKind;
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => static_callable_argument(checker, value, env),
        ExprKind::Assignment { target, value, result_target, .. }
            if assignment_yields_assigned_value(checker, target, result_target.is_some(), env) =>
        {
            static_callable_argument(checker, value, env)
        }
        ExprKind::ErrorSuppress(inner) => static_callable_argument(checker, inner, env),
        _ => arg,
    }
}

/// Returns whether an assignment to `target` evaluates to the value it assigns, so a literal on
/// its right-hand side is the value an enclosing argument or return binds.
///
/// A plain local and a property write do (a compound write stores the combined value, which is
/// no literal). An element write does only when its base is a local typed as an array or an
/// object, or a property declared with an array type: on a string (or on a `mixed`, union,
/// callable or untyped base that may hold one) it is a string-offset write, which evaluates to
/// the one byte it stored.
fn assignment_yields_assigned_value(
    checker: &Checker,
    target: &Expr,
    has_result_target: bool,
    env: &TypeEnv,
) -> bool {
    use crate::parser::ast::ExprKind;
    let holds_array_or_object = |ty: &PhpType| {
        ty.is_php_array() || matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_))
    };
    match &target.kind {
        ExprKind::Variable(_) => !has_result_target,
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => match &array.kind {
            ExprKind::Variable(name) => env.get(name).is_some_and(holds_array_or_object),
            ExprKind::PropertyAccess { object, property } => {
                let ExprKind::Variable(owner) = &object.kind else {
                    return false;
                };
                let Some(PhpType::Object(class_name)) = env.get(owner) else {
                    return false;
                };
                checker.classes.get(class_name).is_some_and(|info| {
                    info.visible_property_is_declared(property)
                        && info.visible_property(property).is_some_and(|(_, (_, ty))| {
                            ty.is_php_array() || matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. })
                        })
                })
            }
            _ => false,
        },
        _ => false,
    }
}

/// Returns the expressions a conditional can evaluate to (`?:`, `??`, `match`), or `None` for
/// any other expression. Each of them is the selected value itself, not data beside it.
pub(crate) fn selected_value_arms(expr: &Expr) -> Option<Vec<&Expr>> {
    use crate::parser::ast::ExprKind;
    match &expr.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => Some(vec![then_expr, else_expr]),
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            Some(vec![value, default])
        }
        ExprKind::Match { arms, default, .. } => {
            Some(arms.iter().map(|(_, result)| result).chain(default.as_deref()).collect())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    /// Type checks one source fixture through the real frontend and returns the builtin names
    /// it records as declared callables.
    fn recorded_names(source: &str) -> std::collections::BTreeSet<String> {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("fixture must parse");
        let program = crate::name_resolver::resolve(program).expect("fixture must resolve");
        let program = crate::func_args::desugar(program).expect("fixture must desugar");
        let program = crate::optimize::fold_constants(program);
        crate::types::check(&program).expect("fixture must type check").declared_callable_names
    }

    /// Verifies a literal behind an assignment is recorded when the assignment evaluates to it (a
    /// local, a property, an element of an array or object local, of an `array` parameter or of
    /// a property declared `array`) and not for a string-offset write, which evaluates to one
    /// byte: recording that name would link a wrapper (and its libraries) the program never
    /// calls.
    #[test]
    fn records_literals_behind_assignments_that_yield_them() {
        let names = recorded_names(
            r#"<?php
class Box { public $cb; }
function apply(callable $f): string { return (string) $f("abc"); }
function pick(): callable|string { $s = "x"; return ($s[0] = "ucfirst"); }
function named(): callable|string { return ($n = "lcfirst"); }
function held(): callable|string { $a = []; return ($a[0] = "ucwords"); }
$s = "x";
$a = [""];
$box = new Box();
echo apply($n = "strrev"), apply($s[0] = "bin2hex"), pick(), named(), held();
echo apply($a[0] = "nl2br"), apply($box->cb = "strtolower");
class Bag { public array $items = []; }
function pass(array $items): string { return apply($items[0] = "strtoupper"); }
$bag = new Bag();
echo pass([""]), apply($bag->items[0] = "lcfirst");
echo apply($argc > 0 ? ($m = "addslashes") : "trim"), apply($argc > 0 ? ($s[0] = "sha1") : "trim");
"#,
        );
        for name in ["strrev", "lcfirst", "ucwords", "nl2br", "strtolower", "strtoupper", "addslashes"] {
            assert!(names.contains(name), "missing {name} in {names:?}");
        }
        for name in ["ucfirst", "bin2hex", "sha1"] {
            assert!(!names.contains(name), "unexpected {name} in {names:?}");
        }
    }
}
