//! Purpose:
//! Applies PHP's parameter-binding rules when a declared user-defined parameter is handed an
//! argument whose inferred type does not already satisfy it: coercive scalar binding
//! (`string $s` accepting `42`), weak `Stringable` object conversion, and callable-name
//! strings (`callable $f` accepting `"strtoupper"`).
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
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, Visibility};
use crate::span::Span;
use crate::types::param_binding::{
    classify_param_binding, gradual_object_requires_runtime_nominal_guard,
    gradual_union_requires_runtime_param_guard,
    nullable_int_requires_runtime_param_guard,
    object_requires_runtime_nominal_guard,
    param_accepts_weak_string_coercion,
    strict_param_binding_rejection, ParamBinding,
};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;
use super::super::inference::php_declared_type_spelling;

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
    pub(crate) fn require_bound_param_arg_type_checked(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        env: &TypeEnv,
        context: &str,
        owner: Option<(&str, &str)>,
        by_ref: bool,
    ) -> Result<(), CompileError> {
        self.require_strict_types_param_binding(expected, actual, arg.span, context)?;
        // Null-only promoted parameters are checked before constructor ABI materialization.
        // Keep by-reference storage outside this value-binding exception.
        if !by_ref && *expected == PhpType::Void && actual.codegen_repr() == PhpType::Mixed {
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
            if expected.is_php_array()
                && matches!(actual.codegen_repr(), PhpType::Mixed)
                && matches!(arg.kind, crate::parser::ast::ExprKind::Variable(_))
            {
                return Ok(());
            }
            return self.require_compatible_arg_type(expected, actual, arg.span, context);
        }
        // Two instantiations (`Box<Dog>` for `Box<Animal>`) that `type_accepts` just refused are
        // an invariant mismatch the checker owns: a run-time nominal guard would only turn the
        // compile-time diagnostic (and its variance hint) into a late `TypeError`.
        let both_instantiations = matches!(
            (expected, actual),
            (PhpType::Object(e), PhpType::Object(a)) if e.contains('<') && a.contains('<')
        );
        if (!both_instantiations && object_requires_runtime_nominal_guard(expected, actual))
            || gradual_object_requires_runtime_nominal_guard(expected, actual)
            || nullable_int_requires_runtime_param_guard(expected, actual)
            || gradual_union_requires_runtime_param_guard(expected, actual)
            || self.nominal_boundary_narrows_to_one_class(expected, actual)
        {
            return Ok(());
        }
        // A `?int` into `string` (symfony/validator's `->setParameter('{{ limit }}',
        // $constraint->max)` once a call dropped the `null !==` narrowing): php converts the int
        // and throws its TypeError for a null. Lowering's `weak_tagged_scalar` arm
        // (`coerce_scalar_arg_to_param_storage`) is that check and cast, under the same condition.
        if !self.strict_types
            && actual.codegen_repr() == PhpType::TaggedScalar
            && crate::types::param_binding::scalar_param_cast(expected, &PhpType::Int)
                == Some(crate::parser::ast::CastType::String)
        {
            return Ok(());
        }
        if !self.strict_types && param_accepts_weak_string_coercion(expected) {
            if let PhpType::Object(class_name) = actual.codegen_repr() {
                if self.object_supports_weak_string_coercion(&class_name) {
                    return Ok(());
                }
            }
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
                self.register_bound_callable_param_sig(owner, arg.span, sig);
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
                    // An object whose class declares `__invoke` IS a callable: php binds it to a
                    // `callable` parameter, and the call through the parameter runs `__invoke`.
                    // Guzzle's handlers (`CurlHandler`, `StreamHandler`) are passed that way.
                    // Lowering normalizes the object to a descriptor at this same boundary.
                    if matches!(actual.codegen_repr(), PhpType::Object(_))
                        && self.invokable_class_for_type(actual).is_some()
                    {
                        let target = crate::parser::ast::CallableTarget::Method {
                            object: Box::new(arg.clone()),
                            method: "__invoke".to_string(),
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
                        self.register_bound_callable_param_sig(owner, arg.span, sig);
                        return Ok(());
                    }
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
                    self.register_bound_callable_param_sig(owner, arg.span, sig);
                    return Ok(());
                }
                self.require_compatible_arg_type(expected, actual, arg.span, context)
            }
        }
    }

    /// [`Self::require_bound_param_arg_type_checked`] with php's run-time `TypeError` substituted
    /// for a compile refusal when the mismatch is statically definite.
    ///
    /// php never refuses `f("8")` under `strict_types=1` (or `g([1])` for `function g(Foo $x)`)
    /// at compile time: the call evaluates its arguments and throws. A refusal here made the
    /// compiler hand a whole vendor file to the interpreter for code that php loads fine.
    pub(crate) fn require_bound_param_arg_type(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        env: &TypeEnv,
        context: &str,
        owner: Option<(&str, &str)>,
        by_ref: bool,
    ) -> Result<(), CompileError> {
        // Two classes with no subtype relation never bind, yet the checker would accept the pair
        // and leave a run-time nominal guard whose message is not php's.
        if matches!((expected, actual), (PhpType::Object(_), PhpType::Object(_)))
            && self.defer_definite_param_mismatch(expected, actual, arg, context, by_ref)
        {
            return Ok(());
        }
        match self.require_bound_param_arg_type_checked(
            expected, actual, arg, env, context, owner, by_ref,
        ) {
            Ok(()) => {
                self.forget_definite_param_mismatch(arg);
                Ok(())
            }
            Err(err) => {
                if self.defer_definite_param_mismatch(expected, actual, arg, context, by_ref)
                    || self.defer_runtime_int_coercion(expected, actual, arg, context, by_ref)
                {
                    Ok(())
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Records a weak-mode string argument to an `int` parameter as php's run-time coercion and
    /// returns whether it did.
    ///
    /// php converts `f($s)` for `function f(int $n)` when the call runs: a numeric string binds,
    /// anything else is a `TypeError`. Refusing it handed whole vendor files to the interpreter
    /// (symfony's ICU polyfill `IntlDateFormatter::format()` passes a string timestamp) and cost a
    /// recompile. A float binds the same way, to `int` or `?int`: an integral value as it stands,
    /// a fractional one with php's "loses precision" deprecation, NaN, the infinities and an
    /// out-of-range value with the `TypeError` (Symfony's `LoginThrottlingListener` passes
    /// `ceil(...)`).
    pub(crate) fn defer_runtime_int_coercion(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        context: &str,
        by_ref: bool,
    ) -> bool {
        if self.strict_types
            || by_ref
            || self.binding_position == 0
            || self.internal_callback_binding
            || !arg.span.identifies_a_node()
            || matches!(arg.kind, ExprKind::Spread(_))
            || !(*expected == PhpType::Int
                || (actual.codegen_repr() == PhpType::Float
                    && crate::types::param_binding::is_nullable_int(expected)))
            || !matches!(actual.codegen_repr(), PhpType::Str | PhpType::Float)
            // A float bound to `?int` is never the null arm: it converts like one bound to `int`.
            || !matches!(
                classify_param_binding(
                    if crate::types::param_binding::is_nullable_int(expected) {
                        &PhpType::Int
                    } else {
                        expected
                    },
                    actual,
                    arg,
                ),
                ParamBinding::NeedsRuntimeCheck(_)
            )
            // A float literal php deprecates (`2.5`) or refuses (`NAN`) takes the same run-time
            // conversion, which prints that deprecation or throws that TypeError when it runs.
            && !(actual.codegen_repr() == PhpType::Float
                && matches!(
                    classify_param_binding(&PhpType::Int, actual, arg),
                    ParamBinding::Deprecated(_) | ParamBinding::TypeError(_)
                ))
        {
            return false;
        }
        let Some((callee, param)) = php_callee_label(context) else {
            return false;
        };
        let callee = self.declaring_callee_label(callee);
        let prefix = format!(
            "{}(): Argument #{} (${}) must be of type int, ",
            callee, self.binding_position, param
        );
        crate::types::checker::record_throw_access_site(
            &mut self.throw_access_sites,
            self.current_loop_storage_scope.clone(),
            arg.span,
            crate::types::ThrowAccessInfo {
                span: arg.span,
                kind: crate::types::ThrowAccessKind::CallArgumentIntCoercion { prefix },
            },
        );
        true
    }

    /// Drops a deferred mismatch recorded for `arg` by an earlier visit of the same call: the
    /// checker re-checks bodies as types settle, and only the last visit's verdict is true.
    pub(crate) fn forget_definite_param_mismatch(&mut self, arg: &Expr) {
        if self.throw_access_sites.is_empty() {
            return;
        }
        let key = (self.current_loop_storage_scope.clone(), arg.span);
        if matches!(
            self.throw_access_sites.get(&key).map(|info| &info.kind),
            Some(
                crate::types::ThrowAccessKind::CallArgumentType { .. }
                    | crate::types::ThrowAccessKind::CallArgumentIntCoercion { .. }
            )
        ) {
            self.throw_access_sites.remove(&key);
        }
    }

    /// Records `arg` as a definite binding failure to throw at run time and returns whether it
    /// did. Only provable cases qualify: see [`Self::definite_param_mismatch_given`].
    pub(crate) fn defer_definite_param_mismatch(
        &mut self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
        context: &str,
        by_ref: bool,
    ) -> bool {
        if by_ref
            || self.binding_position == 0
            || self.internal_callback_binding
            || !arg.span.identifies_a_node()
            || matches!(arg.kind, ExprKind::Spread(_))
        {
            return false;
        }
        let Some((callee, param)) = php_callee_label(context) else {
            return false;
        };
        let Some(given) = self.definite_param_mismatch_given(expected, actual, arg) else {
            return false;
        };
        let callee = self.declaring_callee_label(callee);
        let message = format!(
            "{}(): Argument #{} (${}) must be of type {}, {} given",
            callee,
            self.binding_position,
            param,
            if expected.is_php_array() {
                "array".to_string()
            } else {
                php_declared_type_spelling(expected)
            },
            given
        );
        crate::types::checker::record_throw_access_site(
            &mut self.throw_access_sites,
            self.current_loop_storage_scope.clone(),
            arg.span,
            crate::types::ThrowAccessInfo {
                span: arg.span,
                kind: crate::types::ThrowAccessKind::CallArgumentType { message },
            },
        );
        true
    }

    /// Rewrites `Receiver::method` to `Declaring::method`: php names the class that DECLARES the
    /// method in a binding `TypeError`, so an inherited method called on a subclass says
    /// `Parent::m()`. A name this checker cannot resolve to a class method is returned unchanged.
    fn declaring_callee_label(&self, callee: String) -> String {
        let Some((class_name, method)) = callee.split_once("::") else {
            return callee;
        };
        let key = crate::names::php_symbol_key(method);
        let declaring = self.classes.get(class_name).and_then(|info| {
            info.method_declaring_classes
                .get(&key)
                .or_else(|| info.static_method_declaring_classes.get(&key))
        });
        match declaring {
            Some(declaring) => format!("{}::{}", declaring.trim_start_matches('\\'), method),
            None => callee,
        }
    }

    /// Returns php's name for the value's type when binding `actual` to `expected` can never
    /// succeed at run time, whatever the value turns out to be.
    ///
    /// Deliberately narrow: the static type must pin down the run-time kind. Under
    /// `strict_types=1` that is any scalar pair php does not convert. In either mode it is an
    /// array, a scalar or a literal `null` handed to a class or array parameter, an object handed
    /// to an array parameter, and two unrelated classes (neither a subtype of the other, so no
    /// run-time subclass of the argument's class can be the parameter's class either). Anything
    /// involving an interface, a union, `mixed` or a boolean of unknown value stays a refusal.
    fn definite_param_mismatch_given(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        arg: &Expr,
    ) -> Option<String> {
        let literal_null = matches!(arg.kind, ExprKind::Null);
        let given_scalar = |ty: &PhpType| -> Option<String> {
            match ty {
                PhpType::Int => Some("int".to_string()),
                PhpType::Float => Some("float".to_string()),
                PhpType::Str => Some("string".to_string()),
                PhpType::Bool => match arg.kind {
                    ExprKind::BoolLiteral(true) => Some("true".to_string()),
                    ExprKind::BoolLiteral(false) => Some("false".to_string()),
                    _ => None,
                },
                // The literal `false` singleton (`$v->isValid($s, false, true)` in Symfony's
                // `EmailValidator`, on a branch egulias/email-validator 4 never reaches).
                PhpType::False => Some("false".to_string()),
                _ => None,
            }
        };
        let actual_is_array =
            actual.is_php_array() || matches!(actual, PhpType::Array(_) | PhpType::AssocArray { .. });
        if self.strict_types && strict_param_binding_rejection(expected, actual).is_some() {
            return given_scalar(actual);
        }
        let given = if literal_null {
            "null".to_string()
        } else if actual_is_array {
            "array".to_string()
        } else if let Some(scalar) = given_scalar(actual) {
            scalar
        } else if let PhpType::Object(name) = actual {
            if name.is_empty() {
                return None;
            }
            name.clone()
        } else {
            return None;
        };
        match expected {
            PhpType::Object(expected_name) if !expected_name.is_empty() => {
                let PhpType::Object(actual_name) = actual else {
                    return Some(given);
                };
                if literal_null {
                    return Some(given);
                }
                let both_classes = self.classes.contains_key(expected_name.as_str())
                    && self.classes.contains_key(actual_name.as_str())
                    && !self.interfaces.contains_key(expected_name.as_str())
                    && !self.interfaces.contains_key(actual_name.as_str());
                let related = self.is_subclass_of(expected_name, actual_name)
                    || self.is_subclass_of(actual_name, expected_name)
                    || expected_name == actual_name;
                // An instantiation (`Box<Dog>`) is an elephc extension, not a php class: whether
                // `Box<Dog>` binds `Box<Animal>` is the checker's variance rule, which accepts
                // `+T` widenings and refuses invariant mismatches at COMPILE time. Deferring
                // would throw at run time for the former and silence the latter's diagnostic.
                let instantiation = expected_name.contains('<') || actual_name.contains('<');
                (both_classes && !instantiation && !related).then_some(given)
            }
            expected if expected.is_php_array() => {
                if actual_is_array {
                    None
                } else {
                    Some(given)
                }
            }
            _ => None,
        }
    }

    /// Returns whether a concrete object type can participate in PHP's weak object-to-string
    /// parameter conversion.
    ///
    /// Explicit `Stringable` implementation is authoritative. A public, string-returning
    /// `__toString` is checked directly as well because PHP grants `Stringable` implicitly,
    /// while the checker may validate a call before its final implicit-interface enrichment.
    pub(crate) fn object_supports_weak_string_coercion(&self, class_name: &str) -> bool {
        if self.object_type_implements_interface(class_name, "Stringable") {
            return true;
        }
        let method_key = php_symbol_key("__toString");
        let mut current = Some(class_name);
        while let Some(candidate) = current {
            let Some(class_info) = self.classes.get(candidate) else {
                return false;
            };
            if class_info
                .methods
                .get(&method_key)
                .is_some_and(|sig| sig.return_type == PhpType::Str)
                && class_info.method_visibilities.get(&method_key) == Some(&Visibility::Public)
            {
                return true;
            }
            current = class_info.parent.as_deref();
        }
        false
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
        &self,
        expected: &PhpType,
        actual: &PhpType,
        span: Span,
        context: &str,
    ) -> Result<(), CompileError> {
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
    fn register_bound_callable_param_sig(
        &mut self,
        owner: Option<(&str, &str)>,
        span: Span,
        sig: FunctionSig,
    ) {
        let Some((owner_name, param_name)) = owner else {
            return;
        };
        let key = (owner_name.to_string(), param_name.to_string());
        self.record_callable_param_sig(key, span, sig);
    }

    /// Records the signature the argument at `site` gives a `callable` parameter and returns
    /// whether the parameter's joined signature changed.
    ///
    /// The callee body is checked against ONE signature per parameter, so a parameter reached by
    /// closures of different shapes needs the join of all of them. Keeping only the site checked
    /// last typed `function attempt(callable $f) { return $f(); }` for the final caller's closure:
    /// `attempt(fn() => new R())` beside `attempt(fn() => true)` returned the object where a
    /// boolean was read (and the other way round), whichever call came last.
    ///
    /// Sites are keyed by span so a site re-checked with a refined signature REPLACES its own
    /// earlier answer instead of widening the join with a stale one.
    pub(crate) fn record_callable_param_sig(
        &mut self,
        key: (String, String),
        site: Span,
        sig: FunctionSig,
    ) -> bool {
        let sites = self.callable_param_site_sigs.entry(key.clone()).or_default();
        sites.insert(site, sig);
        // Span order, not the map's, so the join does not depend on hash iteration order.
        let mut ordered: Vec<(&Span, &FunctionSig)> = sites.iter().collect();
        ordered.sort_by_key(|(span, _)| (span.line, span.col, span.end_line, span.end_col));
        let mut joined = ordered[0].1.clone();
        for (_, other) in ordered.iter().skip(1) {
            joined = Self::join_callable_sigs(joined, other);
        }
        if self.callable_param_sigs.get(&key) == Some(&joined) {
            return false;
        }
        self.callable_param_sigs.insert(key, joined);
        true
    }

    /// Joins two callable signatures a parameter may be called through.
    ///
    /// Equal signatures stay as they are. Otherwise every position they disagree on becomes
    /// `mixed`, and the result keeps the longer parameter list, so the body reads the result and
    /// the arguments through the boxed representation both closures can be invoked with.
    fn join_callable_sigs(base: FunctionSig, other: &FunctionSig) -> FunctionSig {
        if &base == other {
            return base;
        }
        let (mut joined, shorter) = if other.params.len() > base.params.len() {
            (other.clone(), &base)
        } else {
            (base.clone(), other)
        };
        for (index, (_, param_ty)) in joined.params.iter_mut().enumerate() {
            if shorter.params.get(index).map(|(_, ty)| ty) != Some(&*param_ty) {
                *param_ty = PhpType::Mixed;
            }
        }
        if base.return_type != other.return_type {
            joined.return_type = PhpType::Mixed;
        }
        joined
    }

    /// Returns whether a multi-class union boundary narrows to ONE class for this argument, so the
    /// runtime nominal guard has a class to test and PHP decides the rest at the call.
    ///
    /// Gated on the same predicate the lowering uses to pick that class
    /// (`nominal_object_boundary_target_for_source`), so the two cannot disagree about which calls
    /// are accepted.
    fn nominal_boundary_narrows_to_one_class(&self, expected: &PhpType, actual: &PhpType) -> bool {
        if !matches!(
            actual.codegen_repr(),
            PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
        ) {
            return false;
        }
        crate::types::param_binding::nominal_object_boundary_target_for_source(
            expected,
            actual,
            &|candidate, source| self.nominal_boundary_classes_are_related(candidate, source),
        )
        .is_some()
    }

    /// Returns whether two class names name the same class or one descends from the other.
    fn nominal_boundary_classes_are_related(&self, candidate: &str, source: &str) -> bool {
        if candidate.eq_ignore_ascii_case(source) {
            return true;
        }
        if !self.classes.contains_key(candidate) || !self.classes.contains_key(source) {
            return true;
        }
        self.nominal_boundary_class_descends_from(candidate, source)
            || self.nominal_boundary_class_descends_from(source, candidate)
    }

    /// Walks the parent chain of `class_name` looking for `ancestor`.
    fn nominal_boundary_class_descends_from(&self, class_name: &str, ancestor: &str) -> bool {
        let mut current = Some(class_name.to_string());
        while let Some(name) = current {
            if name.eq_ignore_ascii_case(ancestor) {
                return true;
            }
            current = self
                .classes
                .get(name.as_str())
                .and_then(|class_info| class_info.parent.clone());
        }
        false
    }
}

/// Splits a call-validator context (`Function 'ns\\f' parameter $x`, `Method C::m parameter $x`,
/// `Constructor 'C::__construct' parameter $x`) into php's callee name and the parameter name.
///
/// Returns `None` for contexts php words differently (variadic collectors, closures), which keep
/// their compile-time diagnostic.
fn php_callee_label(context: &str) -> Option<(String, String)> {
    let (head, param) = context.rsplit_once(" parameter $")?;
    if head.ends_with(" variadic") || param.contains(' ') {
        return None;
    }
    let callee = if let Some(rest) = head.strip_prefix("Function '") {
        rest.strip_suffix('\'')?.trim_start_matches('\\').to_string()
    } else if let Some(rest) = head.strip_prefix("Constructor '") {
        rest.strip_suffix('\'')?.trim_start_matches('\\').to_string()
    } else {
        let rest = ["Static method ", "Parent method ", "Self method ", "Ancestor method ", "Method "]
            .iter()
            .find_map(|prefix| head.strip_prefix(prefix))?;
        rest.trim_start_matches('\\').to_string()
    };
    if callee.is_empty() || callee.contains(' ') || callee.contains('{') {
        return None;
    }
    Some((callee, param.to_string()))
}
