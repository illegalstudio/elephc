//! Purpose:
//! Walks the whole AST in place, rewriting `func_num_args()`, `func_get_args()` and
//! `func_get_arg()` inside every function scope that supports them and adding that scope's
//! hidden `mixed ...$__elephc_func_args` parameter when at least one call was rewritten.
//!
//! Called from:
//! - `crate::func_args::desugar()`.
//!
//! Key details:
//! - Function scopes nest: a closure declared inside a function has its own argument frame,
//!   so `Rewriter::scope` is saved and restored around every function-like node instead of
//!   being inherited.
//! - Children are rewritten before their parent node, so `func_get_arg(func_num_args() - 1)`
//!   lowers the inner call first and the outer call sees a plain expression.
//! - The statement and expression matches are exhaustive (no wildcard arm). A new AST node
//!   must be handled here explicitly; a missed one would silently leave an introspection
//!   call unrewritten, and the checker would then report it as an undefined function.
//! - Parameter defaults, class constant initialisers, property defaults and enum case
//!   values are PHP constant expressions and cannot contain a function call, so they carry
//!   no introspection call to rewrite and are not walked.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{
    AttributeGroup, CallableTarget, ClassMethod, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind,
    TypeExpr,
};

use super::{build, IntrospectionCall, HIDDEN_ARGS_PARAM};

/// The argument frame of the function-like scope currently being walked.
struct Scope {
    /// Declared regular parameters, in declaration order, without the leading `$`.
    param_names: Vec<String>,
    /// The first declared parameter that carries a default value, if any. Such a scope
    /// cannot tell "passed" from "defaulted" through a variadic tail, so it is rejected.
    optional_param: Option<String>,
    /// The variadic parameter the source function declares itself, if any.
    source_variadic: Option<String>,
    /// Set once an introspection call was rewritten in this scope, which is what makes the
    /// hidden variadic parameter necessary.
    used: bool,
    /// Diagnostic label for this scope, e.g. `function 'va'`.
    label: String,
}

/// In-place AST rewriter for the three argument-introspection constructs.
pub(super) struct Rewriter {
    scope: Option<Scope>,
    errors: Vec<CompileError>,
}

impl Rewriter {
    /// Creates a rewriter positioned at top level, where no argument frame exists.
    pub(super) fn new() -> Self {
        Self {
            scope: None,
            errors: Vec::new(),
        }
    }

    /// Consumes the rewriter and returns every diagnostic collected during the walk.
    pub(super) fn into_errors(self) -> Vec<CompileError> {
        self.errors
    }

    /// Rewrites every statement in a body.
    pub(super) fn walk_stmts(&mut self, stmts: &mut [Stmt]) {
        for stmt in stmts.iter_mut() {
            self.walk_stmt(stmt);
        }
    }

    /// Rewrites one statement, recursing into every nested statement and expression.
    fn walk_stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.kind {
            // Statements with no expression and no nested body.
            StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::IncludeOnceMark { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::UseDecl { .. }
            | StmtKind::FunctionVariantGroup { .. }
            | StmtKind::FunctionVariantMark { .. }
            | StmtKind::Global { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => {}

            StmtKind::Echo(expr)
            | StmtKind::Throw(expr)
            | StmtKind::ExprStmt(expr)
            | StmtKind::Assign { value: expr, .. }
            | StmtKind::RefAssign { source: expr, .. }
            | StmtKind::TypedAssign { value: expr, .. }
            | StmtKind::ArrayPush { value: expr, .. }
            | StmtKind::ConstDecl { value: expr, .. }
            | StmtKind::ListUnpack { value: expr, .. }
            | StmtKind::StaticVar { init: expr, .. }
            | StmtKind::Include { path: expr, .. } => self.walk_expr(expr),

            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.walk_expr(value);
                }
            }
            StmtKind::ArrayAssign { index, value, .. } => {
                self.walk_expr(index);
                self.walk_expr(value);
            }
            StmtKind::NestedArrayAssign { target, value } => {
                self.walk_expr(target);
                self.walk_expr(value);
            }
            StmtKind::PropertyAssign { object, value, .. }
            | StmtKind::PropertyArrayPush { object, value, .. } => {
                self.walk_expr(object);
                self.walk_expr(value);
            }
            StmtKind::DynamicPropertyArrayPush {
                object,
                property,
                value,
            } => {
                self.walk_expr(object);
                self.walk_expr(property);
                self.walk_expr(value);
            }
            StmtKind::PropertyArrayAssign {
                object,
                index,
                value,
                ..
            } => {
                self.walk_expr(object);
                self.walk_expr(index);
                self.walk_expr(value);
            }
            StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. } => self.walk_expr(value),
            StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                self.walk_expr(index);
                self.walk_expr(value);
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.walk_expr(condition);
                self.walk_stmts(then_body);
                for (condition, body) in elseif_clauses.iter_mut() {
                    self.walk_expr(condition);
                    self.walk_stmts(body);
                }
                if let Some(body) = else_body {
                    self.walk_stmts(body);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                self.walk_stmts(then_body);
                if let Some(body) = else_body {
                    self.walk_stmts(body);
                }
            }
            StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
                self.walk_expr(condition);
                self.walk_stmts(body);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(init) = init {
                    self.walk_stmt(init);
                }
                if let Some(condition) = condition {
                    self.walk_expr(condition);
                }
                if let Some(update) = update {
                    self.walk_stmt(update);
                }
                self.walk_stmts(body);
            }
            StmtKind::Foreach { array, body, .. } => {
                self.walk_expr(array);
                self.walk_stmts(body);
            }
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
                self.walk_expr(subject);
                for (conditions, body) in cases.iter_mut() {
                    self.walk_exprs(conditions);
                    self.walk_stmts(body);
                }
                if let Some(body) = default {
                    self.walk_stmts(body);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                self.walk_stmts(try_body);
                for catch in catches.iter_mut() {
                    self.walk_stmts(&mut catch.body);
                }
                if let Some(body) = finally_body {
                    self.walk_stmts(body);
                }
            }
            StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => self.walk_stmts(body),
            StmtKind::FunctionDecl {
                name,
                params,
                param_attributes,
                variadic,
                variadic_type,
                body,
                ..
            } => {
                let label = format!("function '{}'", name);
                self.walk_function_scope(
                    label,
                    params,
                    Some(param_attributes),
                    variadic,
                    variadic_type,
                    body,
                );
            }
            StmtKind::ClassDecl {
                name,
                methods,
                constants,
                properties,
                ..
            }
            | StmtKind::TraitDecl {
                name,
                methods,
                constants,
                properties,
                ..
            }
            | StmtKind::InterfaceDecl {
                name,
                methods,
                constants,
                properties,
                ..
            } => {
                let _ = (constants, properties);
                self.walk_methods(name, methods);
            }
            StmtKind::EnumDecl { name, methods, .. } => self.walk_methods(name, methods),
        }
    }

    /// Rewrites every method body of a class, trait, interface or enum. Each method is its
    /// own argument frame.
    fn walk_methods(&mut self, owner: &str, methods: &mut [ClassMethod]) {
        for method in methods.iter_mut() {
            let label = format!("method '{}::{}'", owner, method.name);
            let ClassMethod {
                params,
                param_attributes,
                variadic,
                variadic_type,
                body,
                ..
            } = method;
            self.walk_function_scope(
                label,
                params,
                Some(param_attributes),
                variadic,
                variadic_type,
                body,
            );
        }
    }

    /// Walks a function-like body in its own argument frame and, if the body used one of
    /// the introspection constructs, appends the hidden `mixed ...$__elephc_func_args`
    /// parameter that collects the surplus positional arguments.
    ///
    /// `param_attributes` is `None` for closures, whose AST node carries no per-parameter
    /// attribute list; for every other scope it is kept aligned with `params` plus the one
    /// trailing entry the variadic parameter owns.
    fn walk_function_scope(
        &mut self,
        label: String,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
        param_attributes: Option<&mut Vec<Vec<AttributeGroup>>>,
        variadic: &mut Option<String>,
        variadic_type: &mut Option<TypeExpr>,
        body: &mut Vec<Stmt>,
    ) {
        let scope = Scope {
            param_names: params.iter().map(|(name, ..)| name.clone()).collect(),
            optional_param: params
                .iter()
                .find(|(_, _, default, _)| default.is_some())
                .map(|(name, ..)| name.clone()),
            source_variadic: variadic.clone(),
            used: false,
            label,
        };
        let outer = self.scope.replace(scope);
        self.walk_stmts(body);
        let scope = std::mem::replace(&mut self.scope, outer)
            .expect("function scope was installed before walking the body");
        if !scope.used {
            return;
        }
        *variadic = Some(HIDDEN_ARGS_PARAM.to_string());
        *variadic_type = Some(TypeExpr::Named(Name::unqualified("mixed")));
        if let Some(param_attributes) = param_attributes {
            if param_attributes.len() == params.len() {
                param_attributes.push(Vec::new());
            }
        }
    }

    /// Rewrites a list of expressions in source order.
    fn walk_exprs(&mut self, exprs: &mut [Expr]) {
        for expr in exprs.iter_mut() {
            self.walk_expr(expr);
        }
    }

    /// Rewrites one expression: children first, then the node itself when it is one of the
    /// three introspection calls.
    fn walk_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            // Leaves and identifier-only forms.
            ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::This
            | ExprKind::Variable(_)
            | ExprKind::PreIncrement(_)
            | ExprKind::PostIncrement(_)
            | ExprKind::PreDecrement(_)
            | ExprKind::PostDecrement(_)
            | ExprKind::ConstRef(_)
            | ExprKind::MagicConstant(_)
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::ClassConstant { .. }
            | ExprKind::ScopedConstantAccess { .. } => {}

            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Throw(inner)
            | ExprKind::Clone(inner)
            | ExprKind::ErrorSuppress(inner)
            | ExprKind::Print(inner)
            | ExprKind::Spread(inner)
            | ExprKind::YieldFrom(inner)
            | ExprKind::Cast { expr: inner, .. }
            | ExprKind::PtrCast { expr: inner, .. }
            | ExprKind::ObjectClassName { object: inner }
            | ExprKind::PropertyAccess { object: inner, .. }
            | ExprKind::NullsafePropertyAccess { object: inner, .. }
            | ExprKind::NamedArg { value: inner, .. } => self.walk_expr(inner),

            ExprKind::BinaryOp { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            ExprKind::NullCoalesce { value, default }
            | ExprKind::ShortTernary { value, default } => {
                self.walk_expr(value);
                self.walk_expr(default);
            }
            ExprKind::Pipe { value, callable } => {
                self.walk_expr(value);
                self.walk_expr(callable);
            }
            ExprKind::InstanceOf { value, target } => {
                self.walk_expr(value);
                if let InstanceOfTarget::Expr(target) = target {
                    self.walk_expr(target);
                }
            }
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                self.walk_stmts(prelude);
                self.walk_expr(target);
                self.walk_expr(value);
                if let Some(result_target) = result_target {
                    self.walk_expr(result_target);
                }
            }
            ExprKind::ArrayAccess { array, index } => {
                self.walk_expr(array);
                self.walk_expr(index);
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.walk_expr(condition);
                self.walk_expr(then_expr);
                self.walk_expr(else_expr);
            }
            ExprKind::ArrayLiteral(items) => self.walk_exprs(items),
            ExprKind::ArrayLiteralAssoc(pairs) => {
                for (key, value) in pairs.iter_mut() {
                    self.walk_expr(key);
                    self.walk_expr(value);
                }
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                self.walk_expr(subject);
                for (conditions, body) in arms.iter_mut() {
                    self.walk_exprs(conditions);
                    self.walk_expr(body);
                }
                if let Some(default) = default {
                    self.walk_expr(default);
                }
            }
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.walk_expr(object);
                self.walk_expr(property);
            }
            ExprKind::MethodCall { object, args, .. }
            | ExprKind::NullsafeMethodCall { object, args, .. } => {
                self.walk_expr(object);
                self.walk_exprs(args);
            }
            ExprKind::NullsafeDynamicMethodCall {
                object,
                method,
                args,
            } => {
                self.walk_expr(object);
                self.walk_expr(method);
                self.walk_exprs(args);
            }
            ExprKind::StaticMethodCall { args, .. }
            | ExprKind::NewScopedObject { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::ClosureCall { args, .. } => self.walk_exprs(args),
            ExprKind::NewDynamic { name_expr, args } => {
                self.walk_expr(name_expr);
                self.walk_exprs(args);
            }
            ExprKind::NewDynamicObject {
                class_name, args, ..
            } => {
                self.walk_expr(class_name);
                self.walk_exprs(args);
            }
            ExprKind::ExprCall { callee, args } => {
                self.walk_expr(callee);
                self.walk_exprs(args);
            }
            ExprKind::BufferNew { len, .. } => self.walk_expr(len),
            ExprKind::Yield { key, value } => {
                if let Some(key) = key {
                    self.walk_expr(key);
                }
                if let Some(value) = value {
                    self.walk_expr(value);
                }
            }
            // Transient: the resolver expands `IncludeValue` before this pass runs. Recurse
            // into the path expression so the walk stays exhaustive.
            ExprKind::IncludeValue { path, .. } => self.walk_expr(path),
            ExprKind::FirstClassCallable(target) => {
                match target {
                    CallableTarget::Function(name) => {
                        if let Some(call) = IntrospectionCall::from_name(name) {
                            self.errors.push(CompileError::new(
                                expr.span,
                                &format!("Cannot call {}() dynamically", call.php_name()),
                            ));
                        }
                    }
                    CallableTarget::StaticMethod { .. } => {}
                    CallableTarget::Method { object, .. } => self.walk_expr(object),
                }
                return;
            }
            ExprKind::Closure {
                params,
                variadic,
                variadic_type,
                body,
                ..
            } => {
                self.walk_function_scope(
                    "closure".to_string(),
                    params,
                    None,
                    variadic,
                    variadic_type,
                    body,
                );
                return;
            }
            ExprKind::FunctionCall { args, .. } => self.walk_exprs(args),
        }

        self.try_rewrite_call(expr);
    }

    /// Replaces `expr` in place when it is a call to one of the three introspection
    /// constructs, recording a diagnostic instead when the enclosing scope cannot support
    /// it. Any other expression is left untouched.
    fn try_rewrite_call(&mut self, expr: &mut Expr) {
        let ExprKind::FunctionCall { name, args } = &expr.kind else {
            return;
        };
        let Some(call) = IntrospectionCall::from_name(name) else {
            return;
        };
        let args = args.clone();
        match self.scope_replacement(call, &args, expr.span) {
            Ok(kind) => expr.kind = kind,
            Err(error) => self.errors.push(error),
        }
    }

    /// Validates that `call` can be rewritten in the current scope and, if so, marks the
    /// scope as needing the hidden variadic parameter and builds the replacement.
    ///
    /// Every rejected shape produces a diagnostic instead of a silently different answer:
    /// PHP's own "must be called from a function context" fatal, and the two argument-frame
    /// shapes elephc cannot reconstruct from a variadic tail (optional parameters, whose
    /// "passed" vs "defaulted" status is not recoverable, and a source-declared variadic,
    /// whose contents the body may have reassigned).
    fn scope_replacement(
        &mut self,
        call: IntrospectionCall,
        args: &[Expr],
        span: crate::span::Span,
    ) -> Result<ExprKind, CompileError> {
        if args.len() != call.arity() {
            return Err(CompileError::new(
                span,
                &format!(
                    "{}() expects {} arguments, got {}",
                    call.php_name(),
                    call.arity(),
                    args.len()
                ),
            ));
        }
        if args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
        {
            return Err(CompileError::new(
                span,
                &format!(
                    "{}() does not accept named or unpacked arguments",
                    call.php_name()
                ),
            ));
        }
        let Some(scope) = self.scope.as_mut() else {
            return Err(CompileError::new(
                span,
                &format!(
                    "{}() must be called from a function context",
                    call.php_name()
                ),
            ));
        };
        if let Some(variadic) = &scope.source_variadic {
            return Err(CompileError::new(
                span,
                &format!(
                    "{}() is not supported in {}: it declares the variadic parameter ${} — read that parameter directly",
                    call.php_name(),
                    scope.label,
                    variadic
                ),
            ));
        }
        if let Some(optional) = &scope.optional_param {
            return Err(CompileError::new(
                span,
                &format!(
                    "{}() is not supported in {}: parameter ${} has a default value, so elephc cannot tell a passed argument from a defaulted one",
                    call.php_name(),
                    scope.label,
                    optional
                ),
            ));
        }
        scope.used = true;
        let param_names = scope.param_names.clone();
        Ok(build::replacement(call, &param_names, args, span))
    }
}
