//! Purpose:
//! Checks type compatibility for declarations cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::types::{callable_wrapper_sig, ClassInfo, FunctionSig, PhpType, TypeEnv};

use super::super::inference::syntactic::infer_expr_type_syntactic;
use super::super::{Checker, FnDecl};

impl Checker {
    /// Applies the callable-wrapper transformation to `sig`, producing a new `FunctionSig`
    /// with an additional `$wrapper` closure parameter prepended. This allows first-class
    /// callable syntax on user-defined functions to be dispatched through the runtime's
    /// closure-invocation mechanism.
    pub(crate) fn callable_wrapper_sig(sig: &FunctionSig) -> FunctionSig {
        callable_wrapper_sig(sig)
    }

    /// The storage a declared `callable` parameter gets in `body`.
    ///
    /// A `callable` parameter is normally normalized into a callable descriptor at the call
    /// boundary, which is what makes `$callback(...)` a direct call. That normalization forgets
    /// the value php keeps: `is_string('strlen')`, `is_array([$o, 'm'])`, `$callback[0]` and
    /// `get_class()` all answered as if a `Closure` had been passed (symfony/cache's
    /// `EarlyExpirationMessage::create()` reads `$callback[0]` and `\is_object($callback)`). A body
    /// that INSPECTS the parameter's value -- `instanceof`, an element read, a comparison or
    /// concatenation, a type probe, a reassignment -- keeps the caller's value unconverted
    /// (`Mixed`); a call on it then resolves the callable when it runs. A body that calls it or
    /// only forwards it to another call keeps the descriptor: forwarding needs no value, and the
    /// descriptor is what lets a string such as `'strcasecmp'` reach `array_udiff()` resolved.
    /// The declared `TypeExpr` is untouched, so reflection still reports `callable`.
    pub(crate) fn callable_param_storage(
        declared: PhpType,
        body: &[crate::parser::ast::Stmt],
        name: &str,
    ) -> PhpType {
        // `?callable` too: twig's `TwigFunction::__construct(..., ?callable $callable = null)`.
        let is_callable_contract = match &declared {
            PhpType::Callable => true,
            PhpType::Union(members) => {
                members.contains(&PhpType::Callable)
                    && members.iter().all(|member| matches!(member, PhpType::Callable | PhpType::Void))
            }
            _ => false,
        };
        if !is_callable_contract {
            return declared;
        }
        // A compiler-built body (a prelude: `CallbackFilterIterator`, PDO) keeps the descriptor:
        // it stores the callback in a `callable`-typed property php code cannot declare, and
        // nothing there reads the value php would show. Only source code is ever line-less here.
        if body.iter().all(|stmt| stmt.span.line == 0) {
            return declared;
        }
        // A program that evaluates code keeps converting at the boundary: the interpreter answers
        // any callable name there, while the compiled call sites that later read a kept string
        // know only the builtins the program spells (`literal_builtin_gate`).
        if program_evaluates_code() {
            return declared;
        }
        const VALUE_PROBES: &[&str] = &[
            "is_string", "is_array", "is_object", "is_callable", "gettype", "get_debug_type",
            "get_class", "var_dump", "var_export", "print_r", "json_encode", "serialize",
            "spl_object_id", "spl_object_hash", "count", "implode", "explode", "strlen",
        ];
        let is_param = |expr: &Expr| matches!(&expr.kind, ExprKind::Variable(variable) if variable == name);
        let mut used_as_value = false;
        crate::parser::ast::visit::walk_nodes(body, &mut |node| match node {
            crate::parser::ast::visit::Node::Expr(expr) => match &expr.kind {
                ExprKind::InstanceOf { value, .. } if is_param(value) => used_as_value = true,
                ExprKind::ArrayAccess { array, .. } if is_param(array) => used_as_value = true,
                // A comparison with `null` (`$callback === null`) asks only whether one was passed,
                // which the descriptor answers as well.
                ExprKind::BinaryOp { left, right, .. }
                    if (is_param(left) || is_param(right))
                        && !matches!(left.kind, ExprKind::Null)
                        && !matches!(right.kind, ExprKind::Null) =>
                {
                    used_as_value = true
                }
                ExprKind::FunctionCall { name: function, args }
                    if args.iter().any(is_param)
                        && VALUE_PROBES.contains(
                            &crate::names::php_symbol_key(function.as_str().trim_start_matches('\\'))
                                .as_str(),
                        ) =>
                {
                    used_as_value = true
                }
                ExprKind::Assignment { target, .. } if is_param(target) => used_as_value = true,
                ExprKind::Assignment { value, .. } if is_param(value) => used_as_value = true,
                ExprKind::ArrayLiteral(items) if items.iter().any(is_param) => used_as_value = true,
                ExprKind::ArrayLiteralAssoc(pairs) if pairs.iter().any(|(_, value)| is_param(value)) => {
                    used_as_value = true
                }
                ExprKind::Closure { captures, .. }
                    if captures.iter().any(|capture| capture == name) =>
                {
                    used_as_value = true
                }
                _ => {}
            },
            crate::parser::ast::visit::Node::Stmt(stmt) => {
                use crate::parser::ast::StmtKind;
                if matches!(&stmt.kind, StmtKind::Assign { name: target, .. } if target == name) {
                    used_as_value = true;
                }
                // STORING the parameter keeps it to be read back later: twig's `TwigFunction`
                // stores its `callable $callable` in a property, and symfony/twig-bridge's
                // `DebugCommand` reads it back with `\is_array($cb)` and `$cb[0]`.
                let stored = match &stmt.kind {
                    // A local typed `callable` again (`callable $c = $callback;`) keeps nothing
                    // the descriptor loses: it is the same contract restated.
                    StmtKind::TypedAssign { type_expr, .. } if type_names_callable(type_expr) => None,
                    StmtKind::Assign { value, .. }
                    | StmtKind::TypedAssign { value, .. }
                    | StmtKind::ArrayAssign { value, .. }
                    | StmtKind::NestedArrayAssign { value, .. }
                    | StmtKind::ArrayPush { value, .. }
                    | StmtKind::PropertyAssign { value, .. }
                    | StmtKind::StaticPropertyAssign { value, .. }
                    | StmtKind::StaticPropertyArrayPush { value, .. }
                    | StmtKind::StaticPropertyArrayAssign { value, .. }
                    | StmtKind::PropertyArrayPush { value, .. }
                    | StmtKind::PropertyArrayAssign { value, .. } => Some(value),
                    _ => None,
                };
                if stored.is_some_and(is_param) {
                    used_as_value = true;
                }
            }
        });
        if used_as_value {
            PhpType::Mixed
        } else {
            declared
        }
    }

    /// Resolves a parameter type hint from a `TypeExpr` to a `PhpType`, validating that
    /// the type is valid for a parameter context. Rejects `void` and types containing `never`.
    /// Builds the signature a declared `callable(A): B` promises, for typing its INVOCATION.
    ///
    /// A bare `callable` answers `None` and nothing changes: it carries no types, which is the
    /// whole reason the declared form exists. Resolution of the halves goes through the ordinary
    /// type path, so a signature naming a class resolves that class like any other mention.
    pub(crate) fn declared_callable_signature(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
    ) -> Option<crate::types::FunctionSig> {
        let TypeExpr::CallableSig { params, ret } = type_expr else {
            return None;
        };
        let mut resolved = Vec::with_capacity(params.len());
        for (index, param) in params.iter().enumerate() {
            resolved.push((format!("arg{}", index), self.resolve_type_expr(param, span).ok()?));
        }
        let return_type = self.resolve_type_expr(ret, span).ok()?;
        Some(crate::types::FunctionSig {
            param_type_exprs: params.iter().map(|p| Some(p.clone())).collect(),
            param_attributes: vec![Vec::new(); resolved.len()],
            defaults: vec![None; resolved.len()],
            ref_params: vec![false; resolved.len()],
            declared_params: vec![true; resolved.len()],
            params: resolved,
            return_type,
            declared_return: true,
            by_ref_return: false,
            variadic: None,
            deprecation: None,
            // A declared callable type says nothing about how the callee produces its value.
            is_generator: false,
        })
    }

    /// Resolves a declared parameter type and rejects void or nested never types with contextual diagnostics.
    pub(crate) fn resolve_declared_param_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = self.resolve_type_expr_in_current_class(type_expr, span)?;
        let null_only = matches!(type_expr, TypeExpr::Named(name)
            if name.as_str().eq_ignore_ascii_case("null"));
        match ty {
            PhpType::Void if !null_only => Err(CompileError::new(
                span,
                &format!("{} cannot use type void", context),
            )),
            _ if Self::type_contains_never(&ty) => Err(CompileError::new(
                span,
                &format!("{} cannot use type never", context),
            )),
            _ => Ok(ty),
        }
    }

    /// Resolves a return type hint from a `TypeExpr` to a `PhpType`. Unlike parameter hints,
    /// `never` is allowed here as a standalone return type but not nested inside other types.
    pub(crate) fn resolve_declared_return_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        _context: &str,
    ) -> Result<PhpType, CompileError> {
        if !matches!(type_expr, TypeExpr::Never) && Self::type_expr_contains_never(type_expr) {
            return Err(CompileError::new(
                span,
                "never can only be used as a standalone return type",
            ));
        }
        self.resolve_type_expr_in_current_class(type_expr, span)
    }

    /// Resolves relative class names against the active lexical class before converting a
    /// declaration type expression, while preserving the outside-class diagnostics otherwise.
    fn resolve_type_expr_in_current_class(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        if let Some(current_class) = self.current_class.as_deref() {
            let parent = self
                .classes
                .get(current_class)
                .and_then(|class_info| class_info.parent.as_deref());
            let resolved = type_expr.substitute_relative_class_types(current_class, parent);
            return self.resolve_type_expr(&resolved, span);
        }
        self.resolve_type_expr(type_expr, span)
    }

    /// Resolves a method return contract to its declaring type for schema and ABI metadata.
    ///
    /// The parsed `static` marker remains on the method declaration for call-site refinement;
    /// this nominal type is the concrete declaring class or interface used for compatibility.
    pub(crate) fn resolve_method_return_type_hint(
        &self,
        type_expr: &TypeExpr,
        declaring_type: &str,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let nominal = type_expr.substitute_relative_class_types(declaring_type, None);
        self.resolve_declared_return_type_hint(&nominal, span, context)
    }

    /// Resolves preserved late-static return syntax against a concrete call-site receiver.
    pub(crate) fn resolve_late_static_return_type_hint(
        &self,
        type_expr: &TypeExpr,
        receiver_type: &str,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        let parent = self
            .classes
            .get(receiver_type)
            .and_then(|class_info| class_info.parent.as_deref());
        let bound = type_expr.substitute_relative_class_types(receiver_type, parent);
        self.resolve_declared_return_type_hint(&bound, span, "Late-static method return")
    }

    /// Resolves a local variable type hint from a `TypeExpr` to a `PhpType`, rejecting
    /// types containing `never`.
    pub(crate) fn resolve_declared_local_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = self.resolve_type_expr(type_expr, span)?;
        if Self::type_contains_never(&ty) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type never", context),
            ));
        }
        Ok(ty)
    }

    /// Resolves a property type hint from a `TypeExpr` to a `PhpType`, rejecting
    /// `void`, `never`, and the `callable` pseudo-type while allowing the `Closure` class.
    pub(crate) fn resolve_declared_property_type_hint(
        &self,
        type_expr: &TypeExpr,
        span: crate::span::Span,
        context: &str,
    ) -> Result<PhpType, CompileError> {
        let ty = if matches!(type_expr, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("array")) {
            PhpType::php_array()
        } else {
            self.resolve_type_expr(type_expr, span)?
        };
        let null_only = matches!(type_expr, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("null"));
        if matches!(ty, PhpType::Void) && !null_only {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type void", context),
            ));
        }
        if Self::type_contains_never(&ty) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type never", context),
            ));
        }
        if Self::type_expr_contains_callable_pseudo_type(type_expr) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use type callable", context),
            ));
        }
        Ok(ty)
    }

    /// Returns true if `type_expr` contains PHP's forbidden property pseudo-type `callable`.
    /// The `Closure` class resolves to the same internal callable representation but remains a
    /// valid property declaration, including inside nullable and union types.
    fn type_expr_contains_callable_pseudo_type(type_expr: &TypeExpr) -> bool {
        match type_expr {
            TypeExpr::Named(name) => name.as_str().eq_ignore_ascii_case("callable"),
            TypeExpr::Array(inner) | TypeExpr::Nullable(inner) | TypeExpr::Buffer(inner) => {
                Self::type_expr_contains_callable_pseudo_type(inner)
            }
            TypeExpr::Union(members) | TypeExpr::Intersection(members) => members
                .iter()
                .any(Self::type_expr_contains_callable_pseudo_type),
            _ => false,
        }
    }

    /// Returns true if `ty` is or contains a `PhpType::Never` anywhere in its structure.
    fn type_contains_never(ty: &PhpType) -> bool {
        match ty {
            PhpType::Never => true,
            PhpType::Union(members) => members.iter().any(Self::type_contains_never),
            PhpType::Array(inner) | PhpType::Buffer(inner) => Self::type_contains_never(inner),
            PhpType::AssocArray { key, value } => {
                Self::type_contains_never(key) || Self::type_contains_never(value)
            }
            _ => false,
        }
    }

    /// Returns true if `type_expr` is or contains a `TypeExpr::Never` anywhere in its structure.
    fn type_expr_contains_never(type_expr: &TypeExpr) -> bool {
        match type_expr {
            TypeExpr::Never => true,
            TypeExpr::Array(inner) | TypeExpr::Nullable(inner) | TypeExpr::Buffer(inner) => {
                Self::type_expr_contains_never(inner)
            }
            TypeExpr::Union(members) => members.iter().any(Self::type_expr_contains_never),
            _ => false,
        }
    }

    /// Validates that the caller's variable is suitable storage for a declared
    /// by-reference parameter, which writes its result back through that variable.
    ///
    /// Two rules, both about the WRITE-BACK rather than the incoming value:
    ///
    /// 1. A parameter whose declared type needs boxed or nullable storage cannot write
    ///    every value it accepts into a concrete non-boxed slot. Local array slots that EIR
    ///    can widen, and the reference shapes it can promote, are accepted before that rule.
    /// 2. A parameter whose declared type does NOT accept null cannot be handed a variable
    ///    that holds null, because the slot's representation is the declared scalar's and
    ///    nothing coerces it on the way in. php-src agrees and throws
    ///    `TypeError: f(): Argument #N ($x) must be of type int, null given` for exactly
    ///    this shape (measured on 8.5.10).
    ///
    /// Rule 2 exists because `types_compatible` deliberately accepts `null` for `int`,
    /// `float` and `bool` — correct for a BY-VALUE parameter, which PHP coerces — and that
    /// acceptance was short-circuiting every later by-reference check. The argument was then
    /// admitted with the caller's local still typed `Void`, the callee wrote an int through
    /// the reference, and the first thing the caller did with the local reached EIR lowering
    /// as an operation on a null-typed value: issue #892 reported
    /// `unsupported EIR backend feature: icmp for PHP type Void`, positionless, from
    /// `$running = null; do { … } while ($running > 0);` around `curl_multi_exec()`.
    pub(crate) fn require_boxed_by_ref_storage(
        &mut self,
        expected_ty: &PhpType,
        actual_ty: &PhpType,
        arg: &Expr,
        env: &TypeEnv,
        can_widen_local: bool,
        context: &str,
    ) -> Result<(), CompileError> {
        // The caller-storage preparation pass widens concrete variables to boxed cells whenever
        // a gradual or nullable reference contract can replace their value. Conversely, EIR
        // lowering inserts a checked concrete temporary for a boxed caller passed to a concrete
        // reference parameter, then publishes the result back. Representation equality at the
        // source call site is therefore no longer a validity requirement (this also retires the
        // main-side "boxed variable cannot receive a raw scalar write" refusal).
        // A pointer by-reference parameter writes a raw pointer word back through the caller's
        // storage. Only a whole local can receive it (lowering then checks that its frame slot
        // is a pointer word); an array element lives in boxed or null element storage.
        if matches!(expected_ty.codegen_repr(), PhpType::Pointer(_))
            && !matches!(by_ref_storage_place(arg).kind, ExprKind::Variable(_))
        {
            let shown = crate::types::pointer_param_display(expected_ty);
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "{} expects {}: a pointer by-reference parameter writes a pointer word back \
                     through its argument, so pass a variable (bind the element to a `{} $p` \
                     variable first)",
                    context, shown, shown
                ),
            ));
        }
        // A `Void` value has no storage to write back through: the declared type must accept null.
        if *actual_ty == PhpType::Void && !Self::declared_type_accepts_null(expected_ty) {
            return Err(CompileError::new(
                arg.span,
                // The recovery names the VARIABLE in both branches, deliberately. Saying
                // "declare the parameter nullable" alone is advice that does not work:
                // `?int &$slot` needs the caller's variable to have nullable storage too, so
                // a bare `$v = null` still fails the boxed-storage rule. `?int $v = null` is the
                // spelling that compiles.
                &format!(
                    "{} expects {}, got null — a by-reference parameter writes back \
                     through the caller's variable, so that variable must already hold the \
                     declared type; initialize it (for example `= 0`), or declare BOTH the \
                     parameter and the variable nullable (`?int &$p` with `?int $v = null`)",
                    context, expected_ty
                ),
            ));
        }
        let _ = (can_widen_local, env);
        Ok(())
    }

    /// Returns whether a by-reference call may give one ordinary local canonical Mixed storage.
    pub(crate) fn by_ref_argument_can_widen_local_to_mixed(&self, arg: &Expr) -> bool {
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            return false;
        };
        !self.active_ref_params.contains(name)
            && !self.ref_aliased_locals.contains(name)
            && !self.active_globals.contains(name)
            && !self.static_local_names.contains(name)
            && !self.typed_local_names.contains_key(name)
            && !self.name_is_seeded_program_storage(name)
            && !self.top_level_binding_is_program_global(name)
    }

    /// Validates that a default value expression is compatible with the declared type it is
    /// being assigned to. Checks using `require_compatible_arg_type`.
    pub(crate) fn validate_declared_default_type(
        &self,
        expected_ty: &PhpType,
        default_expr: Option<&Expr>,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if let Some(default_expr) = default_expr {
            let default_ty = self.declaration_default_type(default_expr);
            self.require_compatible_arg_type(expected_ty, &default_ty, span, context)?;
        }
        Ok(())
    }

    /// Preserves known global constant types before a declaration is specialized by a call.
    fn declaration_default_type(&self, expression: &Expr) -> PhpType {
        if crate::types::deferred_defaults::deferred_error_message(expression).is_some() {
            // A normalized lexical Error has no result to constrain an untyped parameter.
            return PhpType::Mixed;
        }
        if let ExprKind::ConstRef(name) = &expression.kind {
            if let Some(ty) = self.constants.get(name.as_str()) {
                return ty.clone();
            }
        }
        infer_expr_type_syntactic(expression)
    }

    /// Semantically resolves a declaration default when it is a scoped constant access, then
    /// validates the resolved type against the declared type. Other defaults keep the syntactic
    /// validation used by declarations that do not depend on completed class-like metadata.
    pub(crate) fn validate_resolved_declared_default_type(
        &mut self,
        expected_ty: &PhpType,
        default_expr: Option<&Expr>,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        let Some(default_expr) = default_expr else {
            return Ok(());
        };
        // PHP permits even a scalar-typed parameter to use an unbound object factory.
        // An explicit argument bypasses it; omission raises the scope Error before type binding.
        if crate::types::deferred_defaults::deferred_error_message(default_expr).is_some() {
            return Ok(());
        }
        let default_ty = match &default_expr.kind {
            ExprKind::ScopedConstantAccess { receiver, name } => {
                self.infer_scoped_constant_access(receiver, name, default_expr)?
            }
            _ => self.declaration_default_type(default_expr),
        };
        self.require_compatible_arg_type(expected_ty, &default_ty, span, context)
    }

    /// Validates a declaration default while class-like schema metadata is still being built.
    /// Object-to-object checks are deferred because inheritance and interface relationships are
    /// incomplete during this phase; every other type pair is validated immediately.
    pub(crate) fn validate_schema_declared_default_type(
        &self,
        expected_ty: &PhpType,
        default_expr: Option<&Expr>,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        // A direct scoped-constant default is DEFERRED, not accepted: enum cases and
        // class/interface constants do not exist yet while schemas are being built, so
        // `infer_expr_type_syntactic` answers `Str` for `Level::Low` and a declared
        // `public Level $level = Level::Low;` was rejected with "expects Object(\"Level\"), got
        // Str" — a constant expression PHP accepts (issue #566).
        //
        // `schema::defaults::validate_deferred_default` revalidates every one of these once the
        // schemas are complete, resolving the constant semantically, so nothing is waved
        // through: a missing case or an incompatible scalar constant is still reported, just
        // from the pass that can tell the difference.
        if default_expr
            .is_some_and(|default| matches!(default.kind, ExprKind::ScopedConstantAccess { .. }))
        {
            return Ok(());
        }
        if let Some(default_expr) = default_expr {
            let default_ty = self.declaration_default_type(default_expr);
            if matches!(expected_ty, PhpType::Object(_)) && matches!(default_ty, PhpType::Object(_))
            {
                return Ok(());
            }
        }
        self.validate_declared_default_type(expected_ty, default_expr, span, context)
    }

    /// Validates a method parameter default while class-like schemas are being built.
    /// Direct scoped constant accesses are deferred until enum cases and class/interface
    /// constants are available; other defaults use the existing schema-time validation.
    pub(crate) fn validate_schema_parameter_default_type(
        &self,
        expected_ty: &PhpType,
        default_expr: Option<&Expr>,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        // The scoped-constant deferral now lives in the declared-default validator below,
        // because a directly declared PROPERTY needs it for the same reason a parameter does
        // (issue #566). Kept as its own entry point so the two callers stay distinguishable at
        // the call site, and so a future parameter-only rule has somewhere to go.
        self.validate_schema_declared_default_type(expected_ty, default_expr, span, context)
    }

    /// Builds the initial parameter type list for a function declaration, resolving type hints,
    /// validating defaults, and inferring types for untyped parameters. Untyped by-reference
    /// parameters keep canonical Mixed storage. Adds a variadic parameter array type, using the
    /// declared element type for typed variadics.
    pub(crate) fn initial_function_param_types(
        &mut self,
        name: &str,
        decl: &FnDecl,
    ) -> Result<Vec<(String, PhpType)>, CompileError> {
        let mut param_types = Vec::new();
        for (idx, param_name) in decl.params.iter().enumerate() {
            if let Some(type_ann) = decl.param_types.get(idx).and_then(|t| t.as_ref()) {
                let declared_ty = self.resolve_declared_param_type_hint(
                    type_ann,
                    decl.span,
                    &format!("Function '{}' parameter ${}", name, param_name),
                )?;
                self.validate_resolved_declared_default_type(
                    &declared_ty,
                    decl.defaults.get(idx).and_then(|d| d.as_ref()),
                    decl.span,
                    &format!("Function '{}' parameter ${}", name, param_name),
                )?;
                let declared_ty = Self::callable_param_storage(declared_ty, &decl.body, param_name);
                param_types.push((param_name.clone(), declared_ty));
            } else if decl.ref_params.get(idx).copied().unwrap_or(false) {
                // An untyped reference is a writable PHP cell, not a call-site-specialized
                // scalar. The callee may legally replace its value with any PHP type, including
                // when the parameter has a scalar or array default.
                param_types.push((param_name.clone(), PhpType::Mixed));
            } else if let Some(default_expr) = decl.defaults.get(idx).and_then(|d| d.as_ref()) {
                param_types.push((param_name.clone(), self.declaration_default_type(default_expr)));
            } else {
                param_types.push((param_name.clone(), PhpType::Int));
            }
        }
        if let Some(variadic_name) = decl.variadic.as_ref() {
            let elem_ty = if decl.variadic_by_ref {
                PhpType::Mixed
            } else if let Some(type_ann) = decl.variadic_type.as_ref() {
                self.resolve_declared_param_type_hint(
                    type_ann,
                    decl.span,
                    &format!("Function '{}' variadic parameter ${}", name, variadic_name),
                )?
            } else {
                PhpType::Int
            };
            param_types.push((variadic_name.clone(), PhpType::Array(Box::new(elem_ty))));
        }
        Ok(param_types)
    }

    /// Returns a bitvec indicating which parameters of a method have declared type hints.
    ///
    /// The resolved signature is authoritative. Flattened declaration lists can retain more
    /// than one declaration with the same method name across an inheritance chain, so selecting
    /// the first syntax node can associate an overriding signature with an ancestor's flags and
    /// accidentally specialize declared parameters from call-site values.
    /// Local declarations provide the annotations; inherited methods retain them in their signature.
    pub(crate) fn declared_method_param_flags(
        class_info: &ClassInfo,
        method_name: &str,
        is_static: bool,
    ) -> Vec<bool> {
        let method_key = crate::names::php_symbol_key(method_name);
        let resolved = if is_static {
            class_info.static_methods.get(&method_key)
        } else {
            class_info.methods.get(&method_key)
        };
        if let Some(signature) = resolved {
            return signature.declared_params.clone();
        }
        class_info
            .method_decls
            .iter()
            .find(|method| {
                crate::names::php_symbol_key(&method.name) == method_key
                    && method.is_static == is_static
            })
            .map(|method| {
                // A typed variadic (`int ...$xs`) is a declared parameter too, so its element
                // type survives the undeclared-params-become-mixed pass and stays enforceable.
                method
                    .params
                    .iter()
                    .map(|(_, type_ann, _, _)| type_ann.is_some())
                    .chain(method.variadic.iter().map(|_| method.variadic_type.is_some()))
                    .collect()
            })
            .unwrap_or_else(|| {
                let signatures = if is_static { &class_info.static_methods } else { &class_info.methods };
                signatures.get(&method_key).map(|sig| sig.declared_params.clone()).unwrap_or_default()
            })
    }

    /// Adjusts a function signature so that parameters without declared type hints are marked
    /// as `PhpType::Mixed`. Untyped parameters become `Mixed` to allow flexible runtime dispatch.
    pub(crate) fn callable_sig_for_declared_params(
        sig: &FunctionSig,
        declared_flags: &[bool],
    ) -> FunctionSig {
        let mut effective_sig = sig.clone();
        for (idx, (_, ty)) in effective_sig.params.iter_mut().enumerate() {
            if !declared_flags.get(idx).copied().unwrap_or(false) {
                *ty = PhpType::Mixed;
            }
        }
        effective_sig.declared_params = declared_flags.to_vec();
        effective_sig
    }

    /// Temporarily replaces the checker's active ref params, globals, and statics stacks with
    /// the given values while running `f`. Saves and restores all state afterward to avoid
    /// leaking context across nested checks.
    ///
    /// `param_names` is every parameter, typed or not, and seeds their binding depth at 0
    /// (see [`Checker::enter_local_binding_scope`]). A parameter's TYPE HINT is not carried
    /// into the declared-type exclusion set: it constrains the incoming argument, not the
    /// local, so a type-hinted parameter is as retypable as any other local. The rest of the
    /// per-body local-binding eligibility state (conditional depth, binding depths, reference
    /// aliases, `static` names) is reset here too — it describes one frame and must not leak
    /// between caller and callee.
    ///
    /// `body` is the statement list `f` is about to check. It is taken so the mixed-storage
    /// pre-scan can run here, once the per-body state is installed and before any statement is
    /// checked: the scan's decision has to be in place at each marked local's FIRST store, where
    /// the slot's type is fixed. The freshly seeded `local_binding_depth` is exactly this body's
    /// parameter set, which the scan excludes from marking wholesale — a parameter's storage is
    /// already `mixed` by call-site specialization, or a declared contract, so marking would take
    /// credit for storage this feature did not create.
    pub(crate) fn with_local_storage_context<T, F>(
        &mut self,
        ref_param_names: Vec<String>,
        param_names: Vec<String>,
        pre_bound_own_storage: std::collections::HashMap<String, crate::types::PhpType>,
        body: &[crate::parser::ast::Stmt],
        f: F,
    ) -> Result<T, CompileError>
    where
        F: FnOnce(&mut Self) -> Result<T, CompileError>,
    {
        // A by-reference parameter that carries no type declaration holds a `mixed` cell: its
        // bound type is `Mixed` (see `build_method_sig`/`build_function_sig`), which is what the
        // pre-bound own-storage map records for it.
        let untyped_ref_params: std::collections::HashSet<String> = ref_param_names
            .iter()
            .filter(|name| {
                matches!(
                    pre_bound_own_storage.get(name.as_str()),
                    Some(crate::types::PhpType::Mixed)
                )
            })
            .cloned()
            .collect();
        let saved_untyped_ref_params =
            std::mem::replace(&mut self.untyped_ref_params, untyped_ref_params);
        let saved_local_binding_scope = self.enter_local_binding_scope(param_names);
        let saved_ref_params = self.active_ref_params.clone();
        let saved_external_ref_bindings = self.active_external_ref_bindings.clone();
        let saved_globals = self.active_globals.clone();
        let saved_statics = self.active_statics.clone();
        let saved_mixed_statics = self.mixed_static_locals.clone();
        let saved_foreach_keys = self.foreach_key_locals.clone();
        let saved_eval_barrier_active = self.eval_barrier_active;
        let saved_break_continue_depth = self.break_continue_depth;
        let saved_loop_break_envs = std::mem::take(&mut self.loop_break_envs);
        let saved_finally_break_continue_bases = self.finally_break_continue_bases.clone();
        let saved_null_probe_scope_is_top_level = self.null_probe_scope_is_top_level;

        self.active_ref_params = ref_param_names.into_iter().collect();
        self.active_external_ref_bindings = self.active_ref_params.clone();
        self.non_builtin_ref_aliased_locals
            .extend(self.active_ref_params.iter().cloned());
        self.boxed_ref_aliased_locals.extend(
            self.active_ref_params.iter()
                .filter(|name| pre_bound_own_storage.get(*name)
                    .is_some_and(|ty| matches!(ty, PhpType::Object(_))))
                .cloned(),
        );
        self.active_globals.clear();
        self.active_statics.clear();
        self.mixed_static_locals.clear();
        self.foreach_key_locals.clear();
        self.eval_barrier_active = false;
        self.break_continue_depth = 0;
        self.finally_break_continue_bases.clear();
        // A function/method/closure body is not the scope whose environment becomes
        // `global_env`, so null-probe roots found here must not be deferred against it.
        self.null_probe_scope_is_top_level = false;

        // Runs on the state installed above, and reads exactly one piece of it: the
        // `local_binding_depth` map `enter_local_binding_scope` has just filled with this body's
        // parameters, which is how EVERY parameter — typed or not, by reference or by value — is
        // kept unmarked. It also records here whether the body calls `eval()` anywhere, which
        // `Checker::local_binding_is_killable` then consults for the whole body.
        //
        // `pre_bound_own_storage` is what this body's OWN frame already holds on entry — a
        // closure's by-value captures, and the parameters — with the type each arrives with. NOT
        // the body's incoming environment: a closure body starts from a clone of the whole
        // enclosing scope, and keying on that silenced fresh closure locals that merely shared a
        // name with something outside. A marked name from this map is announced only when
        // `--strict-locals` would really reject the body; see `run_mixed_storage_scan`.
        self.run_mixed_storage_scan(body, &pre_bound_own_storage);
        let (sites, dependents) = super::super::numeric_regions::find(body, &pre_bound_own_storage);
        self.numeric_region_sites = sites;
        self.numeric_region_dependents = dependents;

        let result = f(self);

        self.active_ref_params = saved_ref_params;
        self.untyped_ref_params = saved_untyped_ref_params;
        self.active_external_ref_bindings = saved_external_ref_bindings;
        self.active_globals = saved_globals;
        self.active_statics = saved_statics;
        self.mixed_static_locals = saved_mixed_statics;
        self.foreach_key_locals = saved_foreach_keys;
        self.eval_barrier_active = saved_eval_barrier_active;
        self.break_continue_depth = saved_break_continue_depth;
        self.loop_break_envs = saved_loop_break_envs;
        self.finally_break_continue_bases = saved_finally_break_continue_bases;
        self.null_probe_scope_is_top_level = saved_null_probe_scope_is_top_level;
        self.exit_local_binding_scope(saved_local_binding_scope);

        result
    }
}

/// Returns the storage place a by-reference argument names, past named-argument wrappers.
fn by_ref_storage_place(arg: &Expr) -> &Expr {
    let mut arg = arg;
    while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
        arg = value;
    }
    arg
}

thread_local! {
    static PROGRAM_EVALUATES_CODE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Records whether the program being checked calls `eval()` (set per check, on the check thread).
pub(crate) fn set_program_evaluates_code(evaluates: bool) {
    PROGRAM_EVALUATES_CODE.with(|cell| cell.set(evaluates));
}

fn program_evaluates_code() -> bool {
    PROGRAM_EVALUATES_CODE.with(|cell| cell.get())
}

/// Whether a declared type is `callable` or `?callable`.
fn type_names_callable(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Named(name) => name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("callable"),
        TypeExpr::Nullable(inner) => type_names_callable(inner),
        _ => false,
    }
}
