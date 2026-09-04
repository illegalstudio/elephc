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
//! - A source variadic is copied into an internal positional-only snapshot before user code.
//!   String-keyed named variadic entries are excluded because PHP omits them from `func_*`.
//! - Children are rewritten before their parent node, so `func_get_arg(func_num_args() - 1)`
//!   lowers the inner call first and the outer call sees a plain expression.
//! - The statement and expression matches are exhaustive (no wildcard arm). A new AST node
//!   must be handled here explicitly; a missed one would silently leave an introspection
//!   call unrewritten, and the checker would then report it as an undefined function.
//! - Parameter defaults, class constant initialisers, property defaults and enum case
//!   values are PHP constant expressions and cannot contain a function call, so they carry
//!   no introspection call to rewrite and are not walked.

use crate::errors::CompileError;
use crate::names::{Name, NameKind};
use crate::parser::ast::{
    AttributeGroup, CallableTarget, ClassMethod, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind,
    TypeExpr,
};

use super::{build, IntrospectionCall, HIDDEN_ARGC_PARAM, HIDDEN_ARGS_PARAM};

/// The argument frame of the function-like scope currently being walked.
struct Scope {
    /// Declared regular parameters, in declaration order, without the leading `$`.
    param_names: Vec<String>,
    /// The first declared parameter that carries a default value, if any.
    optional_param: Option<String>,
    /// The variadic parameter the source function declares itself, if any.
    source_variadic: Option<String>,
    /// Set once an introspection call was rewritten in this scope, which is what makes the
    /// hidden variadic parameter necessary.
    used: bool,
}

/// In-place AST rewriter for the three argument-introspection constructs.
pub(super) struct Rewriter {
    scope: Option<Scope>,
    errors: Vec<CompileError>,
    capture_all_frames: bool,
    rewrite_introspection: bool,
    saw_backtrace: bool,
}

impl Rewriter {
    /// Creates a rewriter positioned at top level with the requested frame-capture mode.
    pub(super) fn new(capture_all_frames: bool, rewrite_introspection: bool) -> Self {
        Self {
            scope: None,
            errors: Vec::new(),
            capture_all_frames,
            rewrite_introspection,
            saw_backtrace: false,
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

    /// Walks a function-like body in its own argument frame and installs its argument snapshot.
    ///
    /// `param_attributes` is `None` for closures, whose AST node carries no per-parameter
    /// attribute list; for every other scope it is kept aligned with `params` plus the one
    /// trailing entry the variadic parameter owns.
    fn walk_function_scope(
        &mut self,
        _label: String,
        params: &mut Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
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
            used: self.capture_all_frames,
        };
        let outer = self.scope.replace(scope);
        self.walk_stmts(body);
        let scope = std::mem::replace(&mut self.scope, outer)
            .expect("function scope was installed before walking the body");
        if !scope.used {
            return;
        }
        if let Some(source_variadic) = scope.source_variadic {
            let count_metadata = scope.optional_param.is_some();
            let span = body
                .first()
                .map(|statement| statement.span)
                .unwrap_or_else(crate::span::Span::dummy);
            let snapshot = source_variadic_snapshot(&source_variadic, count_metadata, span);
            body.splice(0..0, snapshot);
            if count_metadata {
                let attribute_index = params.len();
                params.push((
                    HIDDEN_ARGC_PARAM.to_string(),
                    Some(TypeExpr::Int),
                    Some(Expr::new(ExprKind::IntLiteral(0), span)),
                    false,
                ));
                if let Some(param_attributes) = param_attributes {
                    param_attributes.insert(attribute_index, Vec::new());
                }
            }
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

    /// Replaces direct and literal `call_user_func*` invocations of the three introspection
    /// constructs, recording a diagnostic when the enclosing scope cannot support them.
    fn try_rewrite_call(&mut self, expr: &mut Expr) {
        let ExprKind::FunctionCall { name, args } = &expr.kind else {
            return;
        };
        if name.last_segment().is_some_and(|name| {
            name.eq_ignore_ascii_case("debug_backtrace")
                || name.eq_ignore_ascii_case("debug_print_backtrace")
        }) {
            self.saw_backtrace = true;
        }
        if !self.rewrite_introspection {
            return;
        }
        let replacement = IntrospectionCall::from_name(name)
            .map(|call| (call, args.clone()))
            .or_else(|| literal_call_user_func_introspection(name, args));
        let Some((call, args)) = replacement else {
            return;
        };
        match self.scope_replacement(call, &args, expr.span) {
            Ok(kind) => expr.kind = kind,
            Err(error) => self.errors.push(error),
        }
    }

    /// Validates that `call` can be rewritten in the current scope and, if so, marks the
    /// scope as needing the hidden variadic parameter and builds the replacement.
    ///
    /// Every rejected shape produces a diagnostic instead of a silently different answer.
    /// Optional parameters use the hidden collector's passed-count metadata unless the source
    /// already owns the variadic slot, a combination which still has no count channel.
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
        scope.used = true;
        let param_names = scope.param_names.clone();
        Ok(build::replacement(
            call,
            &param_names,
            args,
            scope.optional_param.is_some(),
            span,
        ))
    }
}

/// Extracts PHP's special literal `call_user_func*('func_*', ...)` call shapes.
fn literal_call_user_func_introspection(
    name: &Name,
    args: &[Expr],
) -> Option<(IntrospectionCall, Vec<Expr>)> {
    let function = name.last_segment()?;
    let ExprKind::StringLiteral(callback) = &args.first()?.kind else {
        return None;
    };
    let call = IntrospectionCall::from_segment(callback)?;
    if function.eq_ignore_ascii_case("call_user_func") {
        return Some((call, args[1..].to_vec()));
    }
    if !function.eq_ignore_ascii_case("call_user_func_array") || args.len() != 2 {
        return None;
    }
    match &args[1].kind {
        ExprKind::ArrayLiteral(values) => Some((call, values.clone())),
        ExprKind::ArrayLiteralAssoc(entries)
            if entries
                .iter()
                .all(|(key, _)| matches!(key.kind, ExprKind::IntLiteral(_))) =>
        {
            Some((call, entries.iter().map(|(_, value)| value.clone()).collect()))
        }
        _ => None,
    }
}

/// Returns whether the resolved program contains a direct Core backtrace call.
pub(super) fn program_uses_backtrace(program: &[Stmt]) -> bool {
    let mut scratch = program.to_vec();
    let mut detector = Rewriter::new(false, false);
    detector.walk_stmts(&mut scratch);
    detector.saw_backtrace
}

/// Builds an entry-time positional snapshot of a source-declared variadic parameter.
fn source_variadic_snapshot(
    source_variadic: &str,
    count_metadata: bool,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let initial = if count_metadata {
        vec![Expr::new(
            ExprKind::Variable(HIDDEN_ARGC_PARAM.to_string()),
            span,
        )]
    } else {
        Vec::new()
    };
    let snapshot_init = Stmt::new(
        StmtKind::Assign {
            name: HIDDEN_ARGS_PARAM.to_string(),
            value: Expr::new(ExprKind::ArrayLiteral(initial), span),
        },
        span,
    );
    let key_name = "__elephc_func_arg_key".to_string();
    let value_name = "__elephc_func_arg_value".to_string();
    let key = Expr::new(ExprKind::Variable(key_name.clone()), span);
    let is_positional = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec!["is_int".to_string()]),
            args: vec![key],
        },
        span,
    );
    let append = Stmt::new(
        StmtKind::ArrayPush {
            array: HIDDEN_ARGS_PARAM.to_string(),
            value: Expr::new(ExprKind::Variable(value_name.clone()), span),
        },
        span,
    );
    let snapshot_positional = Stmt::new(
        StmtKind::Foreach {
            array: Expr::new(ExprKind::Variable(source_variadic.to_string()), span),
            key_var: Some(key_name),
            value_var: value_name,
            value_by_ref: false,
            body: vec![Stmt::new(
                StmtKind::If {
                    condition: is_positional,
                    then_body: vec![append],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                span,
            )],
        },
        span,
    );
    vec![snapshot_init, snapshot_positional]
}
