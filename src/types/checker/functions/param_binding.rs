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
use crate::parser::ast::Expr;
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
            self.record_declared_callable_literal_name(arg);
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

    /// Returns the strict-call validation signature for Tidy's deliberately widened adapters.
    ///
    /// The injected prelude declares `$config` and `$encoding` as `mixed` so its runtime
    /// adapters can reproduce internal Tidy's weak scalar coercion and TypeError ordering.
    /// Under `declare(strict_types=1)` PHP's internal parameter parser still rejects scalars
    /// against `array|string|null` and `?string`; restore those contract types for the
    /// checker's strict validation only, leaving weak-mode runtime behavior untouched.
    pub(crate) fn tidy_strict_adapter_signature(
        &self,
        function: Option<&str>,
        owner: Option<&str>,
        callee_desc: Option<&str>,
        sig: &FunctionSig,
    ) -> Option<FunctionSig> {
        if !self.strict_types {
            return None;
        }
        let tidy_function = function.is_some_and(|name| {
            matches!(
                name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
                "tidy_parse_string" | "tidy_parse_file" | "tidy_repair_string" | "tidy_repair_file"
            )
        });
        let tidy_method = owner
            .zip(callee_desc)
            .is_some_and(|(owner, callee_desc)| self.tidy_prelude_declares_method(owner, callee_desc));
        if !tidy_function && !tidy_method {
            return None;
        }
        let mut adjusted = sig.clone();
        let mut changed = false;
        for (name, ty) in &mut adjusted.params {
            if *ty != PhpType::Mixed {
                continue;
            }
            if name.eq_ignore_ascii_case("config") {
                *ty = self.normalize_union_type(vec![
                    PhpType::php_array(),
                    PhpType::Str,
                    PhpType::Void,
                ]);
                changed = true;
            } else if name.eq_ignore_ascii_case("encoding") {
                *ty = self.normalize_union_type(vec![PhpType::Str, PhpType::Void]);
                changed = true;
            }
        }
        changed.then_some(adjusted)
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
        if let Some(arms) = selected_value_arms(static_callable_argument(arg)) {
            // Each arm of `?:`, `??` or `match` is the bound value itself: a literal arm is
            // resolved like a direct literal (an unknown name is a compile error and its
            // libraries are linked). No signature is registered, since the arms may differ.
            for arm in arms {
                if matches!(arm.kind, crate::parser::ast::ExprKind::StringLiteral(_))
                    || selected_value_arms(arm).is_some()
                {
                    let arm_ty = if matches!(arm.kind, crate::parser::ast::ExprKind::StringLiteral(_)) {
                        PhpType::Str
                    } else {
                        actual.clone()
                    };
                    self.require_declared_callable_arg(expected, &arm_ty, arm, env, context, None)?;
                }
            }
        }
        let target = match (actual.codegen_repr(), &static_callable_argument(arg).kind) {
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
    pub(crate) fn record_declared_callable_literal_name(&mut self, value: &Expr) {
        let value = static_callable_argument(value);
        if let Some(arms) = selected_value_arms(value) {
            // Each arm of a conditional is the value itself, so it is a certain callable.
            for arm in arms {
                self.record_declared_callable_literal_name(arm);
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
            self.record_declared_callable_literal_name(arg);
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
                        | ArrayEntry::Value(value) => {
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

/// Returns the value an argument binds: through a named argument, an assignment
/// (`apply($name = "strlen")`) or the `@` operator, which all evaluate to their inner value.
///
/// Call planning wraps a keyed variadic entry as a named argument, so a literal callable name
/// written there sits behind that node too.
fn static_callable_argument(arg: &Expr) -> &Expr {
    use crate::parser::ast::ExprKind;
    match &arg.kind {
        ExprKind::NamedArg { value, .. } | ExprKind::Assignment { value, .. } => {
            static_callable_argument(value)
        }
        ExprKind::ErrorSuppress(inner) => static_callable_argument(inner),
        _ => arg,
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
